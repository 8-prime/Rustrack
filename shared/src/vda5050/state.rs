use serde::{Deserialize, Serialize};

/// all encompassing state of the AGV.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    /// Contains a list of the current actions and the actions which are yet to be finished. This
    /// may include actions from previous nodes that are still in progress
    /// When an action is completed, an updated state message is published with actionStatus set
    /// to finished and if applicable with the corresponding resultDescription. The actionStates
    /// are kept until a new order is received.
    pub action_states: Vec<ActionState>,
    /// Defines the position on a map in world coordinates. Each floor has its own map.
    pub agv_position: Option<AgvPosition>,
    /// Contains all battery-related information.
    pub battery_state: BatteryState,
    /// Used by line guided vehicles to indicate the distance it has been driving past the
    /// "lastNodeId".
    /// Distance is in meters.
    pub distance_since_last_node: Option<f64>,
    /// True: indicates that the AGV is driving and/or rotating. Other movements of the AGV
    /// (e.g., lift movements) are not included here.
    /// False: indicates that the AGV is neither driving nor rotating
    pub driving: bool,
    /// Array of edgeState-Objects, that need to be traversed for fulfilling the order, empty
    /// list if idle.
    pub edge_states: Vec<EdgeState>,
    /// Array of error-objects. All active errors of the AGV should be in the list. An empty
    /// array indicates that the AGV has no active errors.
    pub errors: Vec<Error>,
    /// headerId of the message. The headerId is defined per topic and incremented by 1 with each
    /// sent (but not necessarily received) message.
    pub header_id: i64,
    /// Array of info-objects. An empty array indicates, that the AGV has no information. This
    /// should only be used for visualization or debugging – it must not be used for logic in
    /// master control.
    pub information: Option<Vec<Information>>,
    /// nodeID of last reached node or, if AGV is currently on a node, current node (e.g.,
    /// "node7"). Empty string ("") if no lastNodeId is available.
    pub last_node_id: String,
    /// sequenceId of the last reached node or, if the AGV is currently on a node, sequenceId of
    /// current node. "0" if no lastNodeSequenceId is available.
    pub last_node_sequence_id: i64,
    /// Loads, that are currently handled by the AGV. Optional: If AGV cannot determine load
    /// state, leave the array out of the state. If the AGV can determine the load state, but the
    /// array is empty, the AGV is considered unloaded.
    pub loads: Option<Vec<Load>>,
    /// Manufacturer of the AGV
    pub manufacturer: String,
    /// Array of map-objects that are currently stored on the vehicle.
    pub maps: Option<Vec<Map>>,
    /// True: AGV is almost at the end of the base and will reduce speed if no new base is
    /// transmitted. Trigger for master control to send new base
    /// False: no base update required.
    pub new_base_request: Option<bool>,
    /// Array of nodeState-Objects, that need to be traversed for fulfilling the order. Empty
    /// list if idle.
    pub node_states: Vec<NodeState>,
    /// Current operating mode of the AGV.
    pub operating_mode: OperatingMode,
    /// Unique order identification of the current order or the previous finished order. The
    /// orderId is kept until a new order is received. Empty string ("") if no previous orderId
    /// is available.
    pub order_id: String,
    /// Order Update Identification to identify that an order update has been accepted by the
    /// AGV. "0" if no previous orderUpdateId is available.
    pub order_update_id: i64,
    /// True: AGV is currently in a paused state, either because of the push of a physical button
    /// on the AGV or because of an instantAction. The AGV can resume the order.
    /// False: The AGV is currently not in a paused state.
    pub paused: Option<bool>,
    /// Contains all safety-related information.
    pub safety_state: SafetyState,
    /// Serial number of the AGV.
    pub serial_number: String,
    /// Timestamp in ISO8601 format (YYYY-MM-DDTHH:mm:ss.ffZ).
    pub timestamp: String,
    /// The AGVs velocity in vehicle coordinates
    pub velocity: Option<Velocity>,
    /// Version of the protocol [Major].[Minor].[Patch]
    pub version: String,
    /// Unique ID of the zone set that the AGV currently uses for path planning. Must be the same
    /// as the one used in the order, otherwise the AGV is to reject the order.
    /// Optional: If the AGV does not use zones, this field can be omitted.
    pub zone_set_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionState {
    /// Additional information on the current action.
    pub action_description: Option<String>,
    /// Unique actionId
    pub action_id: String,
    /// WAITING: waiting for the trigger (passing the mode, entering the edge) PAUSED: paused by
    /// instantAction or external trigger FAILED: action could not be performed.
    pub action_status: ActionStatus,
    /// actionType of the action.
    /// Optional: Only for informational or visualization purposes. Order knows the type.
    pub action_type: Option<String>,
    /// Description of the result, e.g., the result of a RFID-read. Errors will be transmitted in
    /// errors.
    pub result_description: Option<String>,
}

/// WAITING: waiting for the trigger (passing the mode, entering the edge) PAUSED: paused by
/// instantAction or external trigger FAILED: action could not be performed.
#[derive(Serialize, Deserialize)]
pub enum ActionStatus {
    #[serde(rename = "FAILED")]
    Failed,
    #[serde(rename = "FINISHED")]
    Finished,
    #[serde(rename = "INITIALIZING")]
    Initializing,
    #[serde(rename = "RUNNING")]
    Running,
    #[serde(rename = "WAITING")]
    Waiting,
}

/// Defines the position on a map in world coordinates. Each floor has its own map.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgvPosition {
    /// Value for position deviation range in meters. Optional for vehicles that cannot estimate
    /// their deviation, e.g., grid-based localization. Only for logging and visualization
    /// purposes.
    pub deviation_range: Option<f64>,
    /// Describes the quality of the localization and therefore, can be used, e.g., by SLAM-AGV
    /// to describe how accurate the current position information is.
    /// 0.0: position unknown
    /// 1.0: position known
    /// Optional for vehicles that cannot estimate their localization score.
    /// Only for logging and visualization purposes
    pub localization_score: Option<f64>,
    pub map_description: Option<String>,
    pub map_id: String,
    /// True: position is initialized. False: position is not initizalized.
    pub position_initialized: bool,
    pub theta: f64,
    pub x: f64,
    pub y: f64,
}

/// Contains all battery-related information.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatteryState {
    /// State of Charge in %:
    /// If AGV only provides values for good or bad battery levels, these will be indicated as
    /// 20% (bad) and 80% (good).
    pub battery_charge: f64,
    /// State of health in percent.
    pub battery_health: Option<f64>,
    /// Battery voltage
    pub battery_voltage: Option<f64>,
    /// True: charging in progress. False: AGV is currently not charging.
    pub charging: bool,
    /// Estimated reach with current State of Charge in meter.
    pub reach: Option<f64>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeState {
    /// Additional information on the edge.
    pub edge_description: Option<String>,
    /// Unique edge identification
    pub edge_id: String,
    /// True indicates that the edge is part of the base. False indicates that the edge is part
    /// of the horizon.
    pub released: bool,
    /// sequenceId of the edge.
    pub sequence_id: i64,
    /// The trajectory is to be communicated as a NURBS and is defined in chapter 6.7
    /// Implementation of the Order message.
    /// Trajectory segments reach from the point, where the AGV starts to enter the edge to the
    /// point where it reports that the next node was traversed.
    pub trajectory: Option<Trajectory>,
}

/// The trajectory is to be communicated as a NURBS and is defined in chapter 6.7
/// Implementation of the Order message.
/// Trajectory segments reach from the point, where the AGV starts to enter the edge to the
/// point where it reports that the next node was traversed.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trajectory {
    /// List of JSON controlPoint objects defining the control points of the NURBS, which
    /// includes the beginning and end point.
    pub control_points: Vec<ControlPoint>,
    /// Defines the number of control points that influence any given point on the curve.
    /// Increasing the degree increases continuity. If not defined, the default value is 1.
    pub degree: i64,
    /// Sequence of parameter values that determine where and how the control points affect the
    /// NURBS curve. knotVector has size of number of control points + degree + 1
    pub knot_vector: Vec<f64>,
}

#[derive(Serialize, Deserialize)]
pub struct ControlPoint {
    /// The weight, with which this control point pulls on the curve.
    /// When not defined, the default will be 1.0.
    pub weight: Option<f64>,
    pub x: f64,
    pub y: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Error {
    /// Verbose description providing details and possible causes of the error.
    pub error_description: Option<String>,
    /// Hint on how to approach or solve the reported error.
    pub error_hint: Option<String>,
    /// WARNING: AGV is ready to start (e.g., maintenance cycle expiration warning). FATAL: AGV
    /// is not in running condition, user intervention required (e.g., laser scanner is
    /// contaminated).
    pub error_level: ErrorLevel,
    pub error_references: Option<Vec<ErrorReference>>,
    /// Type/name of error.
    pub error_type: String,
}

/// WARNING: AGV is ready to start (e.g., maintenance cycle expiration warning). FATAL: AGV
/// is not in running condition, user intervention required (e.g., laser scanner is
/// contaminated).
#[derive(Serialize, Deserialize)]
pub enum ErrorLevel {
    #[serde(rename = "FATAL")]
    Fatal,
    #[serde(rename = "WARNING")]
    Warning,
}

/// Array of references (e.g. nodeId, edgeId, orderId, actionId, etc.) to provide more
/// information related to the error.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorReference {
    /// Specifies the type of reference used (e.g. nodeId, edgeId, orderId, actionId, etc.).
    pub reference_key: String,
    /// The value that belongs to the reference key. For example, the id of the node where the
    /// error occurred.
    pub reference_value: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Information {
    /// Info of description.
    pub info_description: Option<String>,
    /// DEBUG: used for debugging. INFO: used for visualization.
    pub info_level: InfoLevel,
    pub info_references: Option<Vec<InfoReference>>,
    /// Type/name of information.
    pub info_type: String,
}

/// DEBUG: used for debugging. INFO: used for visualization.
#[derive(Serialize, Deserialize)]
pub enum InfoLevel {
    #[serde(rename = "DEBUG")]
    Debug,
    #[serde(rename = "INFO")]
    Info,
}

/// Array of references.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoReference {
    /// References the type of reference (e.g., headerId, orderId, actionId, etc.).
    pub reference_key: String,
    /// References the value, which belongs to the reference key.
    pub reference_value: String,
}

/// Load object that describes the load if the AGV has information about it.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Load {
    /// Point of reference for the location of the bounding box. The point of reference is always
    /// the center of the bounding box bottom surface (at height = 0) and is described in
    /// coordinates of the AGV coordinate system.
    pub bounding_box_reference: Option<BoundingBoxReference>,
    /// Dimensions of the loads bounding box in meters.
    pub load_dimensions: Option<LoadDimensions>,
    /// Unique identification number of the load (e.g., barcode or RFID). Empty field, if the AGV
    /// can identify the load, but did not identify the load yet. Optional, if the AGV cannot
    /// identify the load.
    pub load_id: Option<String>,
    /// Indicates, which load handling/carrying unit of the AGV is used, e.g., in case the AGV
    /// has multiple spots/positions to carry loads. Optional for vehicles with only one
    /// loadPosition.
    pub load_position: Option<String>,
    /// Type of load.
    pub load_type: Option<String>,
    /// Absolute weight of the load measured in kg.
    pub weight: Option<f64>,
}

/// Point of reference for the location of the bounding box. The point of reference is always
/// the center of the bounding box bottom surface (at height = 0) and is described in
/// coordinates of the AGV coordinate system.
#[derive(Serialize, Deserialize)]
pub struct BoundingBoxReference {
    /// Orientation of the loads bounding box. Important for tugger, trains, etc.
    pub theta: Option<f64>,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Dimensions of the loads bounding box in meters.
#[derive(Serialize, Deserialize)]
pub struct LoadDimensions {
    /// Absolute height of the loads bounding box in meter.
    /// Optional:
    /// Set value only if known.
    pub height: Option<f64>,
    /// Absolute length of the loads bounding box in meter.
    pub length: f64,
    /// Absolute width of the loads bounding box in meter.
    pub width: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Map {
    /// Additional information on the map.
    pub map_description: Option<String>,
    /// ID of the map describing a defined area of the vehicle's workspace.
    pub map_id: String,
    /// Information on the status of the map indicating, if a map version is currently used on
    /// the vehicle. ENABLED: Indicates this map is currently active / used on the AGV. At most
    /// one map with the same mapId can have its status set to ENABLED.<br>DISABLED: Indicates
    /// this map version is currently not enabled on the AGV and thus could be enabled or deleted
    /// by request.
    pub map_status: MapStatus,
    /// Version of the map.
    pub map_version: String,
}

/// Information on the status of the map indicating, if a map version is currently used on
/// the vehicle. ENABLED: Indicates this map is currently active / used on the AGV. At most
/// one map with the same mapId can have its status set to ENABLED.<br>DISABLED: Indicates
/// this map version is currently not enabled on the AGV and thus could be enabled or deleted
/// by request.
#[derive(Serialize, Deserialize)]
pub enum MapStatus {
    #[serde(rename = "DISABLED")]
    Disabled,
    #[serde(rename = "ENABLED")]
    Enabled,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeState {
    /// Additional information on the node.
    pub node_description: Option<String>,
    /// Unique node identification
    pub node_id: String,
    /// Node position. The object is defined in chapter 5.4 Topic: Order (from master control to
    /// AGV).
    /// Optional:Master control has this information. Can be sent additionally, e.g., for
    /// debugging purposes.
    pub node_position: Option<NodePosition>,
    /// True: indicates that the node is part of the base. False: indicates that the node is part
    /// of the horizon.
    pub released: bool,
    /// sequenceId to discern multiple nodes with same nodeId.
    pub sequence_id: i64,
}

/// Node position. The object is defined in chapter 5.4 Topic: Order (from master control to
/// AGV).
/// Optional:Master control has this information. Can be sent additionally, e.g., for
/// debugging purposes.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodePosition {
    pub map_id: String,
    pub theta: Option<f64>,
    pub x: f64,
    pub y: f64,
}

/// Current operating mode of the AGV.
#[derive(Serialize, Deserialize)]
pub enum OperatingMode {
    #[serde(rename = "AUTOMATIC")]
    Automatic,
    #[serde(rename = "MANUAL")]
    Manual,
    #[serde(rename = "SEMIAUTOMATIC")]
    Semiautomatic,
    #[serde(rename = "SERVICE")]
    Service,
    #[serde(rename = "TEACHIN")]
    Teachin,
}

/// Contains all safety-related information.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetyState {
    /// Acknowledge-Type of eStop: AUTOACK: auto-acknowledgeable e-stop is activated, e.g., by
    /// bumper or protective field. MANUAL: e-stop hast to be acknowledged manually at the
    /// vehicle. REMOTE: facility e-stop has to be acknowledged remotely. NONE: no e-stop
    /// activated.
    pub e_stop: EStop,
    /// Protective field violation. True: field is violated. False: field is not violated.
    pub field_violation: bool,
}

/// Acknowledge-Type of eStop: AUTOACK: auto-acknowledgeable e-stop is activated, e.g., by
/// bumper or protective field. MANUAL: e-stop hast to be acknowledged manually at the
/// vehicle. REMOTE: facility e-stop has to be acknowledged remotely. NONE: no e-stop
/// activated.
#[derive(Serialize, Deserialize)]
pub enum EStop {
    #[serde(rename = "AUTOACK")]
    Autoack,
    #[serde(rename = "MANUAL")]
    Manual,
    #[serde(rename = "NONE")]
    None,
    #[serde(rename = "REMOTE")]
    Remote,
}

/// The AGVs velocity in vehicle coordinates
#[derive(Serialize, Deserialize)]
pub struct Velocity {
    /// The AVGs turning speed around its z axis.
    pub omega: Option<f64>,
    /// The AVGs velocity in its x direction
    pub vx: Option<f64>,
    /// The AVGs velocity in its y direction
    pub vy: Option<f64>,
}
