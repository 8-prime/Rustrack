//! The selected robot's route, cut to what it still has to drive.
//!
//! The backend sends the polyline only when it changes, but the robot moves
//! every frame. The part behind it is trimmed here so the line always starts at
//! the vehicle. No GPU types, so the geometry can be tested on its own:
//! `rustc --edition 2024 --test renderer/src/route.rs -o /tmp/route && /tmp/route`.

// Sky-400 (#38bdf8), distinct from the amber used for stations and following.
const BASE_COLOR: [f32; 4] = [0.220, 0.741, 0.973, 1.0];
// The horizon may still change, so it reads as tentative.
const HORIZON_COLOR: [f32; 4] = [0.220, 0.741, 0.973, 0.55];

/// One line segment as uploaded to the GPU: `[ax, ay, bx, by, r, g, b, a]`.
pub type Segment = [f32; 8];

pub struct Route {
    pub serial: String,
    pub points: Vec<[f32; 2]>,
    /// Leading points the robot is somewhere along, up to the next node.
    pub current_points: usize,
    /// Leading points that are released; the rest are the horizon.
    pub released_points: usize,
}

impl Route {
    /// Build `[x, y, x, y, ...]` into a route. A trailing odd value is dropped.
    pub fn new(serial: String, flat: &[f32], current_points: u32, released_points: u32) -> Self {
        let points: Vec<[f32; 2]> = flat.chunks_exact(2).map(|p| [p[0], p[1]]).collect();
        Self {
            serial,
            current_points: (current_points as usize).min(points.len()),
            released_points: (released_points as usize).min(points.len()),
            points,
        }
    }

    /// The segments to draw from `robot` onward, or the whole route if the robot
    /// has no pose to start from.
    pub fn segments(&self, robot: Option<[f32; 2]>) -> Vec<Segment> {
        let Some(&first) = self.points.first() else {
            return Vec::new();
        };
        let (resume, start, mut from) = match robot {
            Some(robot) => {
                let (index, on_route) = nearest(&self.points[..self.current_points.max(1)], robot);
                (index, Some(on_route), robot)
            }
            None => (0, None, first),
        };

        let mut segments = Vec::with_capacity(self.points.len() + 1);
        let mut push = |from: &mut [f32; 2], to: [f32; 2], released: bool| {
            let [r, g, b, a] = if released { BASE_COLOR } else { HORIZON_COLOR };
            segments.push([from[0], from[1], to[0], to[1], r, g, b, a]);
            *from = to;
        };

        if let Some(on_route) = start {
            push(&mut from, on_route, resume < self.released_points);
        }
        for (i, &point) in self.points.iter().enumerate().skip(resume + 1) {
            push(&mut from, point, i < self.released_points);
        }
        segments
    }
}

/// The point on a polyline nearest to `target`, with the index of the segment
/// it lies on. A lone point is its own nearest.
fn nearest(points: &[[f32; 2]], target: [f32; 2]) -> (usize, [f32; 2]) {
    let mut best = (0, points[0]);
    let mut best_distance = f32::INFINITY;

    for (i, w) in points.windows(2).enumerate() {
        let [ax, ay] = w[0];
        let (dx, dy) = (w[1][0] - ax, w[1][1] - ay);
        let length2 = dx * dx + dy * dy;
        let t = if length2 > 0.0 {
            (((target[0] - ax) * dx + (target[1] - ay) * dy) / length2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let point = [ax + dx * t, ay + dy * t];
        let distance = (point[0] - target[0]).powi(2) + (point[1] - target[1]).powi(2);
        if distance < best_distance {
            best_distance = distance;
            best = (i, point);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(segment: &Segment) -> [f32; 4] {
        [segment[0], segment[1], segment[2], segment[3]]
    }

    fn route(flat: &[f32], current: u32, released: u32) -> Route {
        Route::new("agv".into(), flat, current, released)
    }

    #[test]
    fn the_driven_part_of_the_current_edge_is_cut_away() {
        let r = route(&[0.0, 0.0, 10.0, 0.0, 10.0, 5.0], 2, 3);
        let s = r.segments(Some([4.0, 0.0]));

        assert_eq!(s.len(), 3);
        assert_eq!(line(&s[0]), [4.0, 0.0, 4.0, 0.0]);
        assert_eq!(line(&s[1]), [4.0, 0.0, 10.0, 0.0]);
        assert_eq!(line(&s[2]), [10.0, 0.0, 10.0, 5.0]);
    }

    #[test]
    fn a_robot_off_the_line_is_joined_to_it() {
        let r = route(&[0.0, 0.0, 10.0, 0.0], 2, 2);
        let s = r.segments(Some([4.0, 2.0]));

        assert_eq!(line(&s[0]), [4.0, 2.0, 4.0, 0.0]);
    }

    #[test]
    fn a_robot_with_only_a_next_node_gets_a_straight_line_to_it() {
        let r = route(&[5.0, 5.0], 1, 1);
        let s = r.segments(Some([1.0, 1.0]));

        assert_eq!(s.len(), 1);
        assert_eq!(line(&s[0]), [1.0, 1.0, 5.0, 5.0]);
    }

    /// On a loop the robot passes near later legs too; only the leg it is on
    /// may be cut, or most of the route would vanish.
    #[test]
    fn later_legs_are_never_mistaken_for_the_current_one() {
        let r = route(&[0.0, 0.0, 10.0, 0.0, 10.0, 1.0, 1.0, 1.0], 2, 4);
        let s = r.segments(Some([1.0, 0.9]));

        assert_eq!(line(&s[1]), [1.0, 0.0, 10.0, 0.0]);
        assert_eq!(s.len(), 4);
    }

    #[test]
    fn the_horizon_is_drawn_dimmer_than_the_base() {
        let r = route(&[0.0, 0.0, 1.0, 0.0, 2.0, 0.0], 1, 2);
        let s = r.segments(None);

        assert_eq!(s.len(), 2);
        assert_eq!(s[0][7], BASE_COLOR[3]);
        assert_eq!(s[1][7], HORIZON_COLOR[3]);
    }

    #[test]
    fn an_empty_route_draws_nothing() {
        assert!(route(&[], 0, 0).segments(Some([0.0, 0.0])).is_empty());
    }

    #[test]
    fn counts_past_the_end_are_clamped() {
        let r = route(&[0.0, 0.0, 1.0, 0.0], 9, 9);
        assert_eq!(r.segments(Some([0.5, 0.0])).len(), 2);
    }
}
