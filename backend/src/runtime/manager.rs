use anyhow::{Ok, Result, bail};
use serde::Serialize;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

use crate::{
    configuration::configuration::Configuration,
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

impl RuntimesManager {
    pub fn new() -> Result<Self> {
        let persistence = Persistence::new()?;
        persistence.init()?;

        Ok(Self {
            persistence: persistence,
            runtimes: Arc::new(RwLock::new(HashMap::new())),
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

        let manager = Arc::new(StateManager::new());
        let mqtt_receiver = MqttReceiver::new(config.clone(), manager.clone());
        let publisher = Publisher::new(manager.clone());

        let runtime = Runtime {
            runtime_id: config.id.clone(),
            config: config.clone(),
            state_manager: manager,
            mqtt_receiver,
            publisher,
            state: RuntimeState::Stopped,
        };

        let state = runtime.state.clone();

        runtimes.insert(config.id.clone(), runtime);

        tracing::info!("registered runtime '{}'", config.id);

        //Todo insert into database

        Ok(SystemInfo {
            config,
            state: state,
        })
    }

    pub async fn remove(&self, id: String) -> Result<()> {
        tracing::info!("removing runtime '{}'", id);
        let mut runtimes = self.runtimes.write().await;

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
