use std::collections::HashMap;

use crate::config::{MapConfig, NodeDef};
use crate::nurbs::{open_uniform_knots, ControlPoint, NurbsCurve};

#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    pub x: f64,
    pub y: f64,
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
}

impl NodeMap {
    pub fn build(config: &MapConfig) -> Self {
        let nodes: HashMap<String, Node> = config
            .nodes
            .iter()
            .map(|n: &NodeDef| {
                (
                    n.id.clone(),
                    Node {
                        id: n.id.clone(),
                        x: n.x,
                        y: n.y,
                    },
                )
            })
            .collect();

        let mut edges = HashMap::new();
        let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();

        for edef in &config.edges {
            let from_node = &nodes[&edef.from];
            let to_node = &nodes[&edef.to];

            let dx = to_node.x - from_node.x;
            let dy = to_node.y - from_node.y;
            let straight_len = (dx * dx + dy * dy).sqrt();

            let (curve, arc_table, length) = if let Some(cps) = &edef.control_points {
                let control_points: Vec<ControlPoint> = cps
                    .iter()
                    .map(|cp| ControlPoint {
                        x: cp.x,
                        y: cp.y,
                        weight: cp.w,
                    })
                    .collect();
                let n = control_points.len();
                let degree = (n - 1).min(3);
                let knots = open_uniform_knots(n, degree);
                let nurbs = NurbsCurve {
                    degree,
                    knots,
                    control_points,
                };
                let table = nurbs.arc_length_table(50);
                let arc_len = table.last().map(|(s, _)| *s).unwrap_or(straight_len);
                (Some(nurbs), Some(table), arc_len)
            } else {
                (None, None, straight_len)
            };

            let edge = Edge {
                id: edef.id.clone(),
                from: edef.from.clone(),
                to: edef.to.clone(),
                max_speed: edef.max_speed,
                curve,
                arc_table,
                length,
            };
            edges.insert(edef.id.clone(), edge);
            adjacency
                .entry(edef.from.clone())
                .or_default()
                .push(edef.id.clone());
        }

        NodeMap {
            nodes,
            edges,
            adjacency,
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
