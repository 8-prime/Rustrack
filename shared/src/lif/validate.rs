//! Referential-integrity and geometry checks the JSON schema does not perform.
//!
//! All cross-references in LIF are plain strings and the schema validates none
//! of them. Downstream code indexes nodes by id directly, so a dangling
//! reference becomes a panic rather than an error unless it is caught here.

use std::collections::HashSet;

use crate::lif::error::{IdKind, LifError, LifErrors, MAX_REPORTED_ERRORS};
use crate::lif::model::Lif;

/// Accumulates problems up to the reporting cap, counting the rest.
struct Collector {
    errors: Vec<LifError>,
    truncated: usize,
}

impl Collector {
    fn new() -> Self {
        Collector {
            errors: Vec::new(),
            truncated: 0,
        }
    }

    fn push(&mut self, error: LifError) {
        if self.errors.len() < MAX_REPORTED_ERRORS {
            self.errors.push(error);
        } else {
            self.truncated += 1;
        }
    }

    fn finish(self) -> Result<(), LifErrors> {
        if self.errors.is_empty() && self.truncated == 0 {
            Ok(())
        } else {
            Err(LifErrors {
                errors: self.errors,
                truncated: self.truncated,
            })
        }
    }
}

/// Check a whole file: unique ids, resolvable node references, and evaluable
/// trajectories.
///
/// Runs in a single pass per layout and borrows ids from `lif` rather than
/// cloning them, so it stays cheap on layouts with ~10^5 nodes. Only ids that
/// appear in an actual error are cloned.
pub fn validate(lif: &Lif) -> Result<(), LifErrors> {
    let mut c = Collector::new();

    let mut seen_layouts: HashSet<&str> = HashSet::with_capacity(lif.layouts.len());
    for layout in &lif.layouts {
        if !seen_layouts.insert(layout.layout_id.as_str()) {
            c.push(LifError::DuplicateId {
                layout_id: layout.layout_id.clone(),
                kind: IdKind::Layout,
                id: layout.layout_id.clone(),
            });
        }

        let lid = layout.layout_id.as_str();

        // Node ids first — edges and stations are checked against this set.
        let mut node_ids: HashSet<&str> = HashSet::with_capacity(layout.nodes.len());
        for node in &layout.nodes {
            if !node_ids.insert(node.node_id.as_str()) {
                c.push(LifError::DuplicateId {
                    layout_id: lid.to_string(),
                    kind: IdKind::Node,
                    id: node.node_id.clone(),
                });
            }
        }

        let mut edge_ids: HashSet<&str> = HashSet::with_capacity(layout.edges.len());
        for edge in &layout.edges {
            if !edge_ids.insert(edge.edge_id.as_str()) {
                c.push(LifError::DuplicateId {
                    layout_id: lid.to_string(),
                    kind: IdKind::Edge,
                    id: edge.edge_id.clone(),
                });
            }

            for endpoint in [&edge.start_node_id, &edge.end_node_id] {
                if !node_ids.contains(endpoint.as_str()) {
                    c.push(LifError::UnknownNodeRef {
                        layout_id: lid.to_string(),
                        referenced_by: edge.edge_id.clone(),
                        node_id: endpoint.clone(),
                    });
                }
            }

            for props in &edge.vehicle_type_edge_properties {
                let Some(traj) = &props.trajectory else {
                    continue;
                };
                // These are the same invariants `NurbsCurve::is_valid` asserts,
                // checked against the trajectory directly so each failure can
                // report a specific reason — and so validation does not clone
                // control points on a layout with 10^5 edges.
                let expected_knots = traj.control_points.len() + traj.degree + 1;
                if traj.control_points.len() <= traj.degree {
                    c.push(LifError::InvalidTrajectory {
                        layout_id: lid.to_string(),
                        edge_id: edge.edge_id.clone(),
                        vehicle_type_id: props.vehicle_type_id.clone(),
                        reason: format!(
                            "degree {} needs more than {} control points, got {}",
                            traj.degree,
                            traj.degree,
                            traj.control_points.len()
                        ),
                    });
                } else if traj.knot_vector.len() != expected_knots {
                    c.push(LifError::InvalidTrajectory {
                        layout_id: lid.to_string(),
                        edge_id: edge.edge_id.clone(),
                        vehicle_type_id: props.vehicle_type_id.clone(),
                        reason: format!(
                            "knotVector must have controlPoints + degree + 1 = {} entries, got {}",
                            expected_knots,
                            traj.knot_vector.len()
                        ),
                    });
                } else if traj.knot_vector.windows(2).any(|w| w[1] < w[0]) {
                    c.push(LifError::InvalidTrajectory {
                        layout_id: lid.to_string(),
                        edge_id: edge.edge_id.clone(),
                        vehicle_type_id: props.vehicle_type_id.clone(),
                        reason: "knotVector must be non-decreasing".to_string(),
                    });
                }
            }
        }

        let mut station_ids: HashSet<&str> = HashSet::with_capacity(layout.stations.len());
        for station in &layout.stations {
            if !station_ids.insert(station.station_id.as_str()) {
                c.push(LifError::DuplicateId {
                    layout_id: lid.to_string(),
                    kind: IdKind::Station,
                    id: station.station_id.clone(),
                });
            }
            for node_id in &station.interaction_node_ids {
                if !node_ids.contains(node_id.as_str()) {
                    c.push(LifError::UnknownNodeRef {
                        layout_id: lid.to_string(),
                        referenced_by: station.station_id.clone(),
                        node_id: node_id.clone(),
                    });
                }
            }
        }
    }

    c.finish()
}
