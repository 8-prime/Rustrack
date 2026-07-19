use anyhow::{Ok, Result, bail};
use serde::Serialize;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

use crate::{
    configuration::configuration::{Configuration, ConfigurationFields},
    mqtt::receiver::MqttReceiver,
    persistence::Persistence,
    runtime::{publisher::Publisher, state::StateManager},
};

#[derive(Clone, Serialize)]
pub enum RuntimeState {
    Running,
    Stopped,
}

pub struct Runtime {
    pub runtime_id: String,
    pub config: Configuration,
    pub state_manager: Arc<StateManager>,
    pub mqtt_receiver: MqttReceiver,
    pub publisher: Publisher,
    pub state: RuntimeState,
}

#[derive(Clone, Serialize)]
pub struct SystemInfo {
    pub config: Configuration,
    pub state: RuntimeState,
}

impl Runtime {
    pub async fn start(&mut self) -> anyhow::Result<()> {
        if matches!(self.state, RuntimeState::Running) {
            tracing::debug!(
                "runtime '{}' already running, ignoring start",
                self.runtime_id
            );
            return Ok(());
        }
        tracing::info!("starting runtime '{}'", self.runtime_id);
        self.mqtt_receiver.start().await?;
        self.publisher.start()?;
        self.state = RuntimeState::Running;
        tracing::info!("runtime '{}' started", self.runtime_id);
        Ok(())
    }

    pub async fn stop(&mut self) -> anyhow::Result<()> {
        if matches!(self.state, RuntimeState::Stopped) {
            tracing::debug!(
                "runtime '{}' already stopped, ignoring stop",
                self.runtime_id
            );
            return Ok(());
        }
        tracing::info!("stopping runtime '{}'", self.runtime_id);
        self.mqtt_receiver.stop().await?;
        self.publisher.stop().await?;
        self.state = RuntimeState::Stopped;
        tracing::info!("runtime '{}' stopped", self.runtime_id);
        Ok(())
    }
}

pub struct RuntimesManager {
    persistence: Persistence,
    pub runtimes: Arc<RwLock<HashMap<String, Runtime>>>,
}

/// Build the object graph backing a runtime. Shared by `add`, `update` and the
/// startup restore, all of which need a receiver and publisher wired to a fresh
/// state manager.
fn build_runtime(config: Configuration, state: RuntimeState) -> Runtime {
    let manager = Arc::new(StateManager::new());
    let mqtt_receiver = MqttReceiver::new(config.clone(), manager.clone());
    let publisher = Publisher::new(manager.clone());

    Runtime {
        runtime_id: config.id.clone(),
        config,
        state_manager: manager,
        mqtt_receiver,
        publisher,
        state,
    }
}

impl RuntimesManager {
    pub fn new() -> Result<Self> {
        let persistence = Persistence::new()?;
        persistence.init()?;

        // Rehydrate persisted configurations. Restored runtimes come back
        // Stopped, matching how a freshly created one behaves.
        let restored: HashMap<String, Runtime> = persistence
            .read_configurations()?
            .into_iter()
            .map(|config| {
                (
                    config.id.clone(),
                    build_runtime(config, RuntimeState::Stopped),
                )
            })
            .collect();

        tracing::info!("restored {} runtime(s) from persistence", restored.len());

        Ok(Self {
            persistence: persistence,
            runtimes: Arc::new(RwLock::new(restored)),
        })
    }

    pub async fn system_configs(&self) -> anyhow::Result<Vec<SystemInfo>> {
        let runtimes = self.runtimes.read().await;

        Ok(runtimes
            .values()
            .map(|r| SystemInfo {
                config: r.config.clone(),
                state: r.state.clone(),
            })
            .collect())
    }

    pub async fn add(&self, config: Configuration) -> Result<SystemInfo> {
        let mut runtimes = self.runtimes.write().await;

        if runtimes.contains_key(&config.id) {
            bail!("runtime '{}' already exists", config.id);
        }

        // Persist before touching memory so a failed write cannot leave a
        // phantom runtime behind.
        self.persistence.add_configuration(config.clone())?;

        let runtime = build_runtime(config.clone(), RuntimeState::Stopped);
        let state = runtime.state.clone();

        runtimes.insert(config.id.clone(), runtime);

        tracing::info!("registered runtime '{}'", config.id);

        Ok(SystemInfo {
            config,
            state: state,
        })
    }

    pub async fn update(&self, id: String, fields: ConfigurationFields) -> Result<SystemInfo> {
        tracing::info!("update requested for runtime '{}'", id);
        let mut runtimes = self.runtimes.write().await;

        let Some(existing) = runtimes.get(&id) else {
            tracing::warn!("update requested for unknown runtime '{}'", id);
            bail!("runtime '{}' does not exist", id);
        };

        let config = existing.config.with_fields(fields);
        let was_running = matches!(existing.state, RuntimeState::Running);

        // Persist before tearing anything down, so a failed write leaves the
        // current runtime untouched.
        self.persistence.update_configuration(config.clone())?;

        // MqttReceiver copies the connection settings at construction time and
        // never re-reads them, so the runtime has to be rebuilt rather than
        // mutated in place.
        if was_running && let Some(runtime) = runtimes.get_mut(&id) {
            runtime.stop().await?;
        }

        let mut runtime = build_runtime(config.clone(), RuntimeState::Stopped);
        if was_running {
            runtime.start().await?;
        }

        let state = runtime.state.clone();
        runtimes.insert(id.clone(), runtime);

        tracing::info!("updated runtime '{}'", id);

        Ok(SystemInfo { config, state })
    }

    pub async fn remove(&self, id: String) -> Result<()> {
        tracing::info!("removing runtime '{}'", id);
        let mut runtimes = self.runtimes.write().await;

        // Delete from the database first; if that fails the in-memory runtime
        // stays intact rather than silently reappearing on the next restart.
        self.persistence.delete_configuration(id.clone())?;

        if let Some(runtime) = runtimes.get_mut(&id) {
            runtime.stop().await?;
        }

        _ = runtimes.remove(&id);
        Ok(())
    }

    pub async fn start(&self, id: String) -> Result<()> {
        tracing::info!("start requested for runtime '{}'", id);
        let mut runtimes = self.runtimes.write().await;

        let Some(runtime) = runtimes.get_mut(&id) else {
            tracing::warn!("start requested for unknown runtime '{}'", id);
            bail!("runtime '{}' does not exist", id);
        };
        runtime.start().await?;

        Ok(())
    }

    pub async fn stop(&self, id: String) -> Result<()> {
        tracing::info!("stop requested for runtime '{}'", id);
        let mut runtimes = self.runtimes.write().await;

        let Some(runtime) = runtimes.get_mut(&id) else {
            tracing::warn!("stop requested for unknown runtime '{}'", id);
            bail!("runtime '{}' does not exist", id);
        };
        runtime.stop().await?;

        Ok(())
    }
}
