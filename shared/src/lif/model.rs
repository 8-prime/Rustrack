//! LIF 1.0.0 data model — a faithful mirror of the specification.
//!
//! These types deserialize an uploaded `.lif` file as-authored. They keep the
//! spec's per-vehicle-type indirection intact; see [`crate::lif::resolve`] for
//! the flattened form that is actually convenient to drive a simulation from.
//!
//! Unlike the `vda5050` module these derive `Serialize` as well, because the
//! backend serves stored layouts back out to clients.

use serde::{Deserialize, Serialize};

/// Root of a LIF file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lif {
    pub meta_information: MetaInformation,
    pub layouts: Vec<Layout>,
}

/// Provenance of the file: who exported it, when, and against which spec version.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaInformation {
    /// Human-readable name of the project this layout belongs to.
    pub project_identification: String,
    /// The tool or integrator that produced the file.
    pub creator: String,
    /// ISO 8601 UTC timestamp of the export.
    pub export_timestamp: String,
    /// Semantic version of the LIF specification the file conforms to.
    pub lif_version: String,
}

/// A single track layout. A file may carry several, e.g. one per floor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub layout_id: String,
    pub layout_version: Option<String>,
    pub layout_name: Option<String>,
    /// Identifies the floor/level. Layouts on different levels are disjoint graphs.
    pub layout_level_id: Option<String>,
    pub layout_description: Option<String>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    #[serde(default)]
    pub stations: Vec<Station>,
}

/// A point the vehicle can occupy. Traversability is per vehicle type — see
/// [`VehicleTypeNodeProperties`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub node_id: String,
    pub node_name: Option<String>,
    pub node_description: Option<String>,
    /// Identifies the coordinate frame the position is expressed in.
    pub map_id: Option<String>,
    pub node_position: NodePosition,
    pub vehicle_type_node_properties: Vec<VehicleTypeNodeProperties>,
}

/// Position in metres, in the frame named by the node's `map_id`.
///
/// LIF is 2D only; verticality is expressed via `Layout::layout_level_id` and
/// the clearance fields on [`VehicleTypeEdgeProperties`], not a `z` coordinate.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodePosition {
    pub x: f64,
    pub y: f64,
}

/// Per-vehicle-type view of a node. A vehicle type with no entry here cannot
/// use the node at all.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VehicleTypeNodeProperties {
    pub vehicle_type_id: String,
    /// Absolute orientation the vehicle must hold at this node, in **radians**
    /// (−π…π). Note the unit differs from
    /// [`VehicleTypeEdgeProperties::vehicle_orientation`], which is in degrees.
    pub theta: Option<f64>,
    #[serde(default)]
    pub actions: Vec<Action>,
}

/// A directed connection between two nodes. Bidirectional travel requires two
/// edges.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub edge_id: String,
    pub start_node_id: String,
    pub end_node_id: String,
    pub vehicle_type_edge_properties: Vec<VehicleTypeEdgeProperties>,
}

/// Per-vehicle-type view of an edge. A vehicle type with no entry here cannot
/// traverse the edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VehicleTypeEdgeProperties {
    pub vehicle_type_id: String,
    /// Orientation the vehicle holds while traversing, in **degrees** (0…360).
    /// Note the unit differs from [`VehicleTypeNodeProperties::theta`], which is
    /// in radians.
    pub vehicle_orientation: Option<f64>,
    /// How `vehicle_orientation` is interpreted, e.g. `TANGENTIAL`.
    pub orientation_type: Option<String>,
    pub rotation_allowed: Option<bool>,
    pub rotation_at_start_node_allowed: Option<bool>,
    pub rotation_at_end_node_allowed: Option<bool>,
    /// Metres per second.
    pub max_speed: Option<f64>,
    /// Radians per second.
    pub max_rotation_speed: Option<f64>,
    /// Clearance limits in metres — constraints on the load, not elevation.
    pub min_height: Option<f64>,
    pub max_height: Option<f64>,
    pub load_restriction: Option<LoadRestriction>,
    /// Geometry of the edge. Absent means a straight line between the endpoints.
    pub trajectory: Option<Trajectory>,
    #[serde(default)]
    pub actions: Vec<Action>,
    /// Whether the vehicle may re-enter this edge.
    #[serde(default = "default_true")]
    pub reentry_allowed: bool,
}

/// Whether the edge may be traversed loaded, unloaded, or both.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadRestriction {
    pub unloaded: bool,
    pub loaded: bool,
    #[serde(default)]
    pub load_set_names: Vec<String>,
}

/// NURBS geometry of an edge.
///
/// This is field-for-field convertible to [`crate::nurbs::NurbsCurve`], which
/// provides the actual evaluation. Note the knot vector is **authored** here
/// rather than synthesized — do not overwrite it with
/// [`crate::nurbs::open_uniform_knots`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trajectory {
    #[serde(default = "default_degree")]
    pub degree: usize,
    pub knot_vector: Vec<f64>,
    pub control_points: Vec<LifControlPoint>,
}

/// A weighted NURBS control point.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LifControlPoint {
    pub x: f64,
    pub y: f64,
    #[serde(default = "default_weight")]
    pub weight: f64,
}

/// A point of interest referencing one or more nodes. Stations are an overlay
/// on the graph, not routable members of it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Station {
    pub station_id: String,
    /// Nodes a vehicle can occupy in order to interact with this station.
    pub interaction_node_ids: Vec<String>,
    pub station_position: Option<StationPosition>,
    pub station_name: Option<String>,
    pub station_description: Option<String>,
    /// Metres.
    pub station_height: Option<f64>,
}

/// Position and orientation of a station.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationPosition {
    pub x: f64,
    pub y: f64,
    /// **Radians**, consistent with [`VehicleTypeNodeProperties::theta`].
    pub theta: Option<f64>,
}

/// An action to perform at a node or while traversing an edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub action_type: String,
    pub blocking_type: BlockingType,
    pub action_description: Option<String>,
    pub requirement_type: Option<RequirementType>,
    #[serde(default)]
    pub action_parameters: Vec<ActionParameter>,
}

/// A free-form key/value argument to an [`Action`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionParameter {
    pub key: String,
    pub value: serde_json::Value,
}

/// Whether the action permits concurrent driving and/or other actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum BlockingType {
    /// Action may run while driving and alongside other actions.
    None,
    /// Action may run alongside other actions but not while driving.
    Soft,
    /// Action runs alone; the vehicle must be stopped.
    Hard,
}

/// Whether master control must issue the action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum RequirementType {
    Required,
    Conditional,
    Optional,
}

fn default_true() -> bool {
    true
}

fn default_degree() -> usize {
    3
}

fn default_weight() -> f64 {
    1.0
}

impl Lif {
    /// Look up a layout by id, or return the sole layout when `layout_id` is
    /// `None` and the file contains exactly one.
    pub fn layout(&self, layout_id: Option<&str>) -> Option<&Layout> {
        match layout_id {
            Some(id) => self.layouts.iter().find(|l| l.layout_id == id),
            None => match self.layouts.as_slice() {
                [only] => Some(only),
                _ => None,
            },
        }
    }
}
