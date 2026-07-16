//! Pose interpolation for smooth playback.
//!
//! Robots report their pose infrequently and irregularly (the simulator, for
//! instance, at ~1 Hz). Rendering the newest sample directly makes the fleet
//! jump once per update; *extrapolating* past the newest sample makes it race
//! ahead and then halt/jolt when the next sample lands.
//!
//! Instead we do **render-delayed interpolation**: we render slightly in the
//! past and interpolate *between the two most recent real samples*. Motion is
//! then always smooth and paced by the data rather than by a velocity guess. The
//! delay tunes itself to the observed update interval, and pacing uses the
//! backend's monotonic arrival clock (`received_at`) so robot/backend clock skew
//! can't distort it.
//!
//! Interpolation follows the reported NURBS arc when both samples sit on the same
//! edge (so curves aren't chorded across), and falls back to a straight line
//! otherwise. Before a second sample exists we briefly dead-reckon from velocity
//! so a freshly seen robot starts moving right away.

use std::f64::consts::PI;
use std::time::Instant;

use chrono::{DateTime, Utc};
use shared::nurbs::{ControlPoint as NurbsControlPoint, NurbsCurve};

use crate::runtime::state::{
    InterpolatedState, PoseSample, Position, RobotState, Trajectory, Velocity,
};

/// Linear segments used to approximate the curve's arc length.
const ARC_SAMPLES: usize = 50;

/// Bounds on the self-tuned update interval: avoids a divide-by-~zero when two
/// samples arrive together, and stops a long stall from stretching playback to a
/// crawl once updates resume.
const MIN_INTERVAL_S: f64 = 0.001;
const MAX_INTERVAL_S: f64 = 2.0;

/// Cap on how long we dead-reckon from a lone first sample before its successor
/// arrives, so a robot that only ever sends one update doesn't drift forever.
const MAX_DEAD_RECKON_S: f64 = 0.5;

pub fn interpolate(serial: &str, vda: &RobotState) -> Option<InterpolatedState> {
    let cur = vda.position.as_ref()?;
    let now = Instant::now();
    let stamp = Utc::now();

    match &vda.previous {
        // Two real samples: interpolate between them at the delayed render time.
        Some(prev) => Some(interpolate_between(serial, prev, vda, cur, now, stamp)),
        // Only one sample so far: dead-reckon briefly so the robot starts moving
        // immediately rather than sitting still until the second update lands.
        None => {
            let dt = now
                .saturating_duration_since(vda.received_at)
                .as_secs_f64()
                .min(MAX_DEAD_RECKON_S);
            Some(dead_reckon(serial, cur, vda.velocity, dt, stamp))
        }
    }
}

/// Interpolate the pose for render time `now - interval`, i.e. one update behind
/// real time, so the render point stays *between* `previous` and the current
/// sample instead of running past it.
fn interpolate_between(
    serial: &str,
    prev: &PoseSample,
    vda: &RobotState,
    cur: &Position,
    now: Instant,
    stamp: DateTime<Utc>,
) -> InterpolatedState {
    let interval = vda
        .received_at
        .saturating_duration_since(prev.received_at)
        .as_secs_f64()
        .clamp(MIN_INTERVAL_S, MAX_INTERVAL_S);
    // Fraction of the way from `prev` to `cur`. `now == received_at` (a sample
    // just arrived) gives 0 → show `prev`; a full interval later gives 1 → show
    // `cur`. Beyond that it holds at `cur` until the next sample continues it.
    let elapsed = now.saturating_duration_since(vda.received_at).as_secs_f64();
    let f = (elapsed / interval).clamp(0.0, 1.0);

    // Path mode: both samples lie on the same edge (arc-length is present and did
    // not reset), so we can sweep along the NURBS instead of chording the curve.
    if let (Some(traj), Some(d0), Some(d1)) = (
        vda.trajectory.as_ref(),
        prev.distance_since_last_node,
        vda.distance_since_last_node,
    ) {
        if d1 >= d0 {
            if let Some(state) = interpolate_along_path(serial, traj, d0, d1, f, stamp) {
                return state;
            }
        }
    }

    linear(serial, &prev.position, cur, f, stamp)
}

/// Sweep along the NURBS from arc-length `d0` to `d1` by fraction `f`. Returns
/// `None` for a malformed or degenerate trajectory so the caller falls back to a
/// straight line.
fn interpolate_along_path(
    serial: &str,
    trajectory: &Trajectory,
    d0: f64,
    d1: f64,
    f: f64,
    stamp: DateTime<Utc>,
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
    if total <= 0.0 {
        return None;
    }

    let s = (d0 + (d1 - d0) * f).clamp(0.0, total);
    let (x, y) = curve.evaluate(NurbsCurve::t_for_arc_length(&table, s));
    let theta = tangent_heading(&curve, &table, s, total);

    Some(InterpolatedState {
        serial: serial.to_string(),
        x: x as f32,
        y: y as f32,
        theta: theta as f32,
        timestamp: stamp,
    })
}

/// Heading of the curve tangent at arc-length `s`, via a central finite
/// difference clamped to the curve's ends.
fn tangent_heading(curve: &NurbsCurve, table: &[(f64, f64)], s: f64, total: f64) -> f64 {
    let ds = (total * 0.01).max(1e-3);
    let sa = (s - ds).max(0.0);
    let sb = (s + ds).min(total);
    let (xa, ya) = curve.evaluate(NurbsCurve::t_for_arc_length(table, sa));
    let (xb, yb) = curve.evaluate(NurbsCurve::t_for_arc_length(table, sb));
    (yb - ya).atan2(xb - xa)
}

/// Straight-line blend between two poses, taking the shortest arc for heading.
fn linear(
    serial: &str,
    prev: &Position,
    cur: &Position,
    f: f64,
    stamp: DateTime<Utc>,
) -> InterpolatedState {
    let f = f as f32;
    InterpolatedState {
        serial: serial.to_string(),
        x: prev.x + (cur.x - prev.x) * f,
        y: prev.y + (cur.y - prev.y) * f,
        theta: angle_lerp(prev.theta, cur.theta, f),
        timestamp: stamp,
    }
}

/// Integrate the velocity vector for `dt` seconds. VDA5050 velocity is in vehicle
/// coordinates (`vx` forward, `vy` lateral), so it is rotated into the world frame
/// by the reported heading before being added to the position.
fn dead_reckon(
    serial: &str,
    position: &Position,
    velocity: Option<Velocity>,
    dt: f64,
    stamp: DateTime<Utc>,
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
        serial: serial.to_string(),
        x: position.x + dx as f32,
        y: position.y + dy as f32,
        theta: position.theta + dtheta as f32,
        timestamp: stamp,
    }
}

/// Interpolate an angle along its shortest arc, handling the `+pi/-pi` seam.
fn angle_lerp(a: f32, b: f32, f: f32) -> f32 {
    let two_pi = (2.0 * PI) as f32;
    let mut d = (b - a) % two_pi;
    if d > PI as f32 {
        d -= two_pi;
    } else if d < -(PI as f32) {
        d += two_pi;
    }
    a + d * f
}
