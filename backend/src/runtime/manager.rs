use anyhow::{Context, Ok, Result, bail};
use bytes::Bytes;
use chrono::Utc;
use serde::Serialize;
use shared::lif::{Lif, LifSummary};
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
    pub lif: Arc<RwLock<Option<LifSummary>>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub config: Configuration,
    pub state: RuntimeState,
    pub lif: Option<LifSummary>,
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
    persistence: Arc<Persistence>,
    pub runtimes: Arc<RwLock<HashMap<String, Runtime>>>,
}

/// Build the object graph backing a runtime. Shared by `add`, `update` and the
/// startup restore, all of which need a receiver and publisher wired to a fresh
/// state manager.
///
/// `lif` is threaded through rather than defaulted so `update` can carry an
/// existing layout across the rebuild.
fn build_runtime(config: Configuration, state: RuntimeState, lif: Option<LifSummary>) -> Runtime {
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
        lif: Arc::new(RwLock::new(lif)),
    }
}

impl RuntimesManager {
    pub fn new() -> Result<Self> {
        let persistence = Persistence::new()?;
        persistence.init()?;

        let mut summaries: HashMap<String, LifSummary> =
            persistence.read_all_lif_summaries()?.into_iter().collect();

        let restored: HashMap<String, Runtime> = persistence
            .read_configurations()?
            .into_iter()
            .map(|config| {
                let lif = summaries.remove(&config.id);
                (
                    config.id.clone(),
                    build_runtime(config, RuntimeState::Stopped, lif),
                )
            })
            .collect();

        tracing::info!("restored {} runtime(s) from persistence", restored.len());

        Ok(Self {
            persistence: Arc::new(persistence),
            runtimes: Arc::new(RwLock::new(restored)),
        })
    }

    pub async fn system_configs(&self) -> anyhow::Result<Vec<SystemInfo>> {
        let runtimes = self.runtimes.read().await;

        let mut infos = Vec::with_capacity(runtimes.len());
        for r in runtimes.values() {
            infos.push(SystemInfo {
                config: r.config.clone(),
                state: r.state.clone(),
                lif: r.lif.read().await.clone(),
            });
        }
        Ok(infos)
    }

    pub async fn add(&self, config: Configuration) -> Result<SystemInfo> {
        let mut runtimes = self.runtimes.write().await;

        if runtimes.contains_key(&config.id) {
            bail!("runtime '{}' already exists", config.id);
        }

        // Persist before touching memory so a failed write cannot leave a
        // phantom runtime behind.
        self.persistence.add_configuration(config.clone())?;

        let runtime = build_runtime(config.clone(), RuntimeState::Stopped, None);
        let state = runtime.state.clone();

        runtimes.insert(config.id.clone(), runtime);

        tracing::info!("registered runtime '{}'", config.id);

        Ok(SystemInfo {
            config,
            state: state,
            lif: None,
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
        // Carry the layout across the rebuild — editing broker settings must
        // not silently drop a system's map.
        let existing_lif = existing.lif.read().await.clone();

        // Persist before tearing anything down, so a failed write leaves the
        // current runtime untouched.
        self.persistence.update_configuration(config.clone())?;

        // MqttReceiver copies the connection settings at construction time and
        // never re-reads them, so the runtime has to be rebuilt rather than
        // mutated in place.
        if was_running && let Some(runtime) = runtimes.get_mut(&id) {
            runtime.stop().await?;
        }

        let mut runtime =
            build_runtime(config.clone(), RuntimeState::Stopped, existing_lif.clone());
        if was_running {
            runtime.start().await?;
        }

        let state = runtime.state.clone();
        runtimes.insert(id.clone(), runtime);

        tracing::info!("updated runtime '{}'", id);

        Ok(SystemInfo {
            config,
            state,
            lif: existing_lif,
        })
    }

    pub async fn remove(&self, id: String) -> Result<()> {
        tracing::info!("removing runtime '{}'", id);
        let mut runtimes = self.runtimes.write().await;

        // Delete from the database first; if that fails the in-memory runtime
        // stays intact rather than silently reappearing on the next restart.
        self.persistence.delete_configuration(id.clone())?;
        // No foreign keys or cascade on persisted_lif_map, so the layout row has
        // to be removed explicitly — otherwise a multi-megabyte blob is orphaned
        // for every deleted system.
        self.persistence.delete_lif_map(&id)?;

        if let Some(runtime) = runtimes.get_mut(&id) {
            runtime.stop().await?;
        }

        _ = runtimes.remove(&id);
        Ok(())
    }

    /// Whether a system with this id is registered.
    pub async fn exists(&self, id: &str) -> bool {
        self.runtimes.read().await.contains_key(id)
    }

    /// Parse, validate, and store a LIF layout for a system.
    ///
    /// Deliberately not routed through [`Self::update`]: that rebuilds the
    /// runtime, which would discard the `StateManager` and replace the
    /// publisher's broadcast sender, disconnecting every live WebSocket viewer.
    /// A layout feeds neither the receiver nor the publisher, so it is swapped
    /// in place instead.
    pub async fn set_lif(&self, id: String, body: Bytes) -> Result<LifSummary> {
        // Fail before doing any expensive work if the system is unknown.
        {
            let runtimes = self.runtimes.read().await;
            if !runtimes.contains_key(&id) {
                bail!("runtime '{}' does not exist", id);
            }
        }

        let persistence = self.persistence.clone();
        let uploaded_at = Utc::now().to_rfc3339();
        let system_id = id.clone();

        let summary = tokio::task::spawn_blocking(move || -> Result<LifSummary> {
            let raw_bytes = body.len() as u64;

            let lif: Lif =
                serde_json::from_slice(&body).context("uploaded file is not valid LIF JSON")?;
            shared::lif::validate(&lif)?;
            let summary = LifSummary::derive(&lif, raw_bytes, uploaded_at);

            // No need to keep the whole lif in memory. the sumamry is what we keep.
            drop(lif);

            let gzip = compress(&body)?;
            drop(body);

            tracing::info!(
                "layout for '{}': {} bytes -> {} bytes gzipped ({:.1}%)",
                system_id,
                raw_bytes,
                gzip.len(),
                (gzip.len() as f64 / raw_bytes.max(1) as f64) * 100.0,
            );

            persistence.upsert_lif_map(&system_id, &gzip, &summary)?;
            Ok(summary)
        })
        .await??;

        // Persisted successfully, so publish it in memory.
        let runtimes = self.runtimes.read().await;
        let Some(runtime) = runtimes.get(&id) else {
            bail!(
                "runtime '{}' was removed while its layout was uploading",
                id
            );
        };
        *runtime.lif.write().await = Some(summary.clone());

        tracing::info!(
            "stored layout for runtime '{}' ({} nodes, {} edges)",
            id,
            summary.node_count,
            summary.edge_count
        );

        Ok(summary)
    }

    /// Fetch a system's stored layout, still gzip-compressed.
    ///
    /// Returned compressed so it can be served straight through with a
    /// `Content-Encoding: gzip` header — the backend never decompresses it.
    pub async fn get_lif_gzip(&self, id: String) -> Result<Option<Vec<u8>>> {
        let persistence = self.persistence.clone();
        let bytes = tokio::task::spawn_blocking(move || persistence.read_lif_gzip(&id)).await??;
        Ok(bytes)
    }

    /// Remove a system's stored layout.
    pub async fn delete_lif(&self, id: String) -> Result<()> {
        let persistence = self.persistence.clone();
        let system_id = id.clone();
        tokio::task::spawn_blocking(move || persistence.delete_lif_map(&system_id)).await??;

        let runtimes = self.runtimes.read().await;
        if let Some(runtime) = runtimes.get(&id) {
            *runtime.lif.write().await = None;
        }
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

/// Gzip the uploaded document for storage.
fn compress(raw: &[u8]) -> Result<Vec<u8>> {
    use flate2::{Compression, write::GzEncoder};
    use std::io::Write;

    // Default rather than fast: on a 40 MB layout, default costs ~105 ms and
    // stores 0.92 MB, fast costs ~22 ms and stores 1.70 MB (release, see
    // `examples/lif_timing.rs`). Upload is rare and the smaller blob is read
    // back on every fetch, so the extra 80 ms is worth paying once.
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(raw)?;
    Ok(encoder.finish()?)
}
