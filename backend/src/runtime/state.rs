use std::collections::HashMap;

use chrono::{DateTime, Utc};
use shared::vda5050::{
    state::{self, State},
    visualization::{self, Visualization},
};
use tokio::sync::RwLock;

/// Pose of the robot in world coordinates.
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub theta: f32,
}

/// Planned path of the robot as a NURBS curve.
pub struct Trajectory {
    pub control_points: Vec<ControlPoint>,
    pub degree: Option<i64>,
    pub knot_vector: Option<Vec<f64>>,
}

pub struct ControlPoint {
    pub x: f64,
    pub y: f64,
    pub weight: Option<f64>,
}

pub struct RobotState {
    /// Last reported pose. `None` while the robot has not reported a position yet.
    pub position: Option<Position>,
    /// Last reported planned path, if the robot is currently executing an order.
    pub trajectory: Option<Trajectory>,
    pub timestamp: DateTime<Utc>,
}

pub struct InterpolatedState {
    pub x: f32,
    pub y: f32,
    pub theta: f32,
    pub timestamp: DateTime<Utc>,
}

pub struct MobileRobotState {
    pub vda_state: RobotState,
    pub interpolated_state: Option<InterpolatedState>,
}

pub struct StateManager {
    states: RwLock<HashMap<String, MobileRobotState>>,
}

impl StateManager {
    pub fn new() -> Self {
        Self {
            states: RwLock::new(HashMap::new()),
        }
    }

    /// Applies a full VDA5050 `state` message. The state topic is the authoritative
    /// snapshot, so the whole `vda_state` is replaced.
    pub async fn update_state(&self, id: String, state: State) -> anyhow::Result<()> {
        let timestamp = DateTime::parse_from_rfc3339(&state.timestamp)?.with_timezone(&Utc);

        let robot_state = RobotState {
            position: state.mobile_robot_position.map(Position::from),
            trajectory: state
                .planned_path
                .map(|path| Trajectory::from(path.trajectory)),
            timestamp,
        };

        let mut states = self.states.write().await;
        if let Some(existing) = states.get_mut(&id) {
            existing.vda_state = robot_state;
        } else {
            states.insert(
                id,
                MobileRobotState {
                    vda_state: robot_state,
                    interpolated_state: None,
                },
            );
        }

        Ok(())
    }

    /// Applies a `visualization` message. These are partial, high-rate updates, so we
    /// only merge in the fields that are actually present and keep the rest of the
    /// last known state (e.g. the trajectory from the state topic) intact.
    pub async fn update_visualization(
        &self,
        id: String,
        visualization: Visualization,
    ) -> anyhow::Result<()> {
        let timestamp = DateTime::parse_from_rfc3339(&visualization.timestamp)?.with_timezone(&Utc);
        let position = visualization.mobile_robot_position.map(Position::from);
        let trajectory = visualization
            .planned_path
            .map(|path| Trajectory::from(path.trajectory));

        let mut states = self.states.write().await;
        if let Some(existing) = states.get_mut(&id) {
            if position.is_some() {
                existing.vda_state.position = position;
                existing.vda_state.timestamp = timestamp;
            }
            if trajectory.is_some() {
                existing.vda_state.trajectory = trajectory;
            }
        } else {
            // First message we ever received for this robot was a visualization.
            states.insert(
                id,
                MobileRobotState {
                    vda_state: RobotState {
                        position,
                        trajectory,
                        timestamp,
                    },
                    interpolated_state: None,
                },
            );
        }

        Ok(())
    }
}

impl From<state::MobileRobotPosition> for Position {
    fn from(p: state::MobileRobotPosition) -> Self {
        Self {
            x: p.x as f32,
            y: p.y as f32,
            theta: p.theta as f32,
        }
    }
}

impl From<visualization::MobileRobotPosition> for Position {
    fn from(p: visualization::MobileRobotPosition) -> Self {
        Self {
            x: p.x as f32,
            y: p.y as f32,
            theta: p.theta as f32,
        }
    }
}

impl From<state::ControlPoint> for ControlPoint {
    fn from(c: state::ControlPoint) -> Self {
        Self {
            x: c.x,
            y: c.y,
            weight: c.weight,
        }
    }
}

impl From<visualization::ControlPoint> for ControlPoint {
    fn from(c: visualization::ControlPoint) -> Self {
        Self {
            x: c.x,
            y: c.y,
            weight: c.weight,
        }
    }
}

impl From<state::Trajectory> for Trajectory {
    fn from(t: state::Trajectory) -> Self {
        Self {
            control_points: t
                .control_points
                .into_iter()
                .map(ControlPoint::from)
                .collect(),
            degree: t.degree,
            knot_vector: t.knot_vector,
        }
    }
}

impl From<visualization::Trajectory> for Trajectory {
    fn from(t: visualization::Trajectory) -> Self {
        Self {
            control_points: t
                .control_points
                .into_iter()
                .map(ControlPoint::from)
                .collect(),
            degree: t.degree,
            knot_vector: t.knot_vector,
        }
    }
}
