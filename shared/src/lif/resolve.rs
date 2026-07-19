//! Collapse LIF's per-vehicle-type indirection into a flat, directly usable graph.
//!
//! In LIF, traversability, speed limits, and orientation are not properties of
//! a node or edge — they hang off `vehicleType*Properties` arrays keyed by
//! vehicle type. A node with no entry for a given vehicle type is simply not
//! available to it. Resolving against one vehicle type turns that into an
//! ordinary directed graph.
//!
//! The result is also far smaller than the source document: descriptions,
//! actions, unmatched vehicle types, and station metadata are dropped. That is
//! what makes it viable to keep resident even when the source file is tens of
//! megabytes.

use std::collections::HashSet;

use crate::lif::error::{LifError, LifErrors};
use crate::lif::model::Lif;
use crate::nurbs::{ControlPoint, NurbsCurve};

/// Fallback when an edge declares no `maxSpeed` for the vehicle type.
pub const DEFAULT_MAX_SPEED: f64 = 1.5;

/// A node usable by the resolved vehicle type.
#[derive(Debug, Clone)]
pub struct ResolvedNode {
    pub node_id: String,
    pub x: f64,
    pub y: f64,
    /// Required orientation at this node, in radians. `None` means unconstrained.
    pub theta: Option<f64>,
    /// Coordinate frame the position is expressed in.
    pub map_id: Option<String>,
}

/// An edge traversable by the resolved vehicle type.
#[derive(Debug, Clone)]
pub struct ResolvedEdge {
    pub edge_id: String,
    pub start_node_id: String,
    pub end_node_id: String,
    pub max_speed: f64,
    /// Authored geometry. `None` means a straight line between the endpoints.
    pub curve: Option<NurbsCurve>,
}

/// A layout flattened to a single vehicle type.
#[derive(Debug, Clone)]
pub struct ResolvedLayout {
    pub layout_id: String,
    pub layout_name: Option<String>,
    pub nodes: Vec<ResolvedNode>,
    pub edges: Vec<ResolvedEdge>,
    /// Edges dropped because an endpoint was not available to this vehicle type.
    /// Not an error — it is how LIF expresses a restricted sub-network — but
    /// worth surfacing, since a large count usually means the wrong vehicle
    /// type was selected.
    pub edges_excluded: usize,
    /// Nodes dropped because they declare no properties for this vehicle type.
    pub nodes_excluded: usize,
}

impl Lif {
    /// Flatten one layout to one vehicle type.
    ///
    /// `layout_id` may be `None` when the file contains exactly one layout.
    /// Call [`crate::lif::validate`] first — this assumes node references
    /// resolve and trajectories are well-formed.
    pub fn resolve(
        &self,
        layout_id: Option<&str>,
        vehicle_type_id: &str,
    ) -> Result<ResolvedLayout, LifErrors> {
        let layout = self.layout(layout_id).ok_or_else(|| {
            LifErrors::single(LifError::NoSuchLayout {
                requested: layout_id.map(str::to_string),
                available: self.layouts.iter().map(|l| l.layout_id.clone()).collect(),
            })
        })?;

        let mut nodes = Vec::new();
        let mut kept_ids: HashSet<&str> = HashSet::with_capacity(layout.nodes.len());
        let mut nodes_excluded = 0;

        for node in &layout.nodes {
            let Some(props) = node
                .vehicle_type_node_properties
                .iter()
                .find(|p| p.vehicle_type_id == vehicle_type_id)
            else {
                nodes_excluded += 1;
                continue;
            };
            kept_ids.insert(node.node_id.as_str());
            nodes.push(ResolvedNode {
                node_id: node.node_id.clone(),
                x: node.node_position.x,
                y: node.node_position.y,
                theta: props.theta,
                map_id: node.map_id.clone(),
            });
        }

        let mut edges = Vec::new();
        let mut edges_excluded = 0;

        for edge in &layout.edges {
            let Some(props) = edge
                .vehicle_type_edge_properties
                .iter()
                .find(|p| p.vehicle_type_id == vehicle_type_id)
            else {
                edges_excluded += 1;
                continue;
            };
            // An edge is only usable if both of its endpoints are.
            if !kept_ids.contains(edge.start_node_id.as_str())
                || !kept_ids.contains(edge.end_node_id.as_str())
            {
                edges_excluded += 1;
                continue;
            }

            let curve = props.trajectory.as_ref().map(|t| NurbsCurve {
                degree: t.degree,
                // The knot vector is authored by the exporter — never
                // regenerate it with open_uniform_knots(), which would silently
                // change the curve's shape.
                knots: t.knot_vector.clone(),
                control_points: t
                    .control_points
                    .iter()
                    .map(|cp| ControlPoint {
                        x: cp.x,
                        y: cp.y,
                        weight: cp.weight,
                    })
                    .collect(),
            });

            edges.push(ResolvedEdge {
                edge_id: edge.edge_id.clone(),
                start_node_id: edge.start_node_id.clone(),
                end_node_id: edge.end_node_id.clone(),
                max_speed: props.max_speed.unwrap_or(DEFAULT_MAX_SPEED),
                curve,
            });
        }

        if nodes.is_empty() {
            return Err(LifErrors::single(LifError::NoSuchVehicleType {
                vehicle_type_id: vehicle_type_id.to_string(),
                available: declared_vehicle_types(layout),
            }));
        }

        Ok(ResolvedLayout {
            layout_id: layout.layout_id.clone(),
            layout_name: layout.layout_name.clone(),
            nodes,
            edges,
            edges_excluded,
            nodes_excluded,
        })
    }
}

/// Every vehicle type mentioned anywhere in the layout, for error messages.
fn declared_vehicle_types(layout: &crate::lif::model::Layout) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut set: HashSet<&str> = HashSet::new();
    for node in &layout.nodes {
        for p in &node.vehicle_type_node_properties {
            if set.insert(p.vehicle_type_id.as_str()) {
                seen.push(p.vehicle_type_id.clone());
            }
        }
    }
    for edge in &layout.edges {
        for p in &edge.vehicle_type_edge_properties {
            if set.insert(p.vehicle_type_id.as_str()) {
                seen.push(p.vehicle_type_id.clone());
            }
        }
    }
    seen
}
