use anyhow::{Ok, Result, bail};
use serde::Serialize;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

use crate::{
    configuration::configuration::Configuration,
    mqtt::receiver::MqttReceiver,
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
        self.mqtt_receiver.start().await?;
        self.publisher.start()?;
        Ok(())
    }
    pub async fn stop(&self) {}
}

#[derive(Clone)]
pub struct RuntimesManager {
    pub runtimes: Arc<RwLock<HashMap<String, Runtime>>>,
}

impl RuntimesManager {
    pub fn new() -> Self {
        Self {
            runtimes: Arc::new(RwLock::new(HashMap::new())),
        }
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

        //Todo insert into database

        Ok(SystemInfo {
            config,
            state: state,
        })
    }

    pub async fn remove(&self, id: String) -> Result<()> {
        let mut runtimes = self.runtimes.write().await;

        if let Some(runtime) = runtimes.get(&id) {
            runtime.stop().await;
        }

        _ = runtimes.remove(&id);
        Ok(())
    }

    pub async fn start(&self, id: String) -> Result<()> {
        let runtimes = self.runtimes.read().await;

        if let Some(runtime) = runtimes.get(&id) {
            runtime.start().await;
        }

        Ok(())
    }

    pub async fn stop(&self, id: String) -> Result<()> {
        let runtimes = self.runtimes.read().await;

        if let Some(runtime) = runtimes.get(&id) {
            runtime.stop().await;
        }

        Ok(())
    }
}
