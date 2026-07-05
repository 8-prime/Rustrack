use rustrack_shared::vda5050::state::{
    ActiveEmergencyStop, ControlPoint, EdgeState, Error, ErrorLevel, MobileRobotPosition,
    NodePosition, NodeState, OperatingMode, PowerSupply, SafetyState, State, Trajectory, Velocity,
};

use crate::{
    map::{Edge, NodeMap},
    scenario::Scenario,
};

/// Map identifier reported in the VDA5050 state. The simulator uses a single map.
const MAP_ID: &str = "sim-map";

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
    order_id: String,
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
            order_id: "sim-order-001".to_string(),
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
                    self.motion = Motion::Waiting {
                        node_id: arrived_at,
                        remaining_s: 0.5,
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

        let (distance_since_last_node, edge_states, velocity) = match &self.motion {
            Motion::Moving {
                edge_id,
                progress_m,
            } => {
                let edge = &map.edges[edge_id.as_str()];
                let vx = self.speed_m_s.min(edge.max_speed) * theta.cos();
                let vy = self.speed_m_s.min(edge.max_speed) * theta.sin();
                let es = self.build_edge_state(edge_id, edge);
                (
                    Some(*progress_m),
                    vec![es],
                    Some(Velocity {
                        vx: Some(vx),
                        vy: Some(vy),
                        omega: None,
                    }),
                )
            }
            _ => (None, vec![], None),
        };

        // Report the current node in nodeStates.
        let node_states = vec![NodeState {
            node_id: self.current_node.clone(),
            sequence_id: 0,
            released: true,
            node_position: map.nodes.get(&self.current_node).map(|n| NodePosition {
                x: n.x,
                y: n.y,
                theta: Some(0.0),
                map_id: MAP_ID.to_string(),
            }),
            node_descriptor: None,
        }];

        State {
            header_id: self.header_id as i64,
            timestamp: chrono::Utc::now().to_rfc3339(),
            version: "2.1.0".to_string(),
            manufacturer: "Rustrack-Sim".to_string(),
            serial_number: self.serial.clone(),
            order_id: self.order_id.clone(),
            order_update_id: 0,
            last_node_id: self.current_node.clone(),
            last_node_sequence_id: 0,
            driving,
            paused: Some(false),
            new_base_request: Some(false),
            distance_since_last_node,
            mobile_robot_position: Some(MobileRobotPosition {
                x,
                y,
                theta,
                map_id: MAP_ID.to_string(),
                localized: true,
                localization_score: Some(1.0),
                deviation_range: None,
            }),
            velocity,
            node_states,
            edge_states,
            power_supply: PowerSupply {
                state_of_charge: self.battery_pct,
                charging: false,
                battery_voltage: None,
                battery_health: Some(100.0),
                battery_current: None,
                range: None,
            },
            errors: self.maybe_error(),
            safety_state: SafetyState {
                active_emergency_stop: ActiveEmergencyStop::None,
                field_violation: false,
            },
            operating_mode: OperatingMode::Automatic,
            ..State::default()
        }
    }

    fn build_edge_state(&self, edge_id: &str, edge: &Edge) -> EdgeState {
        let trajectory = edge.curve.as_ref().map(|nurbs| Trajectory {
            degree: Some(nurbs.degree as i64),
            knot_vector: Some(nurbs.knots.clone()),
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
            sequence_id: 1,
            released: true,
            trajectory,
            edge_descriptor: None,
        }
    }

    fn maybe_error(&self) -> Vec<Error> {
        if self.battery_pct < 20.0 {
            vec![Error {
                error_type: "batteryLow".to_string(),
                error_level: ErrorLevel::Warning,
                error_description: Some(format!("Battery at {:.1}%", self.battery_pct)),
                error_description_translations: None,
                error_hint: None,
                error_hint_translations: None,
                error_references: None,
            }]
        } else {
            Vec::new()
        }
    }
}
