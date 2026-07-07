use std::collections::HashMap;

use chrono::{DateTime, Utc};
use shared::vda5050::{
    state::{self, State},
    visualization::{self, Visualization},
};
use tokio::sync::RwLock;

use crate::interpolation;

/// Pose of the robot in world coordinates.
#[derive(Clone)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub theta: f32,
}

/// Robot velocity in *vehicle* coordinates (VDA5050 convention): `vx` is
/// forward, `vy` lateral, `omega` the yaw rate. Missing sub-fields default to 0.
#[derive(Clone, Copy)]
pub struct Velocity {
    pub vx: f64,
    pub vy: f64,
    pub omega: f64,
}

impl Velocity {
    /// Ground speed (magnitude of the translational velocity).
    pub fn speed(&self) -> f64 {
        self.vx.hypot(self.vy)
    }
}

/// Planned path of the robot as a NURBS curve.
#[derive(Clone)]
pub struct Trajectory {
    pub control_points: Vec<ControlPoint>,
    pub degree: i64,
    pub knot_vector: Vec<f64>,
}

#[derive(Clone)]
pub struct ControlPoint {
    pub x: f64,
    pub y: f64,
    pub weight: Option<f64>,
}

#[derive(Clone)]
pub struct RobotState {
    /// Last reported pose. `None` while the robot has not reported a position yet.
    pub position: Option<Position>,
    /// Last reported velocity, used to extrapolate the pose between updates.
    pub velocity: Option<Velocity>,
    /// Last reported planned path, if the robot is currently executing an order.
    pub trajectory: Option<Trajectory>,
    /// Distance driven along the current edge since `lastNodeId`, in meters. This
    /// is the robot's arc-length position along `trajectory`.
    pub distance_since_last_node: Option<f64>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Clone)]
pub struct InterpolatedState {
    pub x: f32,
    pub y: f32,
    pub theta: f32,
    pub timestamp: DateTime<Utc>,
}

#[derive(Clone)]
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

    pub async fn get_snapshot(&self) -> HashMap<String, MobileRobotState> {
        let states = self.states.read().await;
        tracing::trace!("snapshot taken for {} robot(s)", states.len());
        states.clone()
    }

    /// Applies a full VDA5050 `state` message. The state topic is the authoritative
    /// snapshot, so the whole `vda_state` is replaced.
    pub async fn update_state(&self, id: String, state: State) -> anyhow::Result<()> {
        let timestamp = DateTime::parse_from_rfc3339(&state.timestamp)?.with_timezone(&Utc);

        let robot_state = RobotState {
            position: state.agv_position.map(Position::from),
            velocity: state.velocity.map(Velocity::from),
            distance_since_last_node: state.distance_since_last_node,
            // v2 has no planned path; the trajectory lives per-edge. Use the first released
            // edge that carries one.
            trajectory: state
                .edge_states
                .into_iter()
                .filter(|e| e.released)
                .find_map(|e| e.trajectory)
                .map(Trajectory::from),
            timestamp,
        };

        let has_position = robot_state.position.is_some();
        let has_trajectory = robot_state.trajectory.is_some();

        let mut states = self.states.write().await;
        if let Some(existing) = states.get_mut(&id) {
            existing.vda_state = robot_state;
        } else {
            states.insert(
                id.clone(),
                MobileRobotState {
                    vda_state: robot_state,
                    interpolated_state: None,
                },
            );
        }

        tracing::debug!(
            "state update for '{id}' (has_position={has_position}, has_trajectory={has_trajectory}), tracking {} robot(s)",
            states.len()
        );

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
        // v2 visualization timestamps are optional; fall back to now when absent.
        let timestamp = visualization
            .timestamp
            .as_deref()
            .map(DateTime::parse_from_rfc3339)
            .transpose()?
            .map(|t| t.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);
        let position = visualization.agv_position.map(Position::from);
        let velocity = visualization.velocity.map(Velocity::from);

        let has_position = position.is_some();

        let mut states = self.states.write().await;
        if let Some(existing) = states.get_mut(&id) {
            // v2 visualization carries neither trajectory nor arc-length; only
            // refresh the pose/velocity. Without a fresh distanceSinceLastNode to
            // pair with this position, clear it so interpolation dead-reckons from
            // the fresh pose instead of a now-stale point on the path.
            if position.is_some() {
                existing.vda_state.position = position;
                existing.vda_state.velocity = velocity;
                existing.vda_state.distance_since_last_node = None;
                existing.vda_state.timestamp = timestamp;
            }
        } else {
            // First message we ever received for this robot was a visualization.
            states.insert(
                id.clone(),
                MobileRobotState {
                    vda_state: RobotState {
                        position,
                        velocity,
                        distance_since_last_node: None,
                        trajectory: None,
                        timestamp,
                    },
                    interpolated_state: None,
                },
            );
        }

        tracing::debug!(
            "visualization update for '{id}' (has_position={has_position}), tracking {} robot(s)",
            states.len()
        );

        Ok(())
    }

    pub async fn update_interpolation(&self) {
        let mut states = self.states.write().await;
        let mut interpolated = 0usize;
        states.values_mut().for_each(|s| {
            s.interpolated_state = interpolation::engine::interpolate(&s);
            if s.interpolated_state.is_some() {
                interpolated += 1;
            }
        });
        tracing::trace!(
            "interpolation updated: {interpolated}/{} robot(s) have a pose",
            states.len()
        );
    }
}

impl From<state::AgvPosition> for Position {
    fn from(p: state::AgvPosition) -> Self {
        Self {
            x: p.x as f32,
            y: p.y as f32,
            theta: p.theta as f32,
        }
    }
}

impl From<visualization::AgvPosition> for Position {
    fn from(p: visualization::AgvPosition) -> Self {
        Self {
            x: p.x as f32,
            y: p.y as f32,
            theta: p.theta as f32,
        }
    }
}

impl From<state::Velocity> for Velocity {
    fn from(v: state::Velocity) -> Self {
        Self {
            vx: v.vx.unwrap_or(0.0),
            vy: v.vy.unwrap_or(0.0),
            omega: v.omega.unwrap_or(0.0),
        }
    }
}

impl From<visualization::Velocity> for Velocity {
    fn from(v: visualization::Velocity) -> Self {
        Self {
            vx: v.vx.unwrap_or(0.0),
            vy: v.vy.unwrap_or(0.0),
            omega: v.omega.unwrap_or(0.0),
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
