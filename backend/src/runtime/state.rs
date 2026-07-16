use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use chrono::{DateTime, Utc};
use shared::vda5050::{
    state::{self, State},
    visualization::{self, Visualization},
};

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

/// A single reported pose, tagged with when the backend received it. Interpolation
/// paces itself by `received_at` (a monotonic backend clock) rather than the
/// robot's own `timestamp`, so clock skew between robot and backend can't distort
/// playback.
#[derive(Clone)]
pub struct PoseSample {
    pub position: Position,
    /// Arc-length along the current edge (`distanceSinceLastNode`), if reported.
    pub distance_since_last_node: Option<f64>,
    pub received_at: Instant,
}

#[derive(Clone)]
pub struct RobotState {
    /// Last reported pose. `None` while the robot has not reported a position yet.
    pub position: Option<Position>,
    /// The pose sample immediately before the current one. Interpolation runs
    /// *between* `previous` and the current pose (render-delayed interpolation)
    /// instead of extrapolating past the newest sample, which is what caused the
    /// "run ahead, halt, then jolt" motion.
    pub previous: Option<PoseSample>,
    /// Backend monotonic clock when the current pose was ingested.
    pub received_at: Instant,
    /// Last reported velocity. Used only to dead-reckon before a second sample
    /// exists to interpolate against.
    pub velocity: Option<Velocity>,
    /// Last reported planned path, if the robot is currently executing an order.
    pub trajectory: Option<Arc<Trajectory>>,
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

impl RobotState {
    /// Install a freshly reported pose, shifting the outgoing pose into
    /// `previous` so interpolation always has two real samples to work between.
    /// Only shifts when both the old and new poses exist.
    fn record_pose(
        &mut self,
        position: Option<Position>,
        distance: Option<f64>,
        timestamp: DateTime<Utc>,
    ) {
        match position {
            Some(new_pos) => {
                if let Some(old_pos) = self.position.take() {
                    self.previous = Some(PoseSample {
                        position: old_pos,
                        distance_since_last_node: self.distance_since_last_node,
                        received_at: self.received_at,
                    });
                }
                self.position = Some(new_pos);
                self.distance_since_last_node = distance;
                self.received_at = Instant::now();
                self.timestamp = timestamp;
            }
            // No fresh position: keep the last known pose but drop the stale
            // history so we never interpolate across an unknown gap.
            None => self.previous = None,
        }
    }
}

pub struct StateManager {
    states: RwLock<HashMap<String, Arc<MobileRobotState>>>,
}

impl StateManager {
    pub fn new() -> Self {
        Self {
            states: RwLock::new(HashMap::new()),
        }
    }

    /// Snapshot the current states and compute a fresh interpolated pose for each
    /// robot. The lock is held only long enough to walk the map and clone out
    /// each robot's `Arc` pointer (cheap refcount bumps, no `RobotState` data
    /// touched); the interpolation math and the per-robot `RobotState` clone
    /// happen after the lock is released, so they never block MQTT writers.
    /// Returns an error instead of panicking if the lock is poisoned.
    pub fn snapshot_interpolated(&self) -> anyhow::Result<HashMap<String, MobileRobotState>> {
        let states: Vec<(String, Arc<MobileRobotState>)> = {
            let guard = self
                .states
                .read()
                .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
            guard
                .iter()
                .map(|(id, state)| (id.clone(), Arc::clone(state)))
                .collect()
        };
        tracing::trace!("snapshot taken for {} robot(s)", states.len());

        let mut interpolated = 0usize;
        let result: HashMap<String, MobileRobotState> = states
            .into_iter()
            .map(|(id, state)| {
                let interpolated_state = interpolation::engine::interpolate(&state);
                if interpolated_state.is_some() {
                    interpolated += 1;
                }
                (
                    id,
                    MobileRobotState {
                        vda_state: state.vda_state.clone(),
                        interpolated_state,
                    },
                )
            })
            .collect();

        tracing::trace!(
            "interpolation updated: {interpolated}/{} robot(s) have a pose",
            result.len()
        );

        Ok(result)
    }

    /// Applies a full VDA5050 `state` message. The state topic is the authoritative
    /// snapshot, so the whole `vda_state` is replaced.
    pub fn update_state(&self, id: String, state: State) -> anyhow::Result<()> {
        let timestamp = DateTime::parse_from_rfc3339(&state.timestamp)?.with_timezone(&Utc);

        let position = state.agv_position.map(Position::from);
        let velocity = state.velocity.map(Velocity::from);
        let distance = state.distance_since_last_node;
        // v2 has no planned path; the trajectory lives per-edge. Use the first released
        // edge that carries one.
        let trajectory = state
            .edge_states
            .into_iter()
            .filter(|e| e.released)
            .find_map(|e| e.trajectory)
            .map(|t| Arc::new(Trajectory::from(t)));

        let has_position = position.is_some();
        let has_trajectory = trajectory.is_some();

        let mut states = self
            .states
            .write()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        if let Some(existing) = states.get_mut(&id) {
            let mutable = Arc::make_mut(existing);
            let vs = &mut mutable.vda_state;
            vs.record_pose(position, distance, timestamp);
            vs.velocity = velocity;
            vs.trajectory = trajectory;
        } else {
            states.insert(
                id.clone(),
                Arc::new(MobileRobotState {
                    vda_state: RobotState {
                        position,
                        previous: None,
                        received_at: Instant::now(),
                        velocity,
                        trajectory,
                        distance_since_last_node: distance,
                        timestamp,
                    },
                    interpolated_state: None,
                }),
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
    pub fn update_visualization(&self, id: String, visualization: Visualization) -> anyhow::Result<()> {
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

        let mut states = self
            .states
            .write()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        if let Some(existing) = states.get_mut(&id) {
            // v2 visualization carries neither trajectory nor arc-length; only
            // refresh the pose/velocity. Pass `None` for the arc-length so
            // interpolation falls back to linear between poses rather than pairing
            // this fresh position with a now-stale point on the path.
            if position.is_some() {
                let mutable = Arc::make_mut(existing);
                let vs = &mut mutable.vda_state;
                vs.record_pose(position, None, timestamp);
                vs.velocity = velocity;
            }
        } else {
            // First message we ever received for this robot was a visualization.
            states.insert(
                id.clone(),
                Arc::new(MobileRobotState {
                    vda_state: RobotState {
                        position,
                        previous: None,
                        received_at: Instant::now(),
                        velocity,
                        trajectory: None,
                        distance_since_last_node: None,
                        timestamp,
                    },
                    interpolated_state: None,
                }),
            );
        }

        tracing::debug!(
            "visualization update for '{id}' (has_position={has_position}), tracking {} robot(s)",
            states.len()
        );

        Ok(())
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
