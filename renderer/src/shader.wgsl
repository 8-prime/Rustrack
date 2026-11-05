// The instanced pipelines share one camera uniform. Every pipeline draws the
// same [-1, 1] unit quad (4 verts, 6 indices); each vertex shader reinterprets
// the corner. Geometry lives in world metres in the instance buffers and is
// never rebuilt on zoom -- only `u.view_proj` changes.

struct Uniforms {
    view_proj: mat4x4<f32>,
    // Framebuffer size in device pixels, used to convert pixel-space widths
    // (which must stay constant on screen) into clip-space offsets.
    viewport: vec2<f32>,
    // sizes.x = edge width (CSS px), sizes.y = node radius (CSS px).
    sizes: vec2<f32>,
    // misc.x = devicePixelRatio, misc.y = route width (CSS px).
    misc: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

// Convert a device-pixel offset into a clip-space offset. 1 clip unit spans
// viewport/2 pixels, so pixels -> clip is `* 2 / viewport`.
fn px_to_clip(offset_px: vec2<f32>) -> vec2<f32> {
    return offset_px * 2.0 / u.viewport;
}

// Expand a [-1, 1] quad corner along the segment a-b into a line `width_px` CSS
// px wide, constant on screen.
fn segment_vertex(corner: vec2<f32>, a: vec2<f32>, b: vec2<f32>, width_px: f32) -> vec4<f32> {
    let ca = u.view_proj * vec4<f32>(a, 0.0, 1.0);
    let cb = u.view_proj * vec4<f32>(b, 0.0, 1.0);

    // Direction of the segment in pixel space (w == 1 here, so xy is clip/NDC).
    let dir_px = (cb.xy - ca.xy) * u.viewport;
    let len = length(dir_px);
    // Degenerate (zero-length) segments get no thickness rather than a NaN.
    var normal = vec2<f32>(0.0, 0.0);
    if (len > 1e-6) {
        let dir = dir_px / len;
        normal = vec2<f32>(-dir.y, dir.x);
    }

    // corner.x in [-1, 1] -> t in [0, 1] along the segment; corner.y is the side.
    let t = corner.x * 0.5 + 0.5;
    let base = mix(ca.xy, cb.xy, t);
    let half_w = width_px * u.misc.x * 0.5;
    let offset = px_to_clip(normal * half_w * corner.y);
    return vec4<f32>(base + offset, 0.0, 1.0);
}

// ---------------------------------------------------------------- edges

struct EdgeVsIn {
    @location(0) corner: vec2<f32>, // [-1, 1] quad
    @location(1) a: vec2<f32>,      // segment start, world metres
    @location(2) b: vec2<f32>,      // segment end, world metres
};

@vertex
fn vs_edge(in: EdgeVsIn) -> @builtin(position) vec4<f32> {
    return segment_vertex(in.corner, in.a, in.b, u.sizes.x);
}

@fragment
fn fs_edge() -> @location(0) vec4<f32> {
    // CANVAS.edge (#c3c9d1).
    return vec4<f32>(0.765, 0.788, 0.820, 1.0);
}

// ---------------------------------------------------------------- nodes

struct NodeVsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>, // [-1, 1], for the circle SDF
};

@vertex
fn vs_node(@location(0) corner: vec2<f32>, @location(1) center: vec2<f32>) -> NodeVsOut {
    let c = (u.view_proj * vec4<f32>(center, 0.0, 1.0)).xy;
    let r = u.sizes.y * u.misc.x;
    var out: NodeVsOut;
    out.pos = vec4<f32>(c + px_to_clip(corner * r), 0.0, 1.0);
    out.uv = corner;
    return out;
}

@fragment
fn fs_node(in: NodeVsOut) -> @location(0) vec4<f32> {
    let d = length(in.uv);
    let aa = fwidth(d);
    let alpha = 1.0 - smoothstep(1.0 - aa, 1.0, d);
    // CANVAS.node (#5d646e), straight alpha for the blend pipeline.
    return vec4<f32>(0.365, 0.392, 0.431, alpha);
}

// ---------------------------------------------------------------- route

struct RouteVsIn {
    @location(0) corner: vec2<f32>, // [-1, 1] quad
    @location(1) a: vec2<f32>,      // segment start, world metres
    @location(2) b: vec2<f32>,      // segment end, world metres
    @location(3) color: vec4<f32>,  // straight alpha
};

struct RouteVsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_route(in: RouteVsIn) -> RouteVsOut {
    var out: RouteVsOut;
    out.pos = segment_vertex(in.corner, in.a, in.b, u.misc.y);
    out.color = in.color;
    return out;
}

@fragment
fn fs_route(in: RouteVsOut) -> @location(0) vec4<f32> {
    return in.color;
}

// ---------------------------------------------------------------- sprites

struct SpriteVsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_sprite(
    @location(0) corner: vec2<f32>,       // [-1, 1] quad
    @location(1) instance: vec4<f32>,     // x, y, theta, size_px
) -> SpriteVsOut {
    let center = (u.view_proj * vec4<f32>(instance.xy, 0.0, 1.0)).xy;
    let theta = instance.z;
    let size = instance.w;

    // Match the old Canvas2D convention: rotate by (pi/2 - theta) in screen
    // space (y-down), so the sprite nose points along the world heading.
    let r = 1.5707963 - theta;
    let cs = cos(r);
    let sn = sin(r);
    let local = corner * 0.5 * size;         // CSS px, image space (y-down)
    let screen_off = vec2<f32>(
        local.x * cs - local.y * sn,
        local.x * sn + local.y * cs,
    );
    // Screen y-down -> clip y-up: negate y before converting to clip.
    let off = px_to_clip(vec2<f32>(screen_off.x, -screen_off.y) * u.misc.x);

    var out: SpriteVsOut;
    out.pos = vec4<f32>(center + off, 0.0, 1.0);
    // corner [-1,1] -> uv [0,1], y-down to match the image.
    out.uv = corner * 0.5 + 0.5;
    return out;
}

@group(1) @binding(0) var sprite_tex: texture_2d<f32>;
@group(1) @binding(1) var sprite_smp: sampler;

@fragment
fn fs_sprite(in: SpriteVsOut) -> @location(0) vec4<f32> {
    return textureSample(sprite_tex, sprite_smp, in.uv);
}
