//! A robot's remaining route, built from what it reports in its VDA5050 state.
//!
//! `nodeStates` and `edgeStates` list what is left of the order, so they shrink
//! as the robot drives: the route is always "from here on". Together they
//! alternate by `sequenceId`, and both geometry fields are optional in the spec.
//! Edges with a trajectory draw as that curve; everything else falls back to a
//! straight line between the node positions that are known. A vehicle reporting
//! neither has no drawable route.

use serde::Serialize;
use shared::vda5050::state::{EdgeState, NodeState};

/// The `nodeStates` and `edgeStates` of the latest state message.
#[derive(Default, PartialEq)]
pub struct PlannedPath {
    pub node_states: Vec<NodeState>,
    pub edge_states: Vec<EdgeState>,
}

/// The remaining route as one polyline, ready to draw.
#[derive(Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    /// World metres, in driving order.
    pub points: Vec<[f32; 2]>,
    /// How many leading points reach the next node. The robot is somewhere along
    /// these, so the renderer cuts away the part it has already driven.
    pub current_points: usize,
    /// How many leading points are released (the base). The rest are the
    /// horizon, which the robot may not drive yet.
    pub released_points: usize,
}

impl From<&PlannedPath> for Route {
    fn from(path: &PlannedPath) -> Self {
        enum Step<'a> {
            Node(&'a NodeState),
            Edge(&'a EdgeState),
        }

        let mut steps: Vec<(i64, Step)> = path
            .node_states
            .iter()
            .map(|n| (n.sequence_id, Step::Node(n)))
            .chain(
                path.edge_states
                    .iter()
                    .map(|e| (e.sequence_id, Step::Edge(e))),
            )
            .collect();
        steps.sort_by_key(|(sequence_id, _)| *sequence_id);

        let mut route = Route::default();
        let mut reached_node = false;

        for (_, step) in steps {
            let (released, added): (bool, Vec<[f64; 2]>) = match step {
                Step::Node(n) => (
                    n.released,
                    n.node_position.iter().map(|p| [p.x, p.y]).collect(),
                ),
                Step::Edge(e) => (
                    e.released,
                    e.trajectory
                        .as_ref()
                        .and_then(|t| t.curve().tessellate())
                        .unwrap_or_default(),
                ),
            };

            for [x, y] in added {
                if !x.is_finite() || !y.is_finite() {
                    continue;
                }
                let point = [x as f32, y as f32];
                // A trajectory ends on its node, so most joints would double up.
                if route.points.last() != Some(&point) {
                    route.points.push(point);
                }
            }

            if released {
                route.released_points = route.points.len();
            }
            if !reached_node {
                route.current_points = route.points.len();
                reached_node = matches!(step, Step::Node(_));
            }
        }

        route
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::vda5050::state::{ControlPoint, NodePosition, Trajectory};

    fn node(sequence_id: i64, released: bool, position: Option<(f64, f64)>) -> NodeState {
        NodeState {
            node_description: None,
            node_id: format!("n{sequence_id}"),
            node_position: position.map(|(x, y)| NodePosition {
                map_id: "map".into(),
                theta: None,
                x,
                y,
            }),
            released,
            sequence_id,
        }
    }

    fn edge(sequence_id: i64, released: bool, trajectory: Option<Trajectory>) -> EdgeState {
        EdgeState {
            edge_description: None,
            edge_id: format!("e{sequence_id}"),
            released,
            sequence_id,
            trajectory,
        }
    }

    /// A quadratic from (0, 0) to (2, 0), bowing up through (1, 1).
    fn curve() -> Trajectory {
        Trajectory {
            control_points: [(0.0, 0.0), (1.0, 2.0), (2.0, 0.0)]
                .map(|(x, y)| ControlPoint { x, y, weight: None })
                .into(),
            degree: 2,
            knot_vector: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        }
    }

    #[test]
    fn straight_edges_join_their_node_positions_in_sequence_order() {
        // Deliberately out of order: the spec doesn't promise a sorted list.
        let route = Route::from(&PlannedPath {
            node_states: vec![
                node(4, true, Some((2.0, 0.0))),
                node(2, true, Some((1.0, 0.0))),
            ],
            edge_states: vec![edge(3, true, None), edge(1, true, None)],
        });

        assert_eq!(route.points, vec![[1.0, 0.0], [2.0, 0.0]]);
        assert_eq!(route.current_points, 1);
        assert_eq!(route.released_points, 2);
    }

    #[test]
    fn a_trajectory_shapes_its_edge_without_doubling_the_end_node() {
        let route = Route::from(&PlannedPath {
            node_states: vec![node(2, true, Some((2.0, 0.0)))],
            edge_states: vec![edge(1, true, Some(curve()))],
        });

        assert_eq!(route.points.first(), Some(&[0.0, 0.0]));
        assert_eq!(route.points.last(), Some(&[2.0, 0.0]));
        assert!(route.points.iter().any(|p| p[1] > 0.5), "should follow the bow");
        assert_eq!(route.points.iter().filter(|p| **p == [2.0, 0.0]).count(), 1);
        assert_eq!(route.current_points, route.points.len());
    }

    #[test]
    fn horizon_points_are_not_counted_as_released() {
        let route = Route::from(&PlannedPath {
            node_states: vec![
                node(2, true, Some((1.0, 0.0))),
                node(4, false, Some((2.0, 0.0))),
                node(6, false, Some((3.0, 0.0))),
            ],
            edge_states: vec![edge(1, true, None), edge(3, false, None), edge(5, false, None)],
        });

        assert_eq!(route.points.len(), 3);
        assert_eq!(route.released_points, 1);
    }

    #[test]
    fn a_robot_stopped_on_a_node_starts_from_that_node() {
        let route = Route::from(&PlannedPath {
            node_states: vec![
                node(0, true, Some((0.0, 0.0))),
                node(2, true, Some((1.0, 0.0))),
            ],
            edge_states: vec![edge(1, true, None)],
        });

        assert_eq!(route.current_points, 1);
    }

    #[test]
    fn a_malformed_trajectory_falls_back_to_the_node_positions() {
        let mut broken = curve();
        broken.knot_vector = vec![1.0, 0.0, 1.0, 0.0, 1.0, 0.0];

        let route = Route::from(&PlannedPath {
            node_states: vec![node(2, true, Some((2.0, 0.0)))],
            edge_states: vec![edge(1, true, Some(broken))],
        });

        assert_eq!(route.points, vec![[2.0, 0.0]]);
    }

    #[test]
    fn nodes_without_positions_contribute_nothing() {
        let route = Route::from(&PlannedPath {
            node_states: vec![node(2, true, None), node(4, true, None)],
            edge_states: vec![edge(1, true, None), edge(3, true, None)],
        });

        assert_eq!(route, Route::default());
    }
}
