use std::{net::ToSocketAddrs, path::PathBuf};

use clap::{Args, Parser};

#[derive(Parser)]
#[command(name = "rustrack", version, about = "VDA5050 fleet tracking server")]
pub struct Cli {
    #[command(flatten)]
    pub server: ServerArgs,

    #[cfg(windows)]
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Args)]
pub struct ServerArgs {
    /// Address to listen on, as HOST:PORT
    #[arg(
        short,
        long,
        global = true,
        default_value = "0.0.0.0:3000",
        value_parser = parse_listen
    )]
    pub listen: String,

    /// Directory for the database [default: the working directory, or
    /// %ProgramData%\Rustrack when running as a Windows service]
    #[arg(long, global = true)]
    pub data_dir: Option<PathBuf>,
}

#[cfg(windows)]
#[derive(clap::Subcommand)]
pub enum Command {
    /// Manage the Windows service
    #[command(subcommand)]
    Service(ServiceCommand),
}

#[cfg(windows)]
#[derive(clap::Subcommand)]
pub enum ServiceCommand {
    /// Register the service with --listen and --data-dir, and start it
    Install,
    /// Stop and remove the service. Stored data is kept
    Uninstall,
    /// Run as the service; only the Windows service manager should call this
    Run,
}

/// Reject a bad address when it is given, rather than when something later
/// tries to bind it -- for a service that is well after the install finished.
fn parse_listen(value: &str) -> Result<String, String> {
    value
        .to_socket_addrs()
        .map(|_| value.to_owned())
        .map_err(|e| format!("expected HOST:PORT: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_all_interfaces_on_port_3000() {
        let cli = Cli::parse_from(["rustrack"]);
        assert_eq!(cli.server.listen, "0.0.0.0:3000");
        assert_eq!(cli.server.data_dir, None);
    }

    #[test]
    fn accepts_ipv4_ipv6_and_hostnames() {
        for address in ["127.0.0.1:8080", "[::1]:8080", "localhost:8080"] {
            let cli = Cli::parse_from(["rustrack", "--listen", address]);
            assert_eq!(cli.server.listen, address);
        }
    }

    #[test]
    fn rejects_an_address_without_a_port() {
        assert!(Cli::try_parse_from(["rustrack", "--listen", "127.0.0.1"]).is_err());
    }
}
