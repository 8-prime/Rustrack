//! Project a LIF layout into a form a client can draw directly.
//!
//! This is a sibling of [`crate::lif::resolve`], with a different audience.
//! `resolve` flattens the graph for *one vehicle type* so a simulation can
//! drive it. A map is a *picture*: it should show the whole track, so this
//! takes the union across vehicle types and keeps every node and edge.
//!
//! It also finishes the geometry. An edge's trajectory is a NURBS curve, and
//! evaluating one needs [`crate::nurbs`]; rather than push that math out to
//! every client, edges are tessellated here into plain polylines. What comes
//! out is points and lines in metres, plus the bounding box needed to fit them
//! to a viewport — no LIF concepts left to interpret.
//!
//! The result is small: a layout whose source document is tens of megabytes
//! projects to a few hundred kilobytes, since descriptions, actions, and the
//! per-vehicle-type properties all drop out.

use std::collections::HashMap;

use serde::Serialize;

use crate::lif::model::{Lif, Trajectory};
use crate::nurbs::{ControlPoint, NurbsCurve};

/// Target length in metres of one tessellated curve segment.
const CURVE_SEGMENT_METRES: f64 = 0.25;
/// Floor on segments per curve, so a short curve is still recognisably curved.
const MIN_CURVE_SEGMENTS: usize = 4;
/// Ceiling on segments per curve, so one long sweep cannot dominate the payload.
const MAX_CURVE_SEGMENTS: usize = 32;

/// Axis-aligned extent of everything drawable in a layout, in metres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl Bounds {
    /// Grow to include a point, starting from a degenerate box at the first one.
    fn extend(slot: &mut Option<Bounds>, x: f64, y: f64) {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        match slot {
            None => {
                *slot = Some(Bounds {
                    min_x: x,
                    min_y: y,
                    max_x: x,
                    max_y: y,
                })
            }
            Some(b) => {
                if x < b.min_x {
                    b.min_x = x;
                }
                if x > b.max_x {
                    b.max_x = x;
                }
                if y < b.min_y {
                    b.min_y = y;
                }
                if y > b.max_y {
                    b.max_y = y;
                }
            }
        }
    }
}

/// A drawable node.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapNode {
    pub id: String,
    pub x: f64,
    pub y: f64,
    /// Orientation in radians, if any vehicle type constrains it. A hint for
    /// drawing a heading tick — not a constraint the client needs to honour.
    pub theta: Option<f64>,
}

/// A drawable edge, already reduced to a polyline.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapEdge {
    pub id: String,
    pub from: String,
    pub to: String,
    /// At least two `[x, y]` points, running start to end. A straight edge has
    /// exactly two; an authored trajectory is tessellated into more.
    pub points: Vec<[f64; 2]>,
}

/// A drawable station.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapStation {
    pub id: String,
    pub name: Option<String>,
    pub x: f64,
    pub y: f64,
}

/// One layout, ready to render.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapView {
    pub layout_id: String,
    pub layout_name: Option<String>,
    /// Every layout id in the document, so a client can offer a layer selector
    /// without fetching again. Layouts on different `layoutLevelId`s are
    /// disjoint graphs — typically one per floor.
    pub available_layouts: Vec<String>,
    /// `None` only when the layout has no drawable geometry at all.
    pub bounds: Option<Bounds>,
    pub nodes: Vec<MapNode>,
    pub edges: Vec<MapEdge>,
    pub stations: Vec<MapStation>,
}

impl Lif {
    /// Project one layout into drawable geometry.
    ///
    /// `layout_id` selects the layout; an unknown or absent id falls back to
    /// the first layout, since a viewer should show *something* rather than
    /// nothing. Returns `None` only for a document with no layouts at all.
    ///
    /// Unlike [`Lif::resolve`], this does not filter by vehicle type and does
    /// not error: a layout that resolves to nothing for every vehicle type
    /// still draws fine.
    pub fn map_view(&self, layout_id: Option<&str>) -> Option<MapView> {
        let layout = layout_id
            .and_then(|id| self.layouts.iter().find(|l| l.layout_id == id))
            .or_else(|| self.layouts.first())?;

        let mut bounds = None;

        let positions: HashMap<&str, (f64, f64)> = layout
            .nodes
            .iter()
            .map(|n| {
                (
                    n.node_id.as_str(),
                    (n.node_position.x, n.node_position.y),
                )
            })
            .collect();

        let nodes: Vec<MapNode> = layout
            .nodes
            .iter()
            .map(|n| {
                Bounds::extend(&mut bounds, n.node_position.x, n.node_position.y);
                MapNode {
                    id: n.node_id.clone(),
                    x: n.node_position.x,
                    y: n.node_position.y,
                    // Vehicle types may disagree; any constraint is a better
                    // drawing hint than none.
                    theta: n
                        .vehicle_type_node_properties
                        .iter()
                        .find_map(|p| p.theta),
                }
            })
            .collect();

        let mut edges = Vec::with_capacity(layout.edges.len());
        for edge in &layout.edges {
            // An edge whose endpoints are missing has no geometry to draw.
            // `validate` rejects these on upload, but a stored document may
            // predate a rule, and a map is not the place to start failing.
            let (Some(&start), Some(&end)) = (
                positions.get(edge.start_node_id.as_str()),
                positions.get(edge.end_node_id.as_str()),
            ) else {
                continue;
            };

            // Geometry is authored per vehicle type, but the curve is a
            // property of the track — take the first one offered.
            let points = edge
                .vehicle_type_edge_properties
                .iter()
                .find_map(|p| p.trajectory.as_ref())
                .and_then(tessellate)
                .unwrap_or_else(|| vec![[start.0, start.1], [end.0, end.1]]);

            for p in &points {
                Bounds::extend(&mut bounds, p[0], p[1]);
            }

            edges.push(MapEdge {
                id: edge.edge_id.clone(),
                from: edge.start_node_id.clone(),
                to: edge.end_node_id.clone(),
                points,
            });
        }

        let mut stations = Vec::with_capacity(layout.stations.len());
        for station in &layout.stations {
            // `stationPosition` is optional; a station without one is drawn at
            // the node you interact with it from, which is where it appears to
            // be anyway.
            let Some((x, y)) = station
                .station_position
                .map(|p| (p.x, p.y))
                .or_else(|| {
                    station
                        .interaction_node_ids
                        .first()
                        .and_then(|id| positions.get(id.as_str()).copied())
                })
            else {
                continue;
            };

            Bounds::extend(&mut bounds, x, y);
            stations.push(MapStation {
                id: station.station_id.clone(),
                name: station.station_name.clone(),
                x,
                y,
            });
        }

        Some(MapView {
            layout_id: layout.layout_id.clone(),
            layout_name: layout.layout_name.clone(),
            available_layouts: self.layouts.iter().map(|l| l.layout_id.clone()).collect(),
            bounds,
            nodes,
            edges,
            stations,
        })
    }
}

/// Sample a trajectory into a polyline, or `None` if it cannot be evaluated.
///
/// A `None` return leaves the caller to fall back to a straight line, which is
/// the right degradation: a malformed curve should cost fidelity, not the edge.
fn tessellate(trajectory: &Trajectory) -> Option<Vec<[f64; 2]>> {
    let curve = NurbsCurve {
        degree: trajectory.degree,
        // Authored by the exporter. Never regenerate it with
        // open_uniform_knots(), which would silently change the curve's shape.
        knots: trajectory.knot_vector.clone(),
        control_points: trajectory
            .control_points
            .iter()
            .map(|cp| ControlPoint {
                x: cp.x,
                y: cp.y,
                weight: cp.weight,
            })
            .collect(),
    };

    // evaluate() indexes the knot vector directly and would panic on a curve
    // whose parts disagree, so this guard is load-bearing.
    if !curve.is_valid() {
        return None;
    }

    let segments = segment_count(&curve.control_points);
    // The parameter domain is whatever the exporter authored — not necessarily
    // 0..1 — and it is private to NurbsCurve. arc_length_table walks the domain
    // and hands back the parameter at each sample, so take the parameters from
    // there rather than assuming a range.
    Some(
        curve
            .arc_length_table(segments)
            .into_iter()
            .map(|(_, u)| {
                let (x, y) = curve.evaluate(u);
                [x, y]
            })
            .collect(),
    )
}

/// Pick a sample count from the control polygon's length, which bounds the
/// curve's own length. A long sweep gets more segments than a short spur, so
/// neither goes visibly faceted nor pays for detail it cannot show.
fn segment_count(control_points: &[ControlPoint]) -> usize {
    let length: f64 = control_points
        .windows(2)
        .map(|w| ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt())
        .sum();

    if !length.is_finite() {
        return MIN_CURVE_SEGMENTS;
    }
    ((length / CURVE_SEGMENT_METRES).ceil() as usize).clamp(MIN_CURVE_SEGMENTS, MAX_CURVE_SEGMENTS)
}
