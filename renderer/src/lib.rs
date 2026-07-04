use wasm_bindgen::prelude::wasm_bindgen;
use web_sys::HtmlCanvasElement;
use wgpu::{DeviceDescriptor, InstanceDescriptor, Surface};

#[wasm_bindgen]
pub async fn renderer(canvas: HtmlCanvasElement) {
    web_sys::console::log_1(&canvas);

    let instance = wgpu::Instance::new(InstanceDescriptor::new_without_display_handle());
    let surface = instance
        .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
        .expect("Cannot create surface");
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptionsBase {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            ..Default::default()
        })
        .await
        .expect("Cannot create adapter");
    let (device, queue) = adapter
        .request_device(&DeviceDescriptor::default())
        .await
        .expect("Cannot create device from adapter");

    surface.configure(
        &device,
        &Surface::get_default_config(&surface, &adapter, canvas.width(), canvas.height())
            .expect("Cannot create default surface config"),
    );

    // --- everything below this line was written by Claude ---

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("triangle shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("triangle pipeline layout"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });

    let swapchain_format = surface.get_capabilities(&adapter).formats[0];

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("triangle pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: None,
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: None,
            compilation_options: Default::default(),
            targets: &[Some(swapchain_format.into())],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    let surface_texture = match surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
        _ => panic!("failed to acquire surface texture"),
    };
    let view = surface_texture
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("triangle encoder"),
    });

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("triangle render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.05,
                        g: 0.05,
                        b: 0.08,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        render_pass.set_pipeline(&pipeline);
        render_pass.draw(0..3, 0..1);
    }

    queue.submit(std::iter::once(encoder.finish()));
    queue.present(surface_texture);
}
