use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SimConfig {
    pub broker: BrokerConfig,
    pub mqtt: MqttPublishConfig,
    pub map: MapConfig,
    pub fleet: Vec<AgvDef>,
}

#[derive(Debug, Deserialize)]
pub struct BrokerConfig {
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,
    #[serde(default = "default_broker_port")]
    pub port: u16,
}

fn default_bind_addr() -> String {
    "0.0.0.0".to_string()
}
fn default_broker_port() -> u16 {
    1883
}

#[derive(Debug, Deserialize)]
pub struct MqttPublishConfig {
    #[serde(default = "default_topic_prefix")]
    pub topic_prefix: String,
    #[serde(default = "default_tick_hz")]
    pub tick_hz: f64,
}

fn default_topic_prefix() -> String {
    "uagv".to_string()
}
fn default_tick_hz() -> f64 {
    1.0
}

/// Where the track layout comes from.
///
/// The map is a LIF (VDMA Layout Interchange Format) file rather than an inline
/// table, so layouts exported by a vehicle integrator load without conversion.
#[derive(Debug, Deserialize, Clone)]
pub struct MapConfig {
    /// Path to the `.lif` file, resolved relative to the config file's directory.
    pub file: std::path::PathBuf,
    /// Which vehicle type's properties to resolve. LIF scopes traversability,
    /// speed, and orientation per vehicle type; nodes and edges without an entry
    /// for this type are excluded from the simulated graph.
    pub vehicle_type_id: String,
    /// Which layout to use. Optional when the file contains exactly one.
    pub layout_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioKind {
    Scripted,
    RandomWalk,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgvDef {
    pub serial: String,
    pub scenario: ScenarioKind,
    /// Required for scripted scenario. Ignored for random_walk.
    pub route: Option<Vec<String>>,
    #[serde(default = "default_speed")]
    pub speed_m_s: f64,
    #[serde(default = "default_loop")]
    pub r#loop: bool,
    /// Starting node for random_walk. Defaults to first map node.
    pub start_node: Option<String>,
}

fn default_speed() -> f64 {
    1.2
}
fn default_loop() -> bool {
    true
}
