//! World <-> screen camera, ported from the TS `PositionCanvas` helpers.
//!
//! The renderer owns the camera now, so the exact pan/zoom feel of the old
//! Canvas2D view lives here: an auto-fit that frames the layout, with a
//! user-controlled zoom/pan layered on top that freezes the fit the moment the
//! user first interacts (so late-arriving robot positions can't slide the view).

// Mirrors of the constants in `web/src/lib/theme.ts`, kept in sync by hand.
pub const AGV_FOOTPRINT_M: f32 = 1.2;
pub const AGV_MIN_PX: f32 = 14.0;
pub const AGV_MAX_PX: f32 = 96.0;
pub const PADDING_FRAC: f32 = 0.1;
pub const MIN_ZOOM: f32 = 0.2;
pub const MAX_ZOOM: f32 = 40.0;

/// Axis-aligned world extent, in metres.
#[derive(Clone, Copy)]
pub struct Bounds {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
}

impl Bounds {
    fn grow_point(&mut self, x: f32, y: f32) {
        self.min_x = self.min_x.min(x);
        self.max_x = self.max_x.max(x);
        self.min_y = self.min_y.min(y);
        self.max_y = self.max_y.max(y);
    }
}

/// The world -> screen transform for one frame. `scale` is the composite
/// px-per-metre (auto-fit scale already multiplied by `zoom`); the overlay reads
/// these same numbers back so its grid and labels line up with the GPU layer.
#[derive(Clone, Copy)]
pub struct Transform {
    pub scale: f32,
    pub zoom: f32,
    pub cx: f32,
    pub cy: f32,
    pub w: f32,
    pub h: f32,
    pub pan_x: f32,
    pub pan_y: f32,
}

impl Transform {
    /// World -> screen (CSS px), flipping Y so world y-up maps to canvas y-down.
    /// The overlay projects in JS; this mirror is kept for parity with `unproject`.
    #[allow(dead_code)]
    pub fn project(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.w / 2.0 + (x - self.cx) * self.scale + self.pan_x,
            self.h / 2.0 - (y - self.cy) * self.scale + self.pan_y,
        )
    }

    /// Screen (CSS px) -> world, the exact inverse of `project`.
    pub fn unproject(&self, sx: f32, sy: f32) -> (f32, f32) {
        (
            self.cx + (sx - self.w / 2.0 - self.pan_x) / self.scale,
            self.cy - (sy - self.h / 2.0 - self.pan_y) / self.scale,
        )
    }

    /// A column-major world(metres) -> clip 4x4 matrix, derived from `project`
    /// so the GPU and the JS overlay agree pixel-for-pixel. `project` gives
    /// screen px; clip is `2*sx/w - 1`, `1 - 2*sy/h`, which is affine in x/y.
    pub fn view_proj(&self) -> [f32; 16] {
        let sx = 2.0 * self.scale / self.w;
        let sy = 2.0 * self.scale / self.h;
        // tx/ty fold in the centre and the pan.
        let tx = -sx * self.cx + 2.0 * self.pan_x / self.w;
        let ty = -sy * self.cy - 2.0 * self.pan_y / self.h;
        [
            sx, 0.0, 0.0, 0.0, // col 0
            0.0, sy, 0.0, 0.0, // col 1
            0.0, 0.0, 1.0, 0.0, // col 2
            tx, ty, 0.0, 1.0, // col 3
        ]
    }
}

/// The user's adjustments to the auto-fit. `frozen` holds the bounds captured at
/// first interaction; while it is `Some`, growing live bounds no longer rescale.
#[derive(Clone)]
pub struct Camera {
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    frozen: Option<Bounds>,
    /// Live bounds, grown as robots and the map are seen. `None` until anything
    /// with a position has been observed.
    bounds: Option<Bounds>,
    /// Viewport in CSS px, kept up to date by `resize`.
    w: f32,
    h: f32,
}

impl Camera {
    pub fn new() -> Self {
        Self {
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            frozen: None,
            bounds: None,
            w: 1.0,
            h: 1.0,
        }
    }

    pub fn set_viewport(&mut self, w: f32, h: f32) {
        self.w = w.max(1.0);
        self.h = h.max(1.0);
    }

    pub fn adjusted(&self) -> bool {
        self.frozen.is_some()
    }

    /// Seed/grow the live bounds from the map's precomputed extent.
    pub fn seed_bounds(&mut self, b: Bounds) {
        self.bounds = Some(match self.bounds {
            None => b,
            Some(mut cur) => {
                cur.grow_point(b.min_x, b.min_y);
                cur.grow_point(b.max_x, b.max_y);
                cur
            }
        });
    }

    /// Grow the live bounds to include a robot position.
    pub fn grow(&mut self, x: f32, y: f32) {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        match &mut self.bounds {
            None => {
                self.bounds = Some(Bounds {
                    min_x: x,
                    max_x: x,
                    min_y: y,
                    max_y: y,
                })
            }
            Some(b) => b.grow_point(x, y),
        }
    }

    /// Detach the view from the growing live bounds, keeping what's on screen.
    fn take_over(&mut self) {
        if self.frozen.is_none() {
            self.frozen = self.bounds;
        }
    }

    pub fn reset(&mut self) {
        self.zoom = 1.0;
        self.pan_x = 0.0;
        self.pan_y = 0.0;
        self.frozen = None;
    }

    /// The transform for this frame, or `None` if nothing has a position yet.
    pub fn transform(&self) -> Option<Transform> {
        let b = self.frozen.or(self.bounds)?;
        let span_x = (b.max_x - b.min_x).max(1e-3);
        let span_y = (b.max_y - b.min_y).max(1e-3);
        let avail_w = self.w * (1.0 - 2.0 * PADDING_FRAC);
        let avail_h = self.h * (1.0 - 2.0 * PADDING_FRAC);
        let fit = (avail_w / span_x).min(avail_h / span_y);
        Some(Transform {
            scale: fit * self.zoom,
            zoom: self.zoom,
            cx: (b.min_x + b.max_x) / 2.0,
            cy: (b.min_y + b.max_y) / 2.0,
            w: self.w,
            h: self.h,
            pan_x: self.pan_x,
            pan_y: self.pan_y,
        })
    }

    /// Wheel zoom about the cursor: pin the world point under (px, py) in place.
    pub fn wheel(&mut self, px: f32, py: f32, delta_y: f32) {
        let Some(t) = self.transform() else { return };
        self.take_over();
        let (wx, wy) = t.unproject(px, py);
        let next = (self.zoom * (-delta_y * 0.0015).exp()).clamp(MIN_ZOOM, MAX_ZOOM);
        // scale is proportional to zoom, so the new scale follows from the ratio.
        let scale = (t.scale / t.zoom) * next;
        self.zoom = next;
        self.pan_x = px - t.w / 2.0 - (wx - t.cx) * scale;
        self.pan_y = py - t.h / 2.0 + (wy - t.cy) * scale;
    }

    pub fn pointer_down(&mut self) {
        self.take_over();
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.take_over();
        self.pan_x += dx;
        self.pan_y += dy;
    }
}

/// The AGV sprite's on-screen size in px. The clamps act on the fit scale alone;
/// user zoom multiplies afterwards so the sprite keeps growing when zoomed in.
pub fn agv_size(t: &Transform) -> f32 {
    let fitted = AGV_FOOTPRINT_M * (t.scale / t.zoom);
    fitted.clamp(AGV_MIN_PX, AGV_MAX_PX) * t.zoom
}
