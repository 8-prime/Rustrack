mod agv;
mod broker;
mod config;
mod fleet;
mod map;
mod nurbs;
mod publisher;
mod scenario;

use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Parser;
use config::{AgvDef, ScenarioKind, SimConfig};
use fleet::FleetController;
use map::NodeMap;
use publisher::Publisher;
use rustrack_shared::lif::{self, Lif};
use scenario::{RandomWalkScenario, ScriptedScenario};

#[derive(Parser)]
#[command(
    name = "rustrack-simulator",
    about = "VDA5050 AGV fleet simulator with embedded MQTT broker"
)]
struct Cli {
    #[arg(short, long, default_value = "simulator/examples/warehouse_5agv.toml")]
    config: std::path::PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "simulator=debug".into()),
        )
        .init();

    let cli = Cli::parse();
    let raw = std::fs::read_to_string(&cli.config)
        .map_err(|e| anyhow::anyhow!("failed to read config {:?}: {e}", cli.config))?;
    let cfg: SimConfig = toml::from_str(&raw)?;

    // 1. Start the embedded MQTT broker
    let broker_addr = broker::start(&cfg.broker.bind_addr, cfg.broker.port)?;

    // 2. Connect publisher client to the local broker (always via loopback)
    let (publisher, mut eventloop) = Publisher::connect(
        "127.0.0.1",
        broker_addr.port(),
        cfg.mqtt.topic_prefix.clone(),
    )
    .await?;

    // Drain the event loop in a background task so the client stays connected
    tokio::spawn(async move {
        loop {
            match eventloop.poll().await {
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!("MQTT event loop error: {e}");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    });

    // 3. Load the LIF layout and build the node map
    let map = load_map(&cli.config, &cfg.map)?;

    // 4. Build the fleet
    let agvs = build_fleet(&cfg.fleet, &map);
    let mut fleet = FleetController::new(agvs);

    tracing::info!(
        "simulator running: {} AGVs, broker at {broker_addr}, tick_hz={}",
        cfg.fleet.len(),
        cfg.mqtt.tick_hz
    );

    // 5. Tick loop
    let tick_interval = Duration::from_secs_f64(1.0 / cfg.mqtt.tick_hz);
    let mut last_tick = Instant::now();

    loop {
        let now = Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f64();
        last_tick = now;

        let states = fleet.tick(dt, &map);
        for (serial, state) in &states {
            if let Err(e) = publisher.publish_state(serial, state).await {
                tracing::warn!("publish error for {serial}: {e}");
            }
        }

        tokio::time::sleep(tick_interval).await;
    }
}

/// Read, validate, and resolve the LIF layout referenced by the config.
///
/// The map path is resolved relative to the config file's directory, so a
/// config and its layout can be moved together.
fn load_map(config_path: &std::path::Path, map_cfg: &config::MapConfig) -> Result<NodeMap> {
    let lif_path = match config_path.parent() {
        Some(dir) if !map_cfg.file.is_absolute() => dir.join(&map_cfg.file),
        _ => map_cfg.file.clone(),
    };

    let raw = std::fs::read_to_string(&lif_path)
        .map_err(|e| anyhow::anyhow!("failed to read LIF map {lif_path:?}: {e}"))?;
    let lif: Lif = serde_json::from_str(&raw)
        .map_err(|e| anyhow::anyhow!("{lif_path:?} is not valid LIF JSON: {e}"))?;

    // Validate before building: NodeMap indexes edge endpoints directly, so a
    // dangling node reference would panic rather than report a useful error.
    lif::validate(&lif).map_err(|e| anyhow::anyhow!("{lif_path:?}: {e}"))?;

    let layout = lif
        .resolve(map_cfg.layout_id.as_deref(), &map_cfg.vehicle_type_id)
        .map_err(|e| anyhow::anyhow!("{lif_path:?}: {e}"))?;

    tracing::info!(
        "loaded layout '{}' ({} nodes, {} edges) for vehicle type '{}'",
        layout.layout_id,
        layout.nodes.len(),
        layout.edges.len(),
        map_cfg.vehicle_type_id,
    );
    if layout.nodes_excluded > 0 || layout.edges_excluded > 0 {
        tracing::warn!(
            "{} nodes and {} edges are not available to vehicle type '{}' and were excluded",
            layout.nodes_excluded,
            layout.edges_excluded,
            map_cfg.vehicle_type_id,
        );
    }

    Ok(NodeMap::build(&layout))
}

fn build_fleet(defs: &[AgvDef], map: &NodeMap) -> Vec<agv::AgvSimulator> {
    let first_node = map.nodes.keys().next().cloned().unwrap_or_default();

    defs.iter()
        .map(|def| {
            let start = def
                .start_node
                .clone()
                .or_else(|| def.route.as_ref().and_then(|r| r.first().cloned()))
                .unwrap_or_else(|| first_node.clone());

            let scenario: Box<dyn scenario::Scenario> = match def.scenario {
                ScenarioKind::Scripted => {
                    let route = def.route.clone().unwrap_or_else(|| vec![start.clone()]);
                    Box::new(ScriptedScenario::new(route, def.r#loop))
                }
                ScenarioKind::RandomWalk => Box::new(RandomWalkScenario::new()),
            };

            agv::AgvSimulator::new(def.serial.clone(), def.speed_m_s, start, scenario, map)
        })
        .collect()
}
