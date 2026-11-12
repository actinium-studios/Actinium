use std::sync::Arc;

mod utils;

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Icon, Window, WindowId},
};

use utils::discord::Presence;

const APP_NAME: &str = "Actinium Game Engine";

fn load_icon() -> Icon {
    let image = image::load_from_memory(include_bytes!("../assets/logo128.png"))
        .expect("failed to load embedded icon")
        .into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    Icon::from_rgba(rgba, width, height).expect("failed to create icon from rgba data")
}

struct GpuState {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_config: wgpu::SurfaceConfiguration,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
}

struct App {
    window: Option<Arc<Window>>,
    gpu: Option<GpuState>,
    menu_bar: crate::utils::menu_bar::MenuBarState,
    update_banner: crate::utils::update_banner::UpdateBanner,
}

impl App {
    fn new() -> Self {
        Self {
            window: None,
            gpu: None,
            menu_bar: crate::utils::menu_bar::MenuBarState::new(),
            update_banner: crate::utils::update_banner::UpdateBanner::new(),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return; // already set up (e.g. resumed after being suspended)
        }

        let icon = load_icon();

        let window_attributes = Window::default_attributes()
            .with_title(APP_NAME)
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0))
            .with_window_icon(Some(icon));

        let window = Arc::new(
            event_loop
                .create_window(window_attributes)
                .expect("failed to create window"),
        );

        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance
            .create_surface(window.clone())
            .expect("failed to create surface");

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
            .expect("failed to find a suitable GPU adapter");

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Actinium Device"),
                ..Default::default()
            },
        ))
            .expect("failed to create device");

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui_ctx.viewport_id(),
            &window,
            None,
            None,
            None,
        );
        let egui_renderer =
            egui_wgpu::Renderer::new(&device, surface_format, egui_wgpu::RendererOptions::default());

        self.gpu = Some(GpuState {
            surface,
            device,
            queue,
            surface_config,
            egui_ctx,
            egui_state,
            egui_renderer,
        });
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        let (Some(window), Some(gpu)) = (&self.window, &mut self.gpu) else {
            return;
        };

        let response = gpu.egui_state.on_window_event(window, &event);
        if response.consumed {
            window.request_redraw();
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(new_size) => {
                if new_size.width > 0 && new_size.height > 0 {
                    gpu.surface_config.width = new_size.width;
                    gpu.surface_config.height = new_size.height;
                    gpu.surface.configure(&gpu.device, &gpu.surface_config);
                }
            }

            WindowEvent::RedrawRequested => {
                let raw_input = gpu.egui_state.take_egui_input(window);

                let mut full_output = gpu.egui_ctx.run_ui(raw_input, |ui| {
                    self.menu_bar.show(ui);
                    self.update_banner.show(ui);
                    egui::CentralPanel::default().show(ui, |_ui| {});
                });

                let frame = match gpu.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(frame)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
                    wgpu::CurrentSurfaceTexture::Outdated
                    | wgpu::CurrentSurfaceTexture::Lost => {
                        full_output.drop_without_applying_deltas();
                        gpu.surface.configure(&gpu.device, &gpu.surface_config);
                        window.request_redraw();
                        return;
                    }
                    wgpu::CurrentSurfaceTexture::Timeout
                    | wgpu::CurrentSurfaceTexture::Occluded
                    | wgpu::CurrentSurfaceTexture::Validation => {
                        full_output.drop_without_applying_deltas();
                        return;
                    }
                };
                gpu.egui_state
                    .handle_platform_output(window, full_output.platform_output);

                let tris = gpu
                    .egui_ctx
                    .tessellate(full_output.shapes, full_output.pixels_per_point);

                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                for (id, image_delta) in &full_output.textures_delta.set {
                    for delta in image_delta {
                        gpu.egui_renderer
                            .update_texture(&gpu.device, &gpu.queue, *id, delta);
                    }
                }

                let mut encoder = gpu
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("Actinium Encoder"),
                    });

                let screen_descriptor = egui_wgpu::ScreenDescriptor {
                    size_in_pixels: [gpu.surface_config.width, gpu.surface_config.height],
                    pixels_per_point: full_output.pixels_per_point,
                };

                gpu.egui_renderer.update_buffers(
                    &gpu.device,
                    &gpu.queue,
                    &mut encoder,
                    &tris,
                    &screen_descriptor,
                );

                {
                    let mut render_pass = encoder
                        .begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("Actinium Render Pass"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &view,
                                depth_slice: None,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color {
                                        r: 0.1,
                                        g: 0.1,
                                        b: 0.1,
                                        a: 1.0,
                                    }),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                            multiview_mask: None,
                        })
                        .forget_lifetime();

                    gpu.egui_renderer
                        .render(&mut render_pass, &tris, &screen_descriptor);
                }

                for id in &full_output.textures_delta.free {
                    gpu.egui_renderer.free_texture(id);
                }
                full_output.textures_delta.clear();

                gpu.queue.submit(Some(encoder.finish()));
                gpu.queue.present(frame);

                window.request_redraw();
            }

            _ => {}
        }
    }
}

fn main() {

    match crate::utils::os_check::check_os_supported() {
        crate::utils::os_check::OsCheckResult::Unsupported { detected, message } => {
            crate::utils::os_check::block_and_exit(&detected, &message);
        }
        crate::utils::os_check::OsCheckResult::Supported => {}
    }

    let presence = Presence::start("1551696179983818914");

    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    presence.set("Making a game", "");

    let mut app = App::new();
    event_loop.run_app(&mut app).expect("event loop error");

    println!("what is uppies guys");
}
