use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use chrono::{DateTime, Utc};
use shared::vda5050::{
    state::{self, State},
    visualization::{self, Visualization},
};

use crate::{interpolation, runtime::route::PlannedPath};

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
    /// What is left of the order. Only replaced when it changes, so a viewer can
    /// tell whether it has already seen this one by comparing the pointer.
    pub path: Arc<PlannedPath>,
    /// Distance driven along the current edge since `lastNodeId`, in meters. This
    /// is the robot's arc-length position along `trajectory`.
    pub distance_since_last_node: Option<f64>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Clone)]
pub struct InterpolatedState {
    pub serial: String,
    pub x: f32,
    pub y: f32,
    pub theta: f32,
    pub timestamp: DateTime<Utc>,
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
    states: RwLock<HashMap<String, Arc<RobotState>>>,
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
    /// touched); the interpolation math happens after the lock is released, so
    /// it never blocks MQTT writers. Robots that can't be interpolated (no pose
    /// yet) are dropped from the result rather than published with stale data.
    /// Returns an error instead of panicking if the lock is poisoned.
    pub fn snapshot_interpolated(&self) -> anyhow::Result<Vec<InterpolatedState>> {
        let states: Vec<(String, Arc<RobotState>)> = {
            let guard = self
                .states
                .read()
                .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
            guard
                .iter()
                .map(|(id, state)| (id.clone(), Arc::clone(state)))
                .collect()
        };
        let total = states.len();
        tracing::trace!("snapshot taken for {total} robot(s)");

        let result: Vec<InterpolatedState> = states
            .into_iter()
            .filter_map(|(id, state)| interpolation::engine::interpolate(&id, &state))
            .collect();

        tracing::trace!("interpolated {}/{total} robot(s)", result.len());

        Ok(result)
    }

    /// The robot's remaining path, if it is known at all.
    pub fn path(&self, id: &str) -> anyhow::Result<Option<Arc<PlannedPath>>> {
        let states = self
            .states
            .read()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        Ok(states.get(id).map(|s| Arc::clone(&s.path)))
    }

    /// Applies a full VDA5050 `state` message. The state topic is the authoritative
    /// snapshot, so the whole `vda_state` is replaced.
    pub fn update_state(&self, id: String, state: State) -> anyhow::Result<()> {
        let timestamp = DateTime::parse_from_rfc3339(&state.timestamp)?.with_timezone(&Utc);

        let position = state.agv_position.map(Position::from);
        let velocity = state.velocity.map(Velocity::from);
        let distance = state.distance_since_last_node;
        let path = PlannedPath {
            node_states: state.node_states,
            edge_states: state.edge_states,
        };
        // v2 has no planned path; the trajectory lives per-edge. Only the edge being
        // driven counts: `distanceSinceLastNode` is measured along that one, so a
        // later edge's curve would place the robot somewhere else entirely.
        let trajectory = path
            .edge_states
            .iter()
            .filter(|e| e.released)
            .min_by_key(|e| e.sequence_id)
            .and_then(|e| e.trajectory.as_ref())
            .map(|t| Arc::new(Trajectory::from(t)));

        let has_position = position.is_some();
        let has_trajectory = trajectory.is_some();

        let mut states = self
            .states
            .write()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        if let Some(existing) = states.get_mut(&id) {
            let vs = Arc::make_mut(existing);
            vs.record_pose(position, distance, timestamp);
            vs.velocity = velocity;
            vs.trajectory = trajectory;
            if *vs.path != path {
                vs.path = Arc::new(path);
            }
        } else {
            states.insert(
                id.clone(),
                Arc::new(RobotState {
                    position,
                    previous: None,
                    received_at: Instant::now(),
                    velocity,
                    trajectory,
                    path: Arc::new(path),
                    distance_since_last_node: distance,
                    timestamp,
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
                let vs = Arc::make_mut(existing);
                vs.record_pose(position, None, timestamp);
                vs.velocity = velocity;
            }
        } else {
            // First message we ever received for this robot was a visualization.
            states.insert(
                id.clone(),
                Arc::new(RobotState {
                    position,
                    previous: None,
                    received_at: Instant::now(),
                    velocity,
                    trajectory: None,
                    path: Arc::default(),
                    distance_since_last_node: None,
                    timestamp,
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

impl From<&state::ControlPoint> for ControlPoint {
    fn from(c: &state::ControlPoint) -> Self {
        Self {
            x: c.x,
            y: c.y,
            weight: c.weight,
        }
    }
}

impl From<&state::Trajectory> for Trajectory {
    fn from(t: &state::Trajectory) -> Self {
        Self {
            control_points: t.control_points.iter().map(ControlPoint::from).collect(),
            degree: t.degree,
            knot_vector: t.knot_vector.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn state(edge_states: serde_json::Value) -> State {
        serde_json::from_value(json!({
            "actionStates": [],
            "batteryState": { "batteryCharge": 100.0, "charging": false },
            "driving": true,
            "edgeStates": edge_states,
            "errors": [],
            "headerId": 1,
            "lastNodeId": "",
            "lastNodeSequenceId": 0,
            "manufacturer": "test",
            "nodeStates": [],
            "operatingMode": "AUTOMATIC",
            "orderId": "",
            "orderUpdateId": 0,
            "safetyState": { "eStop": "NONE", "fieldViolation": false },
            "serialNumber": "agv",
            "timestamp": "2026-01-01T00:00:00Z",
            "version": "2.1.0",
        }))
        .unwrap()
    }

    fn edge(sequence_id: i64, curved: bool) -> serde_json::Value {
        let trajectory = curved.then(|| {
            json!({
                "controlPoints": [{ "x": 0.0, "y": 0.0 }, { "x": 1.0, "y": 0.0 }],
                "degree": 1,
                "knotVector": [0.0, 0.0, 1.0, 1.0],
            })
        });
        json!({
            "edgeId": format!("e{sequence_id}"),
            "released": true,
            "sequenceId": sequence_id,
            "trajectory": trajectory,
        })
    }

    fn robot(manager: &StateManager) -> Arc<RobotState> {
        manager.states.read().unwrap()["agv"].clone()
    }

    /// `distanceSinceLastNode` measures the edge being driven, so a curve further
    /// along the path must not be mistaken for it.
    #[test]
    fn only_the_edge_being_driven_sets_the_trajectory() {
        let manager = StateManager::new();

        manager
            .update_state("agv".into(), state(json!([edge(3, true), edge(1, false)])))
            .unwrap();
        assert!(robot(&manager).trajectory.is_none());

        manager
            .update_state("agv".into(), state(json!([edge(1, true), edge(3, false)])))
            .unwrap();
        assert!(robot(&manager).trajectory.is_some());
    }

    #[test]
    fn an_unchanged_path_keeps_its_identity() {
        let manager = StateManager::new();

        manager.update_state("agv".into(), state(json!([edge(1, true)]))).unwrap();
        let first = robot(&manager).path.clone();

        manager.update_state("agv".into(), state(json!([edge(1, true)]))).unwrap();
        assert!(Arc::ptr_eq(&first, &robot(&manager).path));

        manager.update_state("agv".into(), state(json!([edge(3, true)]))).unwrap();
        assert!(!Arc::ptr_eq(&first, &robot(&manager).path));
    }
}
