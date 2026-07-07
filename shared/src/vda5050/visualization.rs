use serde::{Deserialize, Serialize};

/// AGV position and/or velocity for visualization purposes. Can be published at a higher
/// rate if wanted. Since bandwidth may be expensive depening on the update rate for this
/// topic, all fields are optional.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Visualization {
    /// The AGVs position
    pub agv_position: Option<AgvPosition>,
    /// headerId of the message. The headerId is defined per topic and incremented by 1 with each
    /// sent (but not necessarily received) message.
    pub header_id: Option<i64>,
    /// Manufacturer of the AGV
    pub manufacturer: Option<String>,
    /// Serial number of the AGV.
    pub serial_number: Option<String>,
    /// Timestamp in ISO8601 format (YYYY-MM-DDTHH:mm:ss.ffZ).
    pub timestamp: Option<String>,
    /// The AGVs velocity in vehicle coordinates
    pub velocity: Option<Velocity>,
    /// Version of the protocol [Major].[Minor].[Patch]
    pub version: Option<String>,
}

/// The AGVs position
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgvPosition {
    /// Value for position deviation range in meters. Can be used if the AGV is able to derive it.
    pub deviation_range: Option<f64>,
    /// Localization score for SLAM based vehicles, if the AGV can communicate it.
    pub localization_score: Option<f64>,
    pub map_id: String,
    /// True if the AGVs position is initialized, false, if position is not initizalized.
    pub position_initialized: bool,
    pub theta: f64,
    pub x: f64,
    pub y: f64,
}

/// The AGVs velocity in vehicle coordinates
#[derive(Serialize, Deserialize)]
pub struct Velocity {
    pub omega: Option<f64>,
    pub vx: Option<f64>,
    pub vy: Option<f64>,
}
