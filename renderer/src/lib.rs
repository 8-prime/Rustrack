//! wgpu/wasm renderer for the AGV network map.
//!
//! Owns the map geometry (uploaded once, in world metres), the camera, robot
//! pose interpolation, and its own `requestAnimationFrame` loop. React drives it
//! through the [`MapRenderer`] handle: feeding WebSocket frames, pointer/wheel
//! events and resizes in, and reading the camera + robot positions back out to
//! draw the text/grid overlay on a second stacked canvas.

mod camera;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlCanvasElement;
use wgpu::util::DeviceExt;
use wgpu::{DeviceDescriptor, InstanceDescriptor, Surface};

use camera::{Bounds, Camera, Transform};

// Line/node sizes in CSS px, matching `theme.ts` (MAP_NODE_RADIUS) and the old
// 1.5px edge stroke. Multiplied by devicePixelRatio in the shader.
const EDGE_WIDTH_PX: f32 = 1.5;
const NODE_RADIUS_PX: f32 = 2.5;

// CANVAS.bg (#1b1d21).
const CLEAR: wgpu::Color = wgpu::Color {
    r: 0.106,
    g: 0.114,
    b: 0.129,
    a: 1.0,
};

// Bounds on the self-tuned interval, mirroring the TS render loop.
const MIN_INTERVAL_MS: f64 = 1.0;
const MAX_INTERVAL_MS: f64 = 2000.0;

/// One AGV pose within a frame.
struct Rec {
    serial: String,
    x: f32,
    y: f32,
    theta: f32,
}

/// A decoded frame plus the client-clock time it arrived, for interpolation.
struct Frame {
    recs: Vec<Rec>,
    time: f64,
}

/// The camera uniform, shared by all three pipelines. `repr(C)` + padding to a
/// 16-byte multiple so it matches the WGSL `Uniforms` layout.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: [f32; 16],
    viewport: [f32; 2],
    sizes: [f32; 2],
    misc: [f32; 2],
    _pad: [f32; 2],
}

// The shared [-1, 1] quad every pipeline instances over.
const QUAD_VERTS: [[f32; 2]; 4] = [[-1.0, -1.0], [1.0, -1.0], [-1.0, 1.0], [1.0, 1.0]];
const QUAD_INDICES: [u16; 6] = [0, 1, 2, 2, 1, 3];

/// TS `angleLerp`: interpolate along the shortest arc, handling the +/-pi seam.
fn angle_lerp(a: f32, b: f32, f: f32) -> f32 {
    let two_pi = std::f32::consts::TAU;
    let mut d = (b - a) % two_pi;
    if d > std::f32::consts::PI {
        d -= two_pi;
    } else if d < -std::f32::consts::PI {
        d += two_pi;
    }
    a + d * f
}

/// Port of `decode_frame` in `shared/src/protocol/protocol.rs`, inlined so the
/// renderer needn't depend on `shared` (and its non-wasm transitive deps).
fn decode_frame(data: &[u8]) -> Option<Vec<Rec>> {
    let count = u16::from_le_bytes(data.get(0..2)?.try_into().ok()?) as usize;
    let mut off = 2;
    let mut recs = Vec::with_capacity(count);
    for _ in 0..count {
        let len = u16::from_le_bytes(data.get(off..off + 2)?.try_into().ok()?) as usize;
        off += 2;
        let serial = String::from_utf8(data.get(off..off + len)?.to_vec()).ok()?;
        off += len;
        let x = f32::from_le_bytes(data.get(off..off + 4)?.try_into().ok()?);
        let y = f32::from_le_bytes(data.get(off + 4..off + 8)?.try_into().ok()?);
        let theta = f32::from_le_bytes(data.get(off + 8..off + 12)?.try_into().ok()?);
        off += 12;
        recs.push(Rec { serial, x, y, theta });
    }
    Some(recs)
}

/// All renderer state, mutated both by the rAF loop and by the JS-facing methods
/// (never concurrently -- wasm is single-threaded, so the `RefCell` borrows in
/// [`MapRenderer`] never overlap).
struct Inner {
    surface: Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    dpr: f32,
    running: bool,

    uniform_buf: wgpu::Buffer,
    uniform_bg: wgpu::BindGroup,

    quad_vbuf: wgpu::Buffer,
    quad_ibuf: wgpu::Buffer,

    edge_pipeline: wgpu::RenderPipeline,
    node_pipeline: wgpu::RenderPipeline,
    sprite_pipeline: wgpu::RenderPipeline,

    // Static map geometry, uploaded once by `set_map`.
    edge_ibuf: Option<wgpu::Buffer>,
    edge_count: u32,
    node_ibuf: Option<wgpu::Buffer>,
    node_count: u32,

    // Dynamic sprite instances, rewritten each frame.
    sprite_ibuf: wgpu::Buffer,
    sprite_cap: u32,
    sprite_count: u32,
    sprite_bgl: wgpu::BindGroupLayout,
    sprite_sampler: wgpu::Sampler,
    sprite_bg: Option<wgpu::BindGroup>,

    camera: Camera,
    paused: bool,
    cur: Option<Frame>,
    prev: Option<Frame>,
    prev_map: HashMap<String, (f32, f32, f32)>,
    latest_count: u32,

    // Cached each frame for the overlay getters.
    last_transform: Option<Transform>,
    robot_world: Vec<f32>,
    robot_serials: Vec<String>,

    fps: f32,
    frames: u32,
    fps_since: f64,
}

impl Inner {
    /// Advance interpolation and draw the GPU layer for one frame.
    fn frame(&mut self, now: f64) {
        // Smoothed FPS, a few times a second.
        self.frames += 1;
        if now - self.fps_since >= 250.0 {
            self.fps = (self.frames as f64 * 1000.0 / (now - self.fps_since)) as f32;
            self.frames = 0;
            self.fps_since = now;
        }

        let Some(t) = self.camera.transform() else {
            // Nothing to place against yet: just clear.
            self.clear_only();
            self.last_transform = None;
            return;
        };
        self.last_transform = Some(t);

        // --- interpolate robot poses -> sprite instances + overlay cache ---
        let size = camera::agv_size(&t);
        let mut instances: Vec<[f32; 4]> = Vec::new();
        self.robot_world.clear();
        self.robot_serials.clear();

        if let Some(cur) = &self.cur {
            let mut f = 1.0_f32;
            if let Some(prev) = &self.prev {
                let interval = (cur.time - prev.time).clamp(MIN_INTERVAL_MS, MAX_INTERVAL_MS);
                f = (((now - cur.time) / interval).clamp(0.0, 1.0)) as f32;
            }
            for r in &cur.recs {
                if !r.x.is_finite() || !r.y.is_finite() {
                    continue;
                }
                let (x, y, theta) = match self.prev_map.get(&r.serial) {
                    Some(&(px, py, pt)) => (
                        px + (r.x - px) * f,
                        py + (r.y - py) * f,
                        angle_lerp(pt, r.theta, f),
                    ),
                    None => (r.x, r.y, r.theta),
                };
                instances.push([x, y, theta, size]);
                self.robot_world.push(x);
                self.robot_world.push(y);
                self.robot_serials.push(r.serial.clone());
            }
        }

        self.sprite_count = instances.len() as u32;
        self.ensure_sprite_cap(self.sprite_count);
        if !instances.is_empty() {
            self.queue
                .write_buffer(&self.sprite_ibuf, 0, bytemuck::cast_slice(&instances));
        }

        // --- camera uniform ---
        let u = Uniforms {
            view_proj: t.view_proj(),
            viewport: [self.config.width as f32, self.config.height as f32],
            sizes: [EDGE_WIDTH_PX, NODE_RADIUS_PX],
            misc: [self.dpr, 0.0],
            _pad: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&u));

        self.render();
    }

    /// Grow the sprite instance buffer when the robot count outpaces it.
    fn ensure_sprite_cap(&mut self, n: u32) {
        if n <= self.sprite_cap {
            return;
        }
        let cap = n.next_power_of_two().max(64);
        self.sprite_ibuf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite instances"),
            size: cap as u64 * 16,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.sprite_cap = cap;
    }

    fn clear_only(&mut self) {
        let Some(frame_tex) = self.acquire() else {
            return;
        };
        let view = frame_tex
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let _rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(clear_attachment(&view))],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        self.queue.submit(std::iter::once(enc.finish()));
        self.queue.present(frame_tex);
    }

    fn render(&mut self) {
        let Some(frame_tex) = self.acquire() else {
            return;
        };
        let view = frame_tex
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(clear_attachment(&view))],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            rp.set_bind_group(0, &self.uniform_bg, &[]);
            rp.set_index_buffer(self.quad_ibuf.slice(..), wgpu::IndexFormat::Uint16);
            rp.set_vertex_buffer(0, self.quad_vbuf.slice(..));

            if let Some(buf) = &self.edge_ibuf {
                if self.edge_count > 0 {
                    rp.set_pipeline(&self.edge_pipeline);
                    rp.set_vertex_buffer(1, buf.slice(..));
                    rp.draw_indexed(0..6, 0, 0..self.edge_count);
                }
            }
            if let Some(buf) = &self.node_ibuf {
                if self.node_count > 0 {
                    rp.set_pipeline(&self.node_pipeline);
                    rp.set_vertex_buffer(1, buf.slice(..));
                    rp.draw_indexed(0..6, 0, 0..self.node_count);
                }
            }
            if let Some(bg) = &self.sprite_bg {
                if self.sprite_count > 0 {
                    rp.set_pipeline(&self.sprite_pipeline);
                    rp.set_bind_group(1, bg, &[]);
                    rp.set_vertex_buffer(1, self.sprite_ibuf.slice(..));
                    rp.draw_indexed(0..6, 0, 0..self.sprite_count);
                }
            }
        }
        self.queue.submit(std::iter::once(enc.finish()));
        self.queue.present(frame_tex);
    }

    /// Acquire this frame's surface texture, reconfiguring and skipping on loss.
    fn acquire(&self) -> Option<wgpu::SurfaceTexture> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => Some(t),
            _ => {
                self.surface.configure(&self.device, &self.config);
                None
            }
        }
    }
}

/// The JS-facing handle. A cheap clone of the shared `Inner`; every method takes
/// `&self` and borrows through the `RefCell`.
#[wasm_bindgen]
pub struct MapRenderer {
    inner: Rc<RefCell<Inner>>,
}

#[wasm_bindgen]
impl MapRenderer {
    /// Create the renderer on `canvas` and start its render loop. Async because
    /// requesting the GPU adapter/device is async.
    pub async fn create(canvas: HtmlCanvasElement) -> Result<MapRenderer, JsValue> {
        #[cfg(feature = "console_error_panic_hook")]
        console_error_panic_hook::set_once();

        let instance = wgpu::Instance::new(InstanceDescriptor::new_without_display_handle());
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|e| JsValue::from_str(&format!("create_surface: {e}")))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptionsBase {
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(|e| JsValue::from_str(&format!("request_adapter: {e}")))?;
        let (device, queue) = adapter
            .request_device(&DeviceDescriptor::default())
            .await
            .map_err(|e| JsValue::from_str(&format!("request_device: {e}")))?;

        let mut config = Surface::get_default_config(&surface, &adapter, canvas.width(), canvas.height())
            .ok_or_else(|| JsValue::from_str("no default surface config"))?;
        config.present_mode = wgpu::PresentMode::Immediate;
        surface.configure(&device, &config);
        let format = config.format;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("map shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        // group(0): the camera uniform, shared by every pipeline.
        let uniform_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("uniform bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("uniform bg"),
            layout: &uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });

        // group(1): the sprite texture + sampler (bound only when a sprite is set).
        let sprite_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprite bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sprite_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sprite sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let quad_vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad verts"),
            contents: bytemuck::cast_slice(&QUAD_VERTS),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let quad_ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad indices"),
            contents: bytemuck::cast_slice(&QUAD_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        // --- pipelines ---
        let corner_attrs = wgpu::vertex_attr_array![0 => Float32x2];
        let corner_layout = wgpu::VertexBufferLayout {
            array_stride: 8,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &corner_attrs,
        };

        let edge_attrs = wgpu::vertex_attr_array![1 => Float32x2, 2 => Float32x2];
        let node_attrs = wgpu::vertex_attr_array![1 => Float32x2];
        let sprite_attrs = wgpu::vertex_attr_array![1 => Float32x4];

        let opaque = make_pipeline(
            &device,
            &shader,
            "edge",
            format,
            &[Some(&uniform_bgl)],
            &[
                Some(corner_layout.clone()),
                Some(wgpu::VertexBufferLayout {
                    array_stride: 16,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &edge_attrs,
                }),
            ],
            wgpu::BlendState::REPLACE,
        );
        let node_pipeline = make_pipeline(
            &device,
            &shader,
            "node",
            format,
            &[Some(&uniform_bgl)],
            &[
                Some(corner_layout.clone()),
                Some(wgpu::VertexBufferLayout {
                    array_stride: 8,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &node_attrs,
                }),
            ],
            wgpu::BlendState::ALPHA_BLENDING,
        );
        let sprite_pipeline = make_pipeline(
            &device,
            &shader,
            "sprite",
            format,
            &[Some(&uniform_bgl), Some(&sprite_bgl)],
            &[
                Some(corner_layout.clone()),
                Some(wgpu::VertexBufferLayout {
                    array_stride: 16,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &sprite_attrs,
                }),
            ],
            wgpu::BlendState::ALPHA_BLENDING,
        );

        let sprite_ibuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite instances"),
            size: 64 * 16,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let inner = Inner {
            surface,
            device,
            queue,
            config,
            dpr: 1.0,
            running: true,
            uniform_buf,
            uniform_bg,
            quad_vbuf,
            quad_ibuf,
            edge_pipeline: opaque,
            node_pipeline,
            sprite_pipeline,
            edge_ibuf: None,
            edge_count: 0,
            node_ibuf: None,
            node_count: 0,
            sprite_ibuf,
            sprite_cap: 64,
            sprite_count: 0,
            sprite_bgl,
            sprite_sampler,
            sprite_bg: None,
            camera: Camera::new(),
            paused: false,
            cur: None,
            prev: None,
            prev_map: HashMap::new(),
            latest_count: 0,
            last_transform: None,
            robot_world: Vec::new(),
            robot_serials: Vec::new(),
            fps: 0.0,
            frames: 0,
            fps_since: 0.0,
        };

        let inner = Rc::new(RefCell::new(inner));
        start_loop(inner.clone());
        Ok(MapRenderer { inner })
    }

    /// Upload the static map geometry (world metres). `edges` is flattened
    /// segments `[ax, ay, bx, by, ...]`; `nodes` is `[x, y, ...]`.
    pub fn set_map(
        &self,
        edges: &[f32],
        nodes: &[f32],
        min_x: f32,
        max_x: f32,
        min_y: f32,
        max_y: f32,
    ) {
        let mut i = self.inner.borrow_mut();
        i.edge_count = (edges.len() / 4) as u32;
        i.edge_ibuf = (i.edge_count > 0).then(|| {
            i.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("edge instances"),
                contents: bytemuck::cast_slice(edges),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        i.node_count = (nodes.len() / 2) as u32;
        i.node_ibuf = (i.node_count > 0).then(|| {
            i.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("node instances"),
                contents: bytemuck::cast_slice(nodes),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        i.camera.seed_bounds(Bounds {
            min_x,
            max_x,
            min_y,
            max_y,
        });
    }

    /// Upload the AGV sprite once, as tightly-packed RGBA8 bytes.
    pub fn set_sprite(&self, rgba: &[u8], w: u32, h: u32) {
        let mut i = self.inner.borrow_mut();
        let size = wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        };
        let tex = i.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sprite"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        i.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * w),
                rows_per_image: Some(h),
            },
            size,
        );
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        i.sprite_bg = Some(i.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sprite bg"),
            layout: &i.sprite_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&i.sprite_sampler),
                },
            ],
        }));
    }

    /// Ingest one binary pose frame (the raw WebSocket payload). `now` is the
    /// client clock (`performance.now()`) at arrival, for interpolation timing.
    pub fn push_frame(&self, bytes: &[u8], now: f64) {
        let Some(recs) = decode_frame(bytes) else {
            return;
        };
        let mut i = self.inner.borrow_mut();
        i.latest_count = recs.len() as u32;
        if i.paused {
            return;
        }
        // Shift cur -> prev (building the id lookup), keep the newest as cur.
        if let Some(c) = i.cur.take() {
            i.prev_map = c
                .recs
                .iter()
                .map(|r| (r.serial.clone(), (r.x, r.y, r.theta)))
                .collect();
            i.prev = Some(c);
        }
        for r in &recs {
            i.camera.grow(r.x, r.y);
        }
        i.cur = Some(Frame { recs, time: now });
    }

    pub fn wheel(&self, px: f32, py: f32, delta_y: f32) {
        self.inner.borrow_mut().camera.wheel(px, py, delta_y);
    }

    pub fn pointer_down(&self) {
        self.inner.borrow_mut().camera.pointer_down();
    }

    pub fn pan(&self, dx: f32, dy: f32) {
        self.inner.borrow_mut().camera.pan(dx, dy);
    }

    /// Resize to the canvas backing store (`width`/`height` in device px).
    pub fn resize(&self, width: u32, height: u32, dpr: f32) {
        let mut i = self.inner.borrow_mut();
        if width == 0 || height == 0 {
            return;
        }
        i.config.width = width;
        i.config.height = height;
        let (device, config) = (&i.device, &i.config);
        i.surface.configure(device, config);
        i.dpr = dpr;
        i.camera
            .set_viewport(width as f32 / dpr, height as f32 / dpr);
    }

    pub fn set_paused(&self, paused: bool) {
        self.inner.borrow_mut().paused = paused;
    }

    pub fn reset_view(&self) {
        self.inner.borrow_mut().camera.reset();
    }

    /// `[scale, zoom, cx, cy, panX, panY, w, h]` for the overlay, or empty if the
    /// view isn't ready (nothing positioned yet).
    pub fn camera(&self) -> Vec<f32> {
        match self.inner.borrow().last_transform {
            Some(t) => vec![t.scale, t.zoom, t.cx, t.cy, t.pan_x, t.pan_y, t.w, t.h],
            None => Vec::new(),
        }
    }

    /// Interpolated robot positions in world metres, `[x, y, ...]`, index-aligned
    /// with [`robot_serials`](Self::robot_serials).
    pub fn robots(&self) -> Vec<f32> {
        self.inner.borrow().robot_world.clone()
    }

    pub fn robot_serials(&self) -> Vec<String> {
        self.inner.borrow().robot_serials.clone()
    }

    pub fn robot_count(&self) -> u32 {
        self.inner.borrow().latest_count
    }

    pub fn fps(&self) -> f32 {
        self.inner.borrow().fps
    }

    pub fn adjusted(&self) -> bool {
        self.inner.borrow().camera.adjusted()
    }

    /// Stop the render loop and let the loop closure free itself.
    pub fn destroy(&self) {
        self.inner.borrow_mut().running = false;
    }
}

/// The single colour attachment every pass uses: clear to the bg, then store.
fn clear_attachment(view: &wgpu::TextureView) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(CLEAR),
            store: wgpu::StoreOp::Store,
        },
    }
}

/// Build one instanced pipeline. `name` selects the `vs_<name>`/`fs_<name>`
/// entry points in the shared module.
fn make_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    name: &str,
    format: wgpu::TextureFormat,
    bind_group_layouts: &[Option<&wgpu::BindGroupLayout>],
    buffers: &[Option<wgpu::VertexBufferLayout>],
    blend: wgpu::BlendState,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(name),
        bind_group_layouts,
        immediate_size: 0,
    });
    let vs = format!("vs_{name}");
    let fs = format!("fs_{name}");
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(name),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(&vs),
            compilation_options: Default::default(),
            buffers,
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(&fs),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

/// Drive `Inner::frame` from `requestAnimationFrame`. The closure reschedules
/// itself; when `running` goes false it drops the stored closure, breaking the
/// reference cycle so it can be freed.
fn start_loop(inner: Rc<RefCell<Inner>>) {
    let f: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();
    *g.borrow_mut() = Some(Closure::wrap(Box::new(move |ts: f64| {
        {
            let mut b = inner.borrow_mut();
            if !b.running {
                drop(b);
                let _ = f.borrow_mut().take();
                return;
            }
            b.frame(ts);
        }
        request_animation_frame(f.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut(f64)>));
    request_animation_frame(g.borrow().as_ref().unwrap());
}

fn request_animation_frame(cb: &Closure<dyn FnMut(f64)>) {
    web_sys::window()
        .expect("no window")
        .request_animation_frame(cb.as_ref().unchecked_ref())
        .expect("requestAnimationFrame failed");
}
