//! Pose interpolation: between the (relatively infrequent) state/visualization
//! updates from a robot, project its last reported pose forward to *now* so the
//! rendered fleet moves smoothly instead of snapping on each message.
//!
//! Two regimes, in order of preference:
//!
//! * **Path-following** — when we know both the planned NURBS trajectory and how
//!   far along it the robot is (`distanceSinceLastNode`), advance the arc-length
//!   by `speed * dt` and evaluate the curve. This keeps the robot *on* the path
//!   through curves instead of drifting off along the tangent.
//! * **Dead reckoning** — otherwise integrate the velocity vector. VDA5050
//!   reports velocity in *vehicle* coordinates, so it is rotated into the world
//!   frame by the reported heading before integrating.

use chrono::{DateTime, Utc};
use shared::nurbs::{ControlPoint as NurbsControlPoint, NurbsCurve};

use crate::runtime::state::{InterpolatedState, MobileRobotState, Position, Trajectory, Velocity};

/// Cap on how far ahead of the last update we will extrapolate. If a robot stops
/// reporting, we freeze it here rather than letting it fly off indefinitely.
const MAX_EXTRAPOLATION_MS: i64 = 500;

/// Number of linear segments used to approximate the curve's arc length.
const ARC_SAMPLES: usize = 50;

pub fn interpolate(robot_state: &MobileRobotState) -> Option<InterpolatedState> {
    let vda = &robot_state.vda_state;
    let position = vda.position.as_ref()?;

    let now = Utc::now();
    let dt = elapsed_seconds(vda.timestamp, now);

    let speed = vda.velocity.map(|v| v.speed()).unwrap_or(0.0);

    // Regime B: follow the reported path when we know where on it the robot is.
    if let (Some(trajectory), Some(s0)) = (vda.trajectory.as_ref(), vda.distance_since_last_node) {
        if let Some(state) = interpolate_along_path(trajectory, s0, speed, dt, now) {
            return Some(state);
        }
    }

    // Regime A: dead reckoning from the reported pose and velocity.
    Some(dead_reckon(position, vda.velocity, dt, now))
}

/// Seconds between `from` and `to`, clamped to `[0, MAX_EXTRAPOLATION_MS]`.
fn elapsed_seconds(from: DateTime<Utc>, to: DateTime<Utc>) -> f64 {
    (to - from).num_milliseconds().clamp(0, MAX_EXTRAPOLATION_MS) as f64 / 1000.0
}

/// Extrapolate along the NURBS trajectory by `speed * dt` from arc-length `s0`.
/// Returns `None` for a malformed trajectory so the caller can fall back to dead
/// reckoning.
fn interpolate_along_path(
    trajectory: &Trajectory,
    s0: f64,
    speed: f64,
    dt: f64,
    now: DateTime<Utc>,
) -> Option<InterpolatedState> {
    let curve = NurbsCurve {
        degree: trajectory.degree.max(0) as usize,
        knots: trajectory.knot_vector.clone(),
        control_points: trajectory
            .control_points
            .iter()
            .map(|cp| NurbsControlPoint {
                x: cp.x,
                y: cp.y,
                weight: cp.weight.unwrap_or(1.0),
            })
            .collect(),
    };
    if !curve.is_valid() {
        return None;
    }

    let table = curve.arc_length_table(ARC_SAMPLES);
    let total = table.last().map(|&(s, _)| s)?;
    // A zero-length path carries no direction; let dead reckoning handle it.
    if total <= 0.0 {
        return None;
    }

    let s = (s0 + speed * dt).clamp(0.0, total);
    let (x, y) = curve.evaluate(NurbsCurve::t_for_arc_length(&table, s));
    let theta = tangent_heading(&curve, &table, s, total);

    Some(InterpolatedState {
        x: x as f32,
        y: y as f32,
        theta: theta as f32,
        timestamp: now,
    })
}

/// Heading of the curve tangent at arc-length `s`, via a central finite
/// difference (clamped to the curve's ends).
fn tangent_heading(curve: &NurbsCurve, table: &[(f64, f64)], s: f64, total: f64) -> f64 {
    let ds = (total * 0.01).max(1e-3);
    let sa = (s - ds).max(0.0);
    let sb = (s + ds).min(total);
    let (xa, ya) = curve.evaluate(NurbsCurve::t_for_arc_length(table, sa));
    let (xb, yb) = curve.evaluate(NurbsCurve::t_for_arc_length(table, sb));
    (yb - ya).atan2(xb - xa)
}

/// Integrate the velocity vector for `dt` seconds. VDA5050 velocity is in
/// vehicle coordinates (`vx` forward, `vy` lateral), so rotate it into the world
/// frame by the reported heading before adding it to the position.
fn dead_reckon(
    position: &Position,
    velocity: Option<Velocity>,
    dt: f64,
    now: DateTime<Utc>,
) -> InterpolatedState {
    let (dx, dy, dtheta) = match velocity {
        Some(v) => {
            let theta = position.theta as f64;
            let world_vx = v.vx * theta.cos() - v.vy * theta.sin();
            let world_vy = v.vx * theta.sin() + v.vy * theta.cos();
            (world_vx * dt, world_vy * dt, v.omega * dt)
        }
        None => (0.0, 0.0, 0.0),
    };

    InterpolatedState {
        x: position.x + dx as f32,
        y: position.y + dy as f32,
        theta: position.theta + dtheta as f32,
        timestamp: now,
    }
}
