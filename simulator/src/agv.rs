use rustrack_shared::vda5050::state::{
    AgvPosition, BatteryState, ControlPoint, EStop, EdgeState, Error, ErrorLevel, NodePosition,
    NodeState, OperatingMode, SafetyState, State, Trajectory, Velocity,
};

use crate::{
    map::{Edge, NodeMap},
    scenario::Scenario,
};

/// Nodes an AGV visits before its order is complete and it takes the next one.
const NODES_PER_ORDER: u32 = 4;
/// Pause between two orders, in seconds. Long enough for an idle state to be seen.
const ORDER_DWELL_S: f64 = 2.0;
/// How many edges of the planned path are released; the rest are the horizon.
const RELEASED_EDGES: usize = 3;

enum Motion {
    Idle,
    Moving { edge_id: String, progress_m: f64 },
    Waiting { node_id: String, remaining_s: f64 },
}

pub struct AgvSimulator {
    pub serial: String,
    speed_m_s: f64,
    current_node: String,
    motion: Motion,
    scenario: Box<dyn Scenario>,
    battery_pct: f64,
    header_id: u32,
    order_number: u32,
    order_update_id: i64,
    /// Nodes reached so far in the current order.
    order_progress: u32,
    /// The order is done; the next one starts when the current dwell ends.
    order_complete: bool,
    /// Last computed pose (x, y, theta) — kept so Idle can report a stable position.
    last_pose: (f64, f64, f64),
}

impl AgvSimulator {
    pub fn new(
        serial: String,
        speed_m_s: f64,
        start_node: String,
        scenario: Box<dyn Scenario>,
        map: &NodeMap,
    ) -> Self {
        let pos = map
            .nodes
            .get(&start_node)
            .map(|n| (n.x, n.y, 0.0))
            .unwrap_or((0.0, 0.0, 0.0));
        AgvSimulator {
            serial,
            speed_m_s,
            current_node: start_node.clone(),
            motion: Motion::Waiting {
                node_id: start_node,
                remaining_s: 0.5,
            },
            scenario,
            battery_pct: 95.0,
            header_id: 0,
            order_number: 1,
            order_update_id: 0,
            order_progress: 0,
            order_complete: false,
            last_pose: pos,
        }
    }

    /// Advance the simulation by `dt` seconds and return the current VDA5050 state.
    pub fn tick(&mut self, dt: f64, map: &NodeMap) -> State {
        self.header_id += 1;

        match &mut self.motion {
            Motion::Idle => {}

            Motion::Waiting {
                node_id,
                remaining_s,
            } => {
                *remaining_s -= dt;
                if *remaining_s <= 0.0 {
                    let node_id = node_id.clone();
                    // Ask scenario for next target
                    if let Some(next) = self.scenario.next_target(&node_id, map) {
                        if let Some(edge_id) = map.edge_between(&node_id, &next) {
                            // Only now is there a next order; an AGV with nowhere
                            // to go keeps reporting the one it finished.
                            if self.order_complete {
                                self.order_number += 1;
                                self.order_update_id = 0;
                                self.order_progress = 0;
                                self.order_complete = false;
                            }
                            self.current_node = node_id;
                            self.motion = Motion::Moving {
                                edge_id: edge_id.to_string(),
                                progress_m: 0.0,
                            };
                        } else {
                            self.motion = Motion::Idle;
                        }
                    } else {
                        self.motion = Motion::Idle;
                    }
                }
            }

            Motion::Moving {
                edge_id,
                progress_m,
            } => {
                let edge = &map.edges[edge_id.as_str()];
                let effective_speed = self.speed_m_s.min(edge.max_speed);
                *progress_m += effective_speed * dt;

                if *progress_m >= edge.length {
                    let arrived_at = edge.to.clone();
                    self.current_node = arrived_at.clone();
                    self.last_pose = (map.nodes[&arrived_at].x, map.nodes[&arrived_at].y, 0.0);
                    self.order_progress += 1;
                    self.order_complete = self.order_progress >= NODES_PER_ORDER;
                    if !self.order_complete {
                        self.order_update_id += 1;
                    }
                    self.motion = Motion::Waiting {
                        node_id: arrived_at,
                        remaining_s: if self.order_complete {
                            ORDER_DWELL_S
                        } else {
                            0.5
                        },
                    };
                } else {
                    let (x, y) = map.position_on_edge(edge, *progress_m);
                    let theta = map.heading_on_edge(edge, *progress_m);
                    self.last_pose = (x, y, theta);
                    self.battery_pct = (self.battery_pct - 0.001 * dt).max(0.0);
                }
            }
        }

        self.build_state(map)
    }

    fn build_state(&self, map: &NodeMap) -> State {
        let (x, y, theta) = self.last_pose;
        let driving = matches!(self.motion, Motion::Moving { .. });

        let (distance_since_last_node, velocity) = match &self.motion {
            Motion::Moving {
                edge_id,
                progress_m,
            } => {
                let edge = &map.edges[edge_id.as_str()];
                let speed = self.speed_m_s.min(edge.max_speed);
                // VDA5050 velocity is in *vehicle* coordinates: vx is forward, vy
                // lateral (0 for a non-holonomic AGV), and omega is the yaw rate.
                // Derive omega from how fast the heading turns along the path:
                // omega = dtheta/ds * ds/dt = dtheta/ds * speed.
                let eps = 0.05_f64.min(edge.length).max(1e-6);
                let h0 = map.heading_on_edge(edge, *progress_m);
                let h1 = map.heading_on_edge(edge, (*progress_m + eps).min(edge.length));
                let omega = wrap_angle(h1 - h0) / eps * speed;
                (
                    Some(*progress_m),
                    Some(Velocity {
                        vx: Some(speed),
                        vy: Some(0.0),
                        omega: Some(omega),
                    }),
                )
            }
            _ => (None, None),
        };

        let (node_states, edge_states) = self.build_path(map);

        State {
            header_id: self.header_id as i64,
            timestamp: chrono::Utc::now().to_rfc3339(),
            version: "2.1.0".to_string(),
            manufacturer: "Rustrack-Sim".to_string(),
            serial_number: self.serial.clone(),
            order_id: format!("{}-order-{:03}", self.serial, self.order_number),
            order_update_id: self.order_update_id,
            last_node_id: self.current_node.clone(),
            last_node_sequence_id: 0,
            driving,
            paused: Some(false),
            new_base_request: Some(false),
            distance_since_last_node,
            agv_position: Some(AgvPosition {
                x,
                y,
                theta,
                map_id: map.map_id.clone(),
                map_description: None,
                position_initialized: true,
                localization_score: Some(1.0),
                deviation_range: None,
            }),
            velocity,
            node_states,
            edge_states,
            battery_state: BatteryState {
                battery_charge: self.battery_pct,
                charging: false,
                battery_voltage: None,
                battery_health: Some(100.0),
                reach: None,
            },
            errors: self.maybe_error(),
            safety_state: SafetyState {
                e_stop: EStop::None,
                field_violation: false,
            },
            operating_mode: OperatingMode::Automatic,
            action_states: vec![],
            information: None,
            loads: None,
            maps: None,
            zone_set_id: None,
        }
    }

    /// What is left of the order. As in VDA5050, a node leaves `nodeStates` once
    /// it is passed, so while driving the list starts with the edge in progress.
    fn build_path(&self, map: &NodeMap) -> (Vec<NodeState>, Vec<EdgeState>) {
        // VDA5050 has an idle AGV report empty lists while it keeps the last orderId.
        if self.order_complete || matches!(self.motion, Motion::Idle) {
            return (Vec::new(), Vec::new());
        }

        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut sequence_id = 0;
        // Nodes left in the order. The one being driven to has not been reached yet.
        let mut nodes_left = (NODES_PER_ORDER - self.order_progress) as usize;

        let mut at = match &self.motion {
            Motion::Moving { edge_id, .. } => {
                let edge = &map.edges[edge_id.as_str()];
                edges.push(self.build_edge_state(edge_id, edge, 1, true));
                nodes.push(self.build_node_state(&edge.to, 2, true, map));
                sequence_id = 2;
                nodes_left -= 1;
                edge.to.as_str()
            }
            _ => {
                nodes.push(self.build_node_state(&self.current_node, 0, true, map));
                self.current_node.as_str()
            }
        };

        for next in self.scenario.upcoming().iter().take(nodes_left) {
            let Some(edge_id) = map.edge_between(at, next) else {
                break;
            };
            let released = edges.len() < RELEASED_EDGES;
            let edge = &map.edges[edge_id];
            edges.push(self.build_edge_state(edge_id, edge, sequence_id + 1, released));
            nodes.push(self.build_node_state(next, sequence_id + 2, released, map));
            sequence_id += 2;
            at = next;
        }

        (nodes, edges)
    }

    fn build_node_state(
        &self,
        node_id: &str,
        sequence_id: i64,
        released: bool,
        map: &NodeMap,
    ) -> NodeState {
        NodeState {
            node_id: node_id.to_string(),
            sequence_id,
            released,
            node_position: map.nodes.get(node_id).map(|n| NodePosition {
                x: n.x,
                y: n.y,
                theta: Some(n.theta.unwrap_or(0.0)),
                map_id: map.map_id.clone(),
            }),
            node_description: None,
        }
    }

    fn build_edge_state(
        &self,
        edge_id: &str,
        edge: &Edge,
        sequence_id: i64,
        released: bool,
    ) -> EdgeState {
        let trajectory = edge.curve.as_ref().map(|nurbs| Trajectory {
            degree: nurbs.degree as i64,
            knot_vector: nurbs.knots.clone(),
            control_points: nurbs
                .control_points
                .iter()
                .map(|cp| ControlPoint {
                    x: cp.x,
                    y: cp.y,
                    weight: Some(cp.weight),
                })
                .collect(),
        });

        EdgeState {
            edge_id: edge_id.to_string(),
            sequence_id,
            released,
            trajectory,
            edge_description: None,
        }
    }

    fn maybe_error(&self) -> Vec<Error> {
        if self.battery_pct < 20.0 {
            vec![Error {
                error_type: "batteryLow".to_string(),
                error_level: ErrorLevel::Warning,
                error_description: Some(format!("Battery at {:.1}%", self.battery_pct)),
                error_hint: None,
                error_references: None,
            }]
        } else {
            Vec::new()
        }
    }
}

/// Wrap an angle difference into `[-pi, pi]` so a heading that crosses the
/// `+pi/-pi` seam doesn't produce a spurious near-`2pi` yaw rate.
fn wrap_angle(a: f64) -> f64 {
    use std::f64::consts::PI;
    let mut a = a % (2.0 * PI);
    if a > PI {
        a -= 2.0 * PI;
    } else if a < -PI {
        a += 2.0 * PI;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{map::tests::example_map, scenario::ScriptedScenario};

    fn agv(route: &[&str], looping: bool, map: &NodeMap) -> AgvSimulator {
        let route: Vec<String> = route.iter().map(|n| n.to_string()).collect();
        let start = route[0].clone();
        let scenario = Box::new(ScriptedScenario::new(route, looping));
        AgvSimulator::new("AGV".to_string(), 1.2, start, scenario, map)
    }

    fn tick_until(agv: &mut AgvSimulator, map: &NodeMap, done: impl Fn(&State) -> bool) -> State {
        for _ in 0..100_000 {
            let state = agv.tick(0.1, map);
            if done(&state) {
                return state;
            }
        }
        panic!("simulation never reached the expected state");
    }

    fn node_ids(state: &State) -> Vec<&str> {
        state
            .node_states
            .iter()
            .map(|n| n.node_id.as_str())
            .collect()
    }

    #[test]
    fn reported_path_ends_where_the_order_ends() {
        let map = example_map();
        let route = [
            "N01", "N02", "N03", "N04", "N05", "N10", "N09", "N08", "N07", "N06", "N01",
        ];
        let mut agv = agv(&route, true, &map);

        let state = tick_until(&mut agv, &map, |s| s.driving);
        assert_eq!(state.order_id, "AGV-order-001");
        assert_eq!(node_ids(&state), ["N02", "N03", "N04", "N05"]);
        assert_eq!(state.edge_states.len(), 4);
    }

    #[test]
    fn a_finished_order_goes_idle_then_gives_way_to_the_next() {
        let map = example_map();
        let route = [
            "N01", "N02", "N03", "N04", "N05", "N10", "N09", "N08", "N07", "N06", "N01",
        ];
        let mut agv = agv(&route, true, &map);

        let finished = tick_until(&mut agv, &map, |s| s.node_states.is_empty());
        assert_eq!(finished.order_id, "AGV-order-001");
        assert_eq!(finished.order_update_id, i64::from(NODES_PER_ORDER) - 1);
        assert!(finished.edge_states.is_empty());

        let next = tick_until(&mut agv, &map, |s| !s.node_states.is_empty());
        assert_eq!(next.order_id, "AGV-order-002");
        assert_eq!(next.order_update_id, 0);
    }

    #[test]
    fn an_agv_with_nowhere_to_go_keeps_its_finished_order() {
        let map = example_map();
        let mut agv = agv(&["N01", "N02", "N03", "N04", "N05"], false, &map);

        tick_until(&mut agv, &map, |s| s.node_states.is_empty());
        for _ in 0..100 {
            let state = agv.tick(0.1, &map);
            assert_eq!(state.order_id, "AGV-order-001");
            assert!(state.node_states.is_empty());
        }
    }
}
