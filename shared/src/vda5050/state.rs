use serde::{Serialize, Deserialize};

/// State of the mobile robot.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    /// Array of the current actions and the actions which are yet to be finished. This may
    /// include actions from previous nodes that are still in progress
    /// When an action is completed, an updated state message is published with actionStatus set
    /// to finished and if applicable with the corresponding resultDescriptor. The actionStates
    /// are kept until a new order is received.
    action_states: Vec<ActionState>,
    /// Used by line guided vehicles to indicate the distance it has been driving past the
    /// lastNodeId. Distance is in meters.
    distance_since_last_node: Option<f64>,
    /// True: indicates that the mobile robot is driving and/or rotating. Other movements of the
    /// mobile robot (e.g., lift movements) are not included here.
    /// False: indicates that the mobile robot is neither driving nor rotating.
    driving: bool,
    /// Array of edgeRequest objects that are currently active on the mobile robot. Empty array
    /// if no edge requests are active.
    edge_requests: Option<Vec<EdgeRequest>>,
    /// Array of edgeState-Objects, that need to be traversed for fulfilling the order, empty
    /// list if idle.
    edge_states: Vec<EdgeState>,
    /// Array of error-objects. All active errors of the mobile robot should be in the list. An
    /// empty array indicates that the mobile robot has no active errors.
    errors: Vec<Error>,
    /// headerId of the message. The headerId is defined per topic and incremented by 1 with each
    /// sent (but not necessarily received) message.
    header_id: i64,
    /// Array of info-objects. An empty array indicates, that the mobile robot has no
    /// information. This should only be used for visualization or debugging – it must not be
    /// used for logic in fleet control.
    information: Option<Vec<Info>>,
    /// Array of all instant action states that the mobile robot received. Empty array if the
    /// mobile robot has not received any instant actions. Instant actions are kept in the state
    /// until restart or action clearInstantActions is executed.
    instant_action_states: Vec<ActionState>,
    intermediate_path: Option<IntermediatePath>,
    /// Node ID of last reached node or, if mobile robot is currently on a node, current node
    /// (e.g., "node7"). Empty string ("") if no lastNodeId is available.
    last_node_id: String,
    /// sequenceId of the last reached node or, if the mobile robot is currently on a node,
    /// sequenceId of current node. 0 if no lastNodeSequenceId is available.
    last_node_sequence_id: i64,
    /// Loads, that are currently handled by the mobile robot. Optional: If mobile robot cannot
    /// determine load state, leave the array out of the state. If the mobile robot can determine
    /// the load state, but the array is empty, the mobile robot is considered unloaded.
    loads: Option<Vec<Load>>,
    /// Manufacturer of the mobile robot
    manufacturer: String,
    /// Array of map-objects that are currently stored on the mobile robot.
    maps: Option<Vec<Map>>,
    pub mobile_robot_position: Option<MobileRobotPosition>,
    /// True: mobile robot is almost at the end of the base and will reduce speed if no new base
    /// is transmitted. Trigger for fleet control to send new base
    /// False: no base update required.
    new_base_request: Option<bool>,
    /// Array of nodeState-Objects, that need to be traversed for fulfilling the order. Empty
    /// list if idle.
    node_states: Vec<NodeState>,
    /// Current operating mode of the mobile robot.
    operating_mode: OperatingMode,
    /// Unique order identification of the current order or the previous finished order. The
    /// orderId is kept until a new order is received. Empty string ("") if no previous orderId
    /// is available.
    order_id: String,
    /// Order Update Identification to identify that an order update has been accepted by the
    /// mobile robot. 0 if no previous orderUpdateId is available.
    order_update_id: i64,
    /// True: mobile robot is currently in a paused state, either because of the push of a
    /// physical button on the mobile robot or because of an instantAction. The mobile robot can
    /// resume the order.
    /// False: The mobile robot is currently not in a paused state.
    paused: Option<bool>,
    pub planned_path: Option<PlannedPath>,
    power_supply: PowerSupply,
    safety_state: SafetyState,
    /// Serial number of the mobile robot.
    serial_number: String,
    /// Timestamp in ISO8601 format (YYYY-MM-DDTHH:mm:ss.fffZ).
    pub timestamp: String,
    /// The mobile robot's velocity in mobile robot coordinates
    velocity: Option<Velocity>,
    /// Version of the protocol [Major].[Minor].[Patch]
    version: String,
    /// Array of all zone actions that are in an end state or are currently running; sharing
    /// upcoming actions is optional. Zone action states are kept in the state message until
    /// restart or action clearZoneActions is executed.
    zone_action_states: Option<Vec<ActionState>>,
    /// Array of zoneRequest objects that are currently active on the mobile robot. Empty array
    /// if no zone requests are active.
    zone_requests: Option<Vec<ZoneRequest>>,
    /// Array of zoneSet objects that are currently stored on the mobile robot.
    zone_sets: Option<Vec<ZoneSet>>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionState {
    /// A user-defined, human-readable name or descriptor.
    action_descriptor: Option<String>,
    /// Unique actionId
    action_id: String,
    /// Description of the result, e.g., the result of a RFID-read. Errors will be transmitted in
    /// errors.
    action_result: Option<String>,
    /// WAITING: waiting for the trigger (passing the mode, entering the edge); INITIALIZING:
    /// Action was triggered, preparatory measures are initiated; RUNNING: The action is running;
    /// RETRIABLE: Actions that failed, but can be retried; PAUSED: paused by instantAction or
    /// external trigger; FINISHED: The action is finished; FAILED: action could not be performed.
    action_status: ActionStatus,
    /// actionType of the action. Optional: Only for informational or visualization purposes.
    /// Order knows the type.
    action_type: Option<String>,
}

/// WAITING: waiting for the trigger (passing the mode, entering the edge); INITIALIZING:
/// Action was triggered, preparatory measures are initiated; RUNNING: The action is running;
/// RETRIABLE: Actions that failed, but can be retried; PAUSED: paused by instantAction or
/// external trigger; FINISHED: The action is finished; FAILED: action could not be performed.
#[derive(Serialize, Deserialize)]
pub enum ActionStatus {
    #[serde(rename = "FAILED")]
    Failed,
    #[serde(rename = "FINISHED")]
    Finished,
    #[serde(rename = "INITIALIZING")]
    Initializing,
    #[serde(rename = "PAUSED")]
    Paused,
    #[serde(rename = "RETRIABLE")]
    Retriable,
    #[serde(rename = "RUNNING")]
    Running,
    #[serde(rename = "WAITING")]
    Waiting,
}

/// Edge request information sent by the mobile robot to fleet control.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeRequest {
    /// Globally unique identifier referencing the edge the request is related to.
    edge_id: String,
    /// Unique per mobile robot identifier across all active requests.
    request_id: String,
    /// When stating a request, this is set to REQUESTED. After response or update from fleet
    /// control set to GRANTED or REVOKED. If lease time expires set to EXPIRED.
    request_status: RequestStatus,
    /// Enum specifying the type of edge the request relates to.
    request_type: EdgeRequestRequestType,
    /// Tracking number for sequence of edge within order. Required to uniquely identify the
    /// referenced edge within the order.
    sequence_id: i64,
}

/// When stating a request, this is set to REQUESTED. After response or update from fleet
/// control set to GRANTED or REVOKED. If lease time expires set to EXPIRED.
///
/// When stating a request, this is set to REQUESTED. After response or update from fleet
/// control set to GRANTED or REVOKED. If lease time expires, shall be to EXPIRED.
#[derive(Serialize, Deserialize)]
pub enum RequestStatus {
    #[serde(rename = "EXPIRED")]
    Expired,
    #[serde(rename = "GRANTED")]
    Granted,
    #[serde(rename = "REQUESTED")]
    Requested,
    #[serde(rename = "REVOKED")]
    Revoked,
}

/// Enum specifying the type of edge the request relates to.
#[derive(Serialize, Deserialize)]
pub enum EdgeRequestRequestType {
    #[serde(rename = "CORRIDOR")]
    Corridor,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeState {
    /// A user-defined, human-readable name or descriptor.
    edge_descriptor: Option<String>,
    /// Unique edge identification
    edge_id: String,
    /// True indicates that the edge is part of the base. False indicates that the edge is part
    /// of the horizon.
    released: bool,
    /// Sequence ID to differentiate between multiple edges with the same edgeId
    sequence_id: i64,
    /// Reports the trajectory that has been defined a priori within a layout or was sent for
    /// this edge as part of the order.
    trajectory: Option<Trajectory>,
}

/// Reports the trajectory that has been defined a priori within a layout or was sent for
/// this edge as part of the order.
///
/// The trajectory is to be communicated as a NURBS and is defined in chapter 6.7
/// Implementation of the Order message. Trajectory segments reach from the point, where the
/// mobile robot starts to enter the edge to the point where it reports that the next node
/// was traversed.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trajectory {
    /// List of JSON controlPoint objects defining the control points of the NURBS, which
    /// includes the beginning and end point.
    pub control_points: Vec<ControlPoint>,
    /// Defines the number of control points that influence any given point on the curve.
    /// Increasing the degree increases differentiability. If not defined, the default value is 1.
    pub degree: Option<i64>,
    /// Sequence of parameter values that determine where and how the control points affect the
    /// NURBS curve. knotVector has size of number of control points + degree + 1.
    pub knot_vector: Option<Vec<f64>>,
}

#[derive(Serialize, Deserialize)]
pub struct ControlPoint {
    /// The weight, with which this control point pulls on the curve. When not defined, the
    /// default will be 1.0.
    pub weight: Option<f64>,
    pub x: f64,
    pub y: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Error {
    /// Verbose description providing details and possible causes of the error.
    error_description: Option<String>,
    /// Array of translations of the error description.
    error_description_translations: Option<Vec<Translation>>,
    /// Hint on how to approach or solve the reported error.
    error_hint: Option<String>,
    /// Array of translations of the error hint.
    error_hint_translations: Option<Vec<Translation>>,
    /// WARNING: No immediate attention required, mobile robot is able to continue active and
    /// accept new order. URGENT: Immediate attention required, mobile robot is able to continue
    /// active and accept new order. CRITICAL: Immediate attention required, mobile robot is
    /// unable to continue active order, but can accept new order. FATAL: User intervention is
    /// required, mobile robot is unable to continue active or accept new order.
    error_level: ErrorLevel,
    error_references: Option<Vec<ErrorReference>>,
    /// Error type, extensible enumeration including the following predefined values.
    error_type: String,
}

/// Translation of a text for a given language code.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Translation {
    /// Specifies the language of the translation according to ISO 639-1.
    translation_key: String,
    /// Translation in language of translation key.
    translation_value: String,
}

/// WARNING: No immediate attention required, mobile robot is able to continue active and
/// accept new order. URGENT: Immediate attention required, mobile robot is able to continue
/// active and accept new order. CRITICAL: Immediate attention required, mobile robot is
/// unable to continue active order, but can accept new order. FATAL: User intervention is
/// required, mobile robot is unable to continue active or accept new order.
#[derive(Serialize, Deserialize)]
pub enum ErrorLevel {
    #[serde(rename = "CRITICAL")]
    Critical,
    #[serde(rename = "FATAL")]
    Fatal,
    #[serde(rename = "URGENT")]
    Urgent,
    #[serde(rename = "WARNING")]
    Warning,
}

/// Array of references (e.g. nodeId, edgeId, orderId, actionId, etc.) to provide more
/// information related to the error.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorReference {
    /// Specifies the type of reference used (e.g. nodeId, edgeId, orderId, actionId, etc.).
    reference_key: String,
    /// The value that belongs to the reference key. For example, the id of the node where the
    /// error occurred.
    reference_value: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    /// A user-defined, human-readable name or descriptor.
    info_descriptor: Option<String>,
    /// DEBUG: used for debugging. INFO: used for visualization.
    info_level: InfoLevel,
    info_references: Option<Vec<InfoReference>>,
    /// Type/name of information.
    info_type: String,
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
    reference_key: String,
    /// References the value, which belongs to the reference key.
    reference_value: String,
}

/// Represents the estimated time of arrival at closer waypoints that the mobile robot is
/// able to perceive with its sensors.
#[derive(Serialize, Deserialize)]
pub struct IntermediatePath {
    /// Array of end points of segments of a polyline.
    polyline: Vec<Polyline>,
}

/// Endpoint of a segment within a defined polyline.
#[derive(Serialize, Deserialize)]
pub struct Polyline {
    /// Estimated time of arrival/traversal. Formatted as a timestamp (ISO 8601, UTC);
    /// YYYY-MM-DDTHH:mm:ss.fffZ (e.g., '2017-04-15T11:40:03.123Z').
    eta: String,
    /// Absolute orientation of the mobile robot in the project-specific coordinate system.
    theta: Option<f64>,
    /// X-coordinate described in the project-specific coordinate system.
    x: f64,
    /// Y-coordinate described in the project-specific coordinate system.
    y: f64,
}

/// Load object that describes the load if the mobile robot has information about it.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Load {
    /// Point of reference for the location of the bounding box. The point of reference is always
    /// the center of the bounding box bottom surface (at height = 0) and is described in
    /// coordinates of the mobile robot coordinate system.
    bounding_box_reference: Option<BoundingBoxReference>,
    /// Dimensions of the loads bounding box in meters.
    load_dimensions: Option<LoadDimensions>,
    /// Unique identification number of the load (e.g., barcode or RFID). Empty field, if the
    /// mobile robot can identify the load, but did not identify the load yet. Optional, if the
    /// mobile robot cannot identify the load.
    load_id: Option<String>,
    /// Indicates, which load handling/carrying unit of the mobile robot is used, e.g., in case
    /// the mobile robot has multiple spots/positions to carry loads. Optional for vehicles with
    /// only one loadPosition.
    load_position: Option<String>,
    /// Type of load.
    load_type: Option<String>,
    /// Absolute weight of the load measured in kg.
    weight: Option<f64>,
}

/// Point of reference for the location of the bounding box. The point of reference is always
/// the center of the bounding box bottom surface (at height = 0) and is described in
/// coordinates of the mobile robot coordinate system.
#[derive(Serialize, Deserialize)]
pub struct BoundingBoxReference {
    /// Orientation of the loads bounding box. Important for tugger, trains, etc.
    theta: Option<f64>,
    x: f64,
    y: f64,
    z: f64,
}

/// Dimensions of the loads bounding box in meters.
#[derive(Serialize, Deserialize)]
pub struct LoadDimensions {
    /// Absolute height of the loads bounding box in meter.
    /// Optional:
    /// set value only if known.
    height: Option<f64>,
    /// Absolute length (along the mobile robot’s coordinate system's x-axis) of the load's
    /// bounding box in meters.
    length: f64,
    /// Absolute width (along the mobile robot’s coordinate system's y-axis) of the load's
    /// bounding box in meters.
    width: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Map {
    /// A user-defined, human-readable name or descriptor
    map_descriptor: Option<String>,
    /// ID of the map describing a defined area of the mobile robot's workspace.
    map_id: String,
    /// Information on the status of the map indicating, if a map version is currently used on
    /// the mobile robot. ENABLED: Indicates this map is currently active / used on the mobile
    /// robot. At most one map with the same mapId can have its status set to
    /// ENABLED.<br>DISABLED: Indicates this map version is currently not enabled on the mobile
    /// robot and thus could be enabled or deleted by request.
    map_status: Status,
    /// Version of the map.
    map_version: String,
}

/// Information on the status of the map indicating, if a map version is currently used on
/// the mobile robot. ENABLED: Indicates this map is currently active / used on the mobile
/// robot. At most one map with the same mapId can have its status set to
/// ENABLED.<br>DISABLED: Indicates this map version is currently not enabled on the mobile
/// robot and thus could be enabled or deleted by request.
///
/// ENABLED: Indicates this zone set is currently active / used on the mobile robot. At most
/// one zone set for each map can have its status set to ENABLED. DISABLED: Indicates this
/// zone set is currently not enabled on the mobile robot and thus could be enabled or
/// deleted by fleet control.
#[derive(Serialize, Deserialize)]
pub enum Status {
    #[serde(rename = "DISABLED")]
    Disabled,
    #[serde(rename = "ENABLED")]
    Enabled,
}

/// Defines the position on a map in world coordinates. Each floor has its own map.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MobileRobotPosition {
    /// Value for position deviation range in meters. Optional for vehicles that cannot estimate
    /// their deviation, e.g., grid-based localization. Only for logging and visualization
    /// purposes.
    deviation_range: Option<f64>,
    /// Describes the quality of the localization and therefore, can be used, e.g., by SLAM
    /// mobile robot to describe how accurate the current position information is.
    /// 0.0: position unknown
    /// 1.0: position known
    /// Optional for vehicles that cannot estimate their localization score.
    /// Only for logging and visualization purposes
    localization_score: Option<f64>,
    /// True: mobile robot is localized. x, y, and theta can be trusted. False: mobile robot is
    /// not localized. x, y, and theta cannot be trusted.
    localized: bool,
    /// Unique identification of the map.
    map_id: String,
    pub theta: f64,
    pub x: f64,
    pub y: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeState {
    /// A user-defined, human-readable name or descriptor.
    node_descriptor: Option<String>,
    /// Unique node identification
    node_id: String,
    /// Node position. Optional: Fleet control has this information. Can be sent additionally,
    /// e.g., for debugging purposes.
    node_position: Option<NodePosition>,
    /// True: indicates that the node is part of the base. False: indicates that the node is part
    /// of the horizon.
    released: bool,
    /// Sequence ID to discern multiple nodes with same nodeId.
    sequence_id: i64,
}

/// Node position. Optional: Fleet control has this information. Can be sent additionally,
/// e.g., for debugging purposes.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodePosition {
    map_id: String,
    theta: Option<f64>,
    x: f64,
    y: f64,
}

/// Current operating mode of the mobile robot.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OperatingMode {
    Automatic,
    Intervened,
    Manual,
    Semiautomatic,
    Service,
    Startup,
    #[serde(rename = "TEACH_IN")]
    TeachIn,
}

/// Represents a path within the robot's currently active order as NURBS.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedPath {
    pub trajectory: Trajectory,
    /// Array of nodeIds as communicated in the currently executed order that are traversed
    /// within the shared planned path.
    traversed_nodes: Option<Vec<String>>,
}

/// Contains all battery-related information.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerSupply {
    /// Battery current in Ampere (A)
    battery_current: Option<f64>,
    /// State of health in percent.
    battery_health: Option<f64>,
    /// Battery voltage
    battery_voltage: Option<f64>,
    /// True: charging in progress. False: mobile robot is currently not charging.
    charging: bool,
    /// Estimated reach with current State of Charge in meter.
    range: Option<f64>,
    /// State of Charge in %: If mobile robot only provides values for good or bad battery
    /// levels, these will be indicated as 20% (bad) and 80% (good).
    state_of_charge: f64,
}

/// Contains all safety-related information.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetyState {
    /// EmergencyStop-Types:  MANUAL: e-stop shall be acknowledged manually at the mobile robot.
    /// REMOTE: facility e-stop shall be acknowledged remotely. NONE: no e-stop activated.
    active_emergency_stop: ActiveEmergencyStop,
    /// Protective field violation. True: field is violated. False: field is not violated.
    field_violation: bool,
}

/// EmergencyStop-Types:  MANUAL: e-stop shall be acknowledged manually at the mobile robot.
/// REMOTE: facility e-stop shall be acknowledged remotely. NONE: no e-stop activated.
#[derive(Serialize, Deserialize)]
pub enum ActiveEmergencyStop {
    #[serde(rename = "MANUAL")]
    Manual,
    #[serde(rename = "NONE")]
    None,
    #[serde(rename = "REMOTE")]
    Remote,
}

/// The mobile robot's velocity in mobile robot coordinates
#[derive(Serialize, Deserialize)]
pub struct Velocity {
    /// The mobile robot's turning speed around its z axis.
    omega: Option<f64>,
    /// The mobile robot's velocity in its x direction
    vx: Option<f64>,
    /// The mobile robot's velocity in its y direction
    vy: Option<f64>,
}

/// Zone information sent by the mobile robot to fleet control.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneRequest {
    /// Unique per mobile robot identifier within all active requests.
    request_id: String,
    /// When stating a request, this is set to REQUESTED. After response or update from fleet
    /// control set to GRANTED or REVOKED. If lease time expires, shall be to EXPIRED.
    request_status: RequestStatus,
    /// Enum specifying the type of zone the request relates to. Feasible values are ACCESS or
    /// REPLANNING.
    request_type: ZoneRequestRequestType,
    trajectory: Option<Trajectory>,
    /// Locally (within the zone set) unique identifier referencing the zone the request is
    /// related to.
    zone_id: String,
    /// Due to the zoneId only being unique to a zoneSet, the zoneSetId is part of the request.
    zone_set_id: String,
}

/// Enum specifying the type of zone the request relates to. Feasible values are ACCESS or
/// REPLANNING.
#[derive(Serialize, Deserialize)]
pub enum ZoneRequestRequestType {
    #[serde(rename = "ACCESS")]
    Access,
    #[serde(rename = "REPLANNING")]
    Replanning,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneSet {
    /// Identifier of the corresponding map.
    map_id: String,
    /// Unique identifier of the zone set that is currently enabled for the map.<br> This field
    /// shall be left empty only if the mobile robot has no zones defined for the corresponding
    /// map.
    zone_set_id: String,
    /// ENABLED: Indicates this zone set is currently active / used on the mobile robot. At most
    /// one zone set for each map can have its status set to ENABLED. DISABLED: Indicates this
    /// zone set is currently not enabled on the mobile robot and thus could be enabled or
    /// deleted by fleet control.
    zone_set_status: Status,
}
