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

#[derive(Debug, Deserialize, Clone)]
pub struct NodeDef {
    pub id: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ControlPointDef {
    pub x: f64,
    pub y: f64,
    #[serde(default = "default_weight")]
    pub w: f64,
}

fn default_weight() -> f64 {
    1.0
}

#[derive(Debug, Deserialize, Clone)]
pub struct EdgeDef {
    pub id: String,
    pub from: String,
    pub to: String,
    #[serde(default = "default_max_speed")]
    pub max_speed: f64,
    /// Optional NURBS control points. If absent, edge uses linear interpolation.
    pub control_points: Option<Vec<ControlPointDef>>,
}

fn default_max_speed() -> f64 {
    1.5
}

#[derive(Debug, Deserialize, Clone)]
pub struct MapConfig {
    pub nodes: Vec<NodeDef>,
    pub edges: Vec<EdgeDef>,
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
