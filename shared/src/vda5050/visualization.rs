use serde::{Serialize, Deserialize};

/// Mobile robot position and/or velocity for visualization purposes. Can be published at a
/// higher rate if wanted. Since bandwidth may be expensive depening on the update rate for
/// this topic, all fields are optional.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Visualization {
    /// headerId of the message. The headerId is defined per topic and incremented by 1 with each
    /// sent (but not necessarily received) message.
    header_id: i64,
    intermediate_path: Option<IntermediatePath>,
    /// Manufacturer of the mobile robot
    manufacturer: String,
    mobile_robot_position: Option<MobileRobotPosition>,
    planned_path: Option<PlannedPath>,
    /// Header ID of the state message this visualization message refers to.
    reference_state_header_id: i64,
    /// Serial number of the mobile robot.
    serial_number: String,
    /// Timestamp in ISO8601 format (YYYY-MM-DDTHH:mm:ss.fffZ).
    timestamp: String,
    velocity: Option<Velocity>,
    /// Version of the protocol [Major].[Minor].[Patch]
    version: String,
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
    theta: f64,
    x: f64,
    y: f64,
}

/// Represents a path within the robot's currently active order as NURBS.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedPath {
    trajectory: Trajectory,
    /// Array of nodeIds as communicated in the currently executed order that are traversed
    /// within the shared planned path.
    traversed_nodes: Option<Vec<String>>,
}

/// The trajectory is to be communicated as a NURBS and is defined in chapter 6.7
/// Implementation of the Order message. Trajectory segments reach from the point, where the
/// mobile robot starts to enter the edge to the point where it reports that the next node
/// was traversed.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trajectory {
    /// List of JSON controlPoint objects defining the control points of the NURBS, which
    /// includes the beginning and end point.
    control_points: Vec<ControlPoint>,
    /// Defines the number of control points that influence any given point on the curve.
    /// Increasing the degree increases differentiability. If not defined, the default value is 1.
    degree: Option<i64>,
    /// Sequence of parameter values that determine where and how the control points affect the
    /// NURBS curve. knotVector has size of number of control points + degree + 1.
    knot_vector: Option<Vec<f64>>,
}

#[derive(Serialize, Deserialize)]
pub struct ControlPoint {
    /// The weight, with which this control point pulls on the curve. When not defined, the
    /// default will be 1.0.
    weight: Option<f64>,
    x: f64,
    y: f64,
}

/// The mobile robot's velocity in mobile robot's coordinates
#[derive(Serialize, Deserialize)]
pub struct Velocity {
    /// The mobile robot's turning speed around its Z-axis.
    omega: Option<f64>,
    /// The mobile robot's velocity in its X-direction.
    vx: Option<f64>,
    /// The mobile robot's velocity in its Y-direction.
    vy: Option<f64>,
}
