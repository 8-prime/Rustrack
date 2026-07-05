//! Minimal 2D NURBS curve evaluation used to drive AGVs along curved edges.
//!
//! The shared VDA5050 library only defines the *data* representation of a
//! trajectory (control points, degree, knot vector). The simulator needs the
//! actual geometry — evaluating points along the curve and mapping arc length
//! to the curve parameter — so that math lives here.

/// A single weighted control point of a NURBS curve.
#[derive(Debug, Clone)]
pub struct ControlPoint {
    pub x: f64,
    pub y: f64,
    pub weight: f64,
}

/// A rational B-spline (NURBS) curve in the plane.
#[derive(Debug, Clone)]
pub struct NurbsCurve {
    pub degree: usize,
    pub knots: Vec<f64>,
    pub control_points: Vec<ControlPoint>,
}

/// Build a clamped (open-uniform) knot vector for `n` control points of the
/// given `degree`, normalized to the parameter domain `[0, 1]`.
pub fn open_uniform_knots(n: usize, degree: usize) -> Vec<f64> {
    let m = n + degree + 1;
    let mut knots = vec![0.0; m];
    // Number of evenly spaced interior knots.
    let interior = n.saturating_sub(degree + 1);
    for (i, knot) in knots.iter_mut().enumerate() {
        *knot = if i <= degree {
            0.0
        } else if i >= n {
            1.0
        } else {
            (i - degree) as f64 / (interior + 1) as f64
        };
    }
    knots
}

impl NurbsCurve {
    /// Domain of the curve parameter `[u_start, u_end]`.
    fn domain(&self) -> (f64, f64) {
        let last = self.control_points.len().saturating_sub(1);
        (self.knots[self.degree], self.knots[last + 1])
    }

    /// Evaluate the curve at parameter `u` (clamped to the valid domain),
    /// returning the `(x, y)` point.
    pub fn evaluate(&self, u: f64) -> (f64, f64) {
        let p = self.degree;
        let last = self.control_points.len() - 1;
        let (u_start, u_end) = self.domain();
        let u = u.clamp(u_start, u_end);

        let span = find_span(last, p, u, &self.knots);
        let basis = basis_funs(span, u, p, &self.knots);

        let (mut x, mut y, mut w) = (0.0, 0.0, 0.0);
        for i in 0..=p {
            let cp = &self.control_points[span - p + i];
            let wn = basis[i] * cp.weight;
            x += wn * cp.x;
            y += wn * cp.y;
            w += wn;
        }
        if w != 0.0 {
            (x / w, y / w)
        } else {
            (x, y)
        }
    }

    /// Sample the curve into a table of `(cumulative_arc_length, parameter)`
    /// pairs using `samples` linear segments. The final entry's arc length is
    /// the total curve length.
    pub fn arc_length_table(&self, samples: usize) -> Vec<(f64, f64)> {
        let (u_start, u_end) = self.domain();
        let mut table = Vec::with_capacity(samples + 1);

        let mut prev = self.evaluate(u_start);
        let mut acc = 0.0;
        table.push((0.0, u_start));
        for k in 1..=samples {
            let u = u_start + (u_end - u_start) * (k as f64 / samples as f64);
            let pt = self.evaluate(u);
            acc += ((pt.0 - prev.0).powi(2) + (pt.1 - prev.1).powi(2)).sqrt();
            table.push((acc, u));
            prev = pt;
        }
        table
    }

    /// Given an arc-length table, find the curve parameter `u` corresponding to
    /// arc length `s` via linear interpolation between samples.
    pub fn t_for_arc_length(table: &[(f64, f64)], s: f64) -> f64 {
        match (table.first(), table.last()) {
            (Some(&(_, u0)), Some(&(total, u_end))) => {
                let s = s.clamp(0.0, total);
                for w in table.windows(2) {
                    let (s0, t0) = w[0];
                    let (s1, t1) = w[1];
                    if s <= s1 {
                        let seg = s1 - s0;
                        let frac = if seg > 0.0 { (s - s0) / seg } else { 0.0 };
                        return t0 + frac * (t1 - t0);
                    }
                }
                let _ = u0;
                u_end
            }
            _ => 0.0,
        }
    }
}

/// Locate the knot span index containing parameter `u`.
/// `last` is the index of the final control point (`control_points.len() - 1`).
fn find_span(last: usize, degree: usize, u: f64, knots: &[f64]) -> usize {
    if u >= knots[last + 1] {
        return last;
    }
    if u <= knots[degree] {
        return degree;
    }
    let (mut low, mut high) = (degree, last + 1);
    let mut mid = (low + high) / 2;
    while u < knots[mid] || u >= knots[mid + 1] {
        if u < knots[mid] {
            high = mid;
        } else {
            low = mid;
        }
        mid = (low + high) / 2;
    }
    mid
}

/// Compute the `degree + 1` non-zero B-spline basis functions at `u` within the
/// given knot `span` (Cox–de Boor, per The NURBS Book, algorithm A2.2).
fn basis_funs(span: usize, u: f64, degree: usize, knots: &[f64]) -> Vec<f64> {
    let mut n = vec![0.0; degree + 1];
    let mut left = vec![0.0; degree + 1];
    let mut right = vec![0.0; degree + 1];
    n[0] = 1.0;
    for j in 1..=degree {
        left[j] = u - knots[span + 1 - j];
        right[j] = knots[span + j] - u;
        let mut saved = 0.0;
        for r in 0..j {
            let temp = n[r] / (right[r + 1] + left[j - r]);
            n[r] = saved + right[r + 1] * temp;
            saved = left[j - r] * temp;
        }
        n[j] = saved;
    }
    n
}
