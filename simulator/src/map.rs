use std::collections::HashMap;

use rustrack_shared::lif::ResolvedLayout;

use crate::nurbs::NurbsCurve;

#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    pub x: f64,
    pub y: f64,
    /// Orientation the vehicle must hold at this node, in radians, when the
    /// layout specifies one. Headings along edges are still derived from
    /// geometry by [`NodeMap::heading_on_edge`].
    pub theta: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct Edge {
    pub id: String,
    pub from: String,
    pub to: String,
    pub max_speed: f64,
    /// Pre-computed arc-length table. Present when NURBS control points are defined.
    pub curve: Option<NurbsCurve>,
    pub arc_table: Option<Vec<(f64, f64)>>,
    /// Straight-line length (used for linear edges and as fallback).
    pub length: f64,
}

#[derive(Debug, Clone)]
pub struct NodeMap {
    pub nodes: HashMap<String, Node>,
    /// All edges, keyed by edge id.
    pub edges: HashMap<String, Edge>,
    /// Adjacency: from_node_id → list of edge_ids leaving that node.
    pub adjacency: HashMap<String, Vec<String>>,
    /// Coordinate frame reported in the VDA5050 state, taken from the layout.
    pub map_id: String,
}

/// Arc-length samples per curved edge.
///
/// This allocates `ARC_SAMPLES + 1` pairs for every curved edge, which on a
/// large layout is the simulator's dominant memory cost. Raise it only with
/// that in mind.
const ARC_SAMPLES: usize = 50;

impl NodeMap {
    /// Build the runtime graph from a LIF layout already resolved to one
    /// vehicle type.
    ///
    /// Assumes the source document passed `rustrack_shared::lif::validate` —
    /// edge endpoints are indexed directly, so a dangling reference would panic.
    pub fn build(layout: &ResolvedLayout) -> Self {
        let nodes: HashMap<String, Node> = layout
            .nodes
            .iter()
            .map(|n| {
                (
                    n.node_id.clone(),
                    Node {
                        id: n.node_id.clone(),
                        x: n.x,
                        y: n.y,
                        theta: n.theta,
                    },
                )
            })
            .collect();

        // LIF puts the coordinate frame on each node; the simulator reports a
        // single map, so take the first one declared and fall back to the
        // layout id.
        let map_id = layout
            .nodes
            .iter()
            .find_map(|n| n.map_id.clone())
            .unwrap_or_else(|| layout.layout_id.clone());

        let mut edges = HashMap::new();
        let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();

        for redge in &layout.edges {
            let from_node = &nodes[&redge.start_node_id];
            let to_node = &nodes[&redge.end_node_id];

            let dx = to_node.x - from_node.x;
            let dy = to_node.y - from_node.y;
            let straight_len = (dx * dx + dy * dy).sqrt();

            let (curve, arc_table, length) = match &redge.curve {
                // The knot vector is authored in the LIF file; use it as given.
                Some(nurbs) => {
                    let nurbs = nurbs.clone();
                    let table = nurbs.arc_length_table(ARC_SAMPLES);
                    let arc_len = table.last().map(|(s, _)| *s).unwrap_or(straight_len);
                    (Some(nurbs), Some(table), arc_len)
                }
                None => (None, None, straight_len),
            };

            let edge = Edge {
                id: redge.edge_id.clone(),
                from: redge.start_node_id.clone(),
                to: redge.end_node_id.clone(),
                max_speed: redge.max_speed,
                curve,
                arc_table,
                length,
            };
            edges.insert(redge.edge_id.clone(), edge);
            adjacency
                .entry(redge.start_node_id.clone())
                .or_default()
                .push(redge.edge_id.clone());
        }

        NodeMap {
            nodes,
            edges,
            adjacency,
            map_id,
        }
    }

    /// Find the edge id connecting `from` → `to`, if one exists.
    pub fn edge_between(&self, from: &str, to: &str) -> Option<&str> {
        self.adjacency.get(from)?.iter().find_map(|eid| {
            let e = &self.edges[eid];
            (e.to == to).then_some(eid.as_str())
        })
    }

    /// Evaluate position on an edge at arc-length `s` meters from the start node.
    pub fn position_on_edge(&self, edge: &Edge, s: f64) -> (f64, f64) {
        if let (Some(curve), Some(table)) = (&edge.curve, &edge.arc_table) {
            let t = NurbsCurve::t_for_arc_length(table, s);
            curve.evaluate(t)
        } else {
            // Linear interpolation between from and to nodes
            let from = &self.nodes[&edge.from];
            let to = &self.nodes[&edge.to];
            let t = if edge.length > 0.0 {
                (s / edge.length).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (from.x + t * (to.x - from.x), from.y + t * (to.y - from.y))
        }
    }

    /// Heading angle (theta) on an edge at arc-length `s`, computed from a small forward step.
    pub fn heading_on_edge(&self, edge: &Edge, s: f64) -> f64 {
        let epsilon = 0.01_f64.min(edge.length * 0.01).max(1e-6);
        let s1 = (s + epsilon).min(edge.length);
        let (x0, y0) = self.position_on_edge(edge, s);
        let (x1, y1) = self.position_on_edge(edge, s1);
        (y1 - y0).atan2(x1 - x0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustrack_shared::lif::{validate, Lif};
    use rustrack_shared::nurbs::{open_uniform_knots, ControlPoint};

    /// Build the map from the shipped example, the way the binary does.
    fn example_map() -> NodeMap {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/warehouse.lif");
        let raw = std::fs::read_to_string(path).expect("example layout should exist");
        let lif: Lif = serde_json::from_str(&raw).expect("example layout should be valid LIF");
        validate(&lif).expect("example layout should pass validation");
        let layout = lif
            .resolve(Some("warehouse"), "sim-agv")
            .expect("example layout should resolve for sim-agv");
        NodeMap::build(&layout)
    }

    #[test]
    fn example_layout_builds_expected_graph() {
        let map = example_map();
        assert_eq!(map.nodes.len(), 10);
        assert_eq!(map.edges.len(), 21);
        assert_eq!(map.map_id, "warehouse-floor-1");

        // Aisles are bidirectional, so every forward edge has a return.
        assert_eq!(map.edge_between("N01", "N02"), Some("E01"));
        assert_eq!(map.edge_between("N02", "N01"), Some("E01R"));
        // Non-adjacent nodes must not resolve to an edge.
        assert_eq!(map.edge_between("N01", "N05"), None);
    }

    #[test]
    fn node_positions_survive_the_lif_round_trip() {
        let map = example_map();
        let n10 = &map.nodes["N10"];
        assert_eq!((n10.x, n10.y), (20.0, -8.0));
    }

    /// Before LIF the simulator synthesized knot vectors with
    /// `open_uniform_knots`. The authored vector in the example must reproduce
    /// that curve exactly, or converting the map silently changed how AGVs move.
    #[test]
    fn curved_edge_matches_pre_lif_geometry() {
        let map = example_map();
        let e02n = &map.edges["E02N"];
        let curve = e02n.curve.as_ref().expect("E02N should be curved");

        let control_points = vec![
            ControlPoint { x: 5.0, y: 0.0, weight: 1.0 },
            ControlPoint { x: 7.5, y: 1.5, weight: 1.0 },
            ControlPoint { x: 10.0, y: 0.0, weight: 1.0 },
        ];
        // What the old code derived: degree = (n - 1).min(3), knots synthesized.
        let degree = (control_points.len() - 1).min(3);
        let legacy = NurbsCurve {
            degree,
            knots: open_uniform_knots(control_points.len(), degree),
            control_points,
        };

        assert_eq!(curve.degree, legacy.degree);
        assert_eq!(curve.knots, legacy.knots, "authored knots must match");
        assert!(
            (e02n.length - legacy.arc_length_table(ARC_SAMPLES).last().unwrap().0).abs() < 1e-9,
            "arc length must be unchanged"
        );

        // The curve must actually bow away from the straight chord.
        let (mx, my) = map.position_on_edge(e02n, e02n.length / 2.0);
        assert!(my > 0.5, "midpoint should bow upward, got y={my}");
        assert!((mx - 7.5).abs() < 1e-6);
    }

    /// A curved edge is longer than the straight chord, so its arc length must
    /// come from the curve rather than the endpoint distance.
    #[test]
    fn curved_edge_is_longer_than_its_chord() {
        let map = example_map();
        assert!(map.edges["E02N"].length > map.edges["E02"].length);
        assert!((map.edges["E02"].length - 5.0).abs() < 1e-9);
    }
}
