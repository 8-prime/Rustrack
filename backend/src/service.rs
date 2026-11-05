//! Running as a Windows service.
//!
//! A service has no console and starts in `System32`, so unlike a console run
//! it logs to a file and keeps its data under `%ProgramData%`.

use std::{
    env,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    sync::OnceLock,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, bail};
use tokio_util::sync::CancellationToken;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use windows_service::{
    Error as ServiceError, define_windows_service,
    service::{
        ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceExitCode,
        ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle},
    service_dispatcher,
    service_manager::{ServiceManager, ServiceManagerAccess},
};

use crate::{
    cli::{ServerArgs, ServiceCommand},
    log_filter,
    server::Server,
};

const SERVICE_NAME: &str = "Rustrack";
const SERVICE_DESCRIPTION: &str = "Tracks VDA5050 fleets and serves the Rustrack web interface.";
const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;

/// How long startup and shutdown may take before the service manager gives up.
const PENDING_WAIT_HINT: Duration = Duration::from_secs(30);
/// How long `install` and `uninstall` wait for the service to change state.
const STATE_CHANGE_TIMEOUT: Duration = Duration::from_secs(30);
const STATE_POLL_INTERVAL: Duration = Duration::from_millis(250);

const LOG_FILES_KEPT: usize = 14;

const ERROR_SERVICE_DOES_NOT_EXIST: i32 = 1060;

/// `service_main` is called by the service manager through a fixed signature,
/// so the parsed arguments have to reach it out of band.
static ARGS: OnceLock<ServerArgs> = OnceLock::new();

define_windows_service!(ffi_service_main, service_main);

pub fn dispatch(command: ServiceCommand, args: ServerArgs) -> anyhow::Result<()> {
    match command {
        ServiceCommand::Install => install(&args),
        ServiceCommand::Uninstall => uninstall(),
        ServiceCommand::Run => {
            let _ = ARGS.set(args);
            service_dispatcher::start(SERVICE_NAME, ffi_service_main)
                .context("connecting to the service manager (run with `service install` instead)")
        }
    }
}

fn data_dir(args: &ServerArgs) -> PathBuf {
    args.data_dir.clone().unwrap_or_else(|| {
        env::var_os("ProgramData")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
            .join(SERVICE_NAME)
    })
}

fn logs_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("logs")
}

fn install(args: &ServerArgs) -> anyhow::Result<()> {
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )
    .context("connecting to the service manager (is this an elevated prompt?)")?;

    let mut launch_arguments: Vec<OsString> = vec![
        "service".into(),
        "run".into(),
        "--listen".into(),
        args.listen.clone().into(),
    ];
    // The service won't share this process's working directory, so a relative
    // path has to be pinned down now.
    if let Some(data_dir) = &args.data_dir {
        launch_arguments.push("--data-dir".into());
        launch_arguments.push(std::path::absolute(data_dir)?.into());
    }

    let service = manager
        .create_service(
            &ServiceInfo {
                name: SERVICE_NAME.into(),
                display_name: SERVICE_NAME.into(),
                service_type: SERVICE_TYPE,
                start_type: ServiceStartType::AutoStart,
                error_control: ServiceErrorControl::Normal,
                executable_path: env::current_exe()?,
                launch_arguments,
                dependencies: vec![],
                account_name: None, // LocalSystem
                account_password: None,
            },
            ServiceAccess::CHANGE_CONFIG | ServiceAccess::START | ServiceAccess::QUERY_STATUS,
        )
        .context("registering the service")?;
    service.set_description(SERVICE_DESCRIPTION)?;

    service
        .start::<&OsStr>(&[])
        .context("starting the service")?;

    let deadline = Instant::now() + STATE_CHANGE_TIMEOUT;
    loop {
        match service.query_status()?.current_state {
            ServiceState::Running => break,
            ServiceState::Stopped => bail!(
                "the service was registered but stopped during startup; see the logs in {}",
                logs_dir(&data_dir(args)).display()
            ),
            _ if Instant::now() > deadline => bail!("the service did not finish starting in time"),
            _ => thread::sleep(STATE_POLL_INTERVAL),
        }
    }

    println!(
        "{SERVICE_NAME} service installed, listening on {}",
        args.listen
    );
    Ok(())
}

fn uninstall() -> anyhow::Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .context("connecting to the service manager (is this an elevated prompt?)")?;

    let access = ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE;
    let service = match manager.open_service(SERVICE_NAME, access) {
        Ok(service) => service,
        Err(ServiceError::Winapi(e)) if e.raw_os_error() == Some(ERROR_SERVICE_DOES_NOT_EXIST) => {
            println!("{SERVICE_NAME} service is not installed");
            return Ok(());
        }
        Err(e) => return Err(e).context("opening the service"),
    };

    if service.query_status()?.current_state != ServiceState::Stopped {
        service.stop().context("stopping the service")?;

        // Callers delete the executable next, which fails while it is running.
        let deadline = Instant::now() + STATE_CHANGE_TIMEOUT;
        while service.query_status()?.current_state != ServiceState::Stopped {
            if Instant::now() > deadline {
                bail!("the service did not stop in time");
            }
            thread::sleep(STATE_POLL_INTERVAL);
        }
    }

    service.delete().context("removing the service")?;
    println!("{SERVICE_NAME} service removed");
    Ok(())
}

fn service_main(_arguments: Vec<OsString>) {
    // Stdout goes nowhere under the service manager, so nothing can be reported
    // until logging is up; a failure before that only sets the exit code.
    let args = ARGS.get().expect("set before the dispatcher starts");
    let data_dir = data_dir(args);
    if let Err(e) = run(args, data_dir) {
        tracing::error!("service failed: {e:#}");
    }
}

fn run(args: &ServerArgs, data_dir: PathBuf) -> anyhow::Result<()> {
    let shutdown = CancellationToken::new();
    let status_handle = service_control_handler::register(SERVICE_NAME, {
        let shutdown = shutdown.clone();
        move |control| match control {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                shutdown.cancel();
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    })?;

    report(&status_handle, ServiceState::StartPending, 0)?;
    let result = serve(args, &data_dir, shutdown, &status_handle);
    report(
        &status_handle,
        ServiceState::Stopped,
        u32::from(result.is_err()),
    )?;
    result
}

fn serve(
    args: &ServerArgs,
    data_dir: &Path,
    shutdown: CancellationToken,
    status_handle: &ServiceStatusHandle,
) -> anyhow::Result<()> {
    let logs = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .max_log_files(LOG_FILES_KEPT)
        .filename_prefix("rustrack")
        .filename_suffix("log")
        .build(logs_dir(data_dir))
        .context("opening the log directory")?;
    tracing_subscriber::fmt()
        .with_env_filter(log_filter())
        .with_ansi(false)
        .with_writer(logs)
        .init();

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let server = Server::bind(&args.listen, data_dir).await?;
        report(status_handle, ServiceState::Running, 0)?;
        let result = server.serve(shutdown.clone().cancelled_owned()).await;
        // Reached on a failed serve as well as a requested stop.
        report(status_handle, ServiceState::StopPending, 0)?;
        result
    })
}

fn report(handle: &ServiceStatusHandle, state: ServiceState, exit_code: u32) -> anyhow::Result<()> {
    let pending = matches!(
        state,
        ServiceState::StartPending | ServiceState::StopPending
    );
    handle.set_service_status(ServiceStatus {
        service_type: SERVICE_TYPE,
        current_state: state,
        controls_accepted: match state {
            ServiceState::Running => ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
            _ => ServiceControlAccept::empty(),
        },
        exit_code: ServiceExitCode::Win32(exit_code),
        checkpoint: u32::from(pending),
        wait_hint: if pending {
            PENDING_WAIT_HINT
        } else {
            Duration::ZERO
        },
        process_id: None,
    })?;
    Ok(())
}
