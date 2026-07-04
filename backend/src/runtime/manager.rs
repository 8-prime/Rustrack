use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, Utc};
use std::{collections::HashMap, sync::RwLock};

use crate::{
    configuration::configuration::Configuration,
    mqtt::receiver::MqttReceiver,
    runtime::{publisher::Publisher, state::StateManager},
};

pub struct Runtime {
    pub runtime_id: String,
    pub state_manager: Statemanager,
    pub mqtt_receiver: MqttReceiver,
    pub publisher: Publisher,
}

pub struct RuntimesManager {
    pub runtimes: RwLock<HashMap<String, Runtime>>,
}

impl RuntimesManager {
    pub fn new() -> Self {
        Self {
            runtimes: RwLock::new(HashMap::new()),
        }
    }

    pub async fn add(&self, config: Configuration) -> Result<()> {
        let mut runtimes = self
            .runtimes
            .write()
            .map_err(|_| anyhow!("runtimes lock poisoned"))?;

        if runtimes.contains_key(&config.id) {
            bail!("runtime '{}' already exists", config.id);
        }

        let manager = StateManager::new();
        let mqtt_receiver = MqttReceiver::new(&config);
        let publisher = Publisher::new(&manager);

        runtimes.insert(
            config.id,
            Runtime {
                runtime_id: config.id,
                state_manager: manager,
                mqtt_receiver,
                publisher,
            },
        );

        Ok(())
    }
}
