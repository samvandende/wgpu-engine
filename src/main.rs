use egui::FontDefinitions;
use egui_wgpu_backend::{RenderPass, ScreenDescriptor};
use winit::{event::Event::*, event_loop::{ControlFlow, EventLoop}};
use egui_winit_platform::{Platform, PlatformDescriptor};

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;
enum RedrawEvent {
    RequestRedraw,
}
enum EngineEvent {
    Update { dt: f64 },
}

struct RepaintSignal(std::sync::Mutex<winit::event_loop::EventLoopProxy<RedrawEvent>>);
impl epi::backend::RepaintSignal for RepaintSignal {
    fn request_repaint(&self) {
        self.0.lock().unwrap().send_event(RedrawEvent::RequestRedraw).ok();
    }
}

struct Engine {
    event_loop: Option<winit::event_loop::EventLoop<RedrawEvent>>,
    window: Option<winit::window::Window>,
    render_state: Option<RenderState>,
}

impl Engine {
    async fn load() -> Self {
        let event_loop = EventLoop::with_user_event();

        let window = winit::window::WindowBuilder::new()
            .with_decorations(true)
            .with_resizable(true)
            .with_transparent(false)
            .with_title("wgpu-engine")
            .with_inner_size(winit::dpi::PhysicalSize {
                width: WIDTH,
                height: HEIGHT,
            })
            .build(&event_loop)
            .unwrap();
        
        let render_state = RenderState::new(&event_loop, &window).await;
        

        Engine {
            event_loop: Some(event_loop),
            window: Some(window),
            render_state: Some(render_state),
        }
    }

    fn run(&mut self) {
        let mut event_loop = self.event_loop.take().unwrap();
        let window = self.window.take().unwrap();
        let mut render_state = self.render_state.take().unwrap();

        let mut time = std::time::Instant::now();
        let start_time = time;
        
        event_loop.run(move |event, _, control_flow| {
            render_state.platform.handle_event(&event);
            match event {
                RedrawRequested(..) => {
                    let _dt = time.elapsed().as_secs_f32();
                    time = std::time::Instant::now();
    
                    render_state.update(&start_time);
                    render_state.render(&window);
                },
                MainEventsCleared | UserEvent(RedrawEvent::RequestRedraw) => {
                    window.request_redraw();
                },
                WindowEvent { event, ..} => match event {
                    winit::event::WindowEvent::Resized(size) => {
                        render_state.resize(size);
                    }
                    winit::event::WindowEvent::CloseRequested => {
                        *control_flow = ControlFlow::Exit;
                    }
                    _ => {}
                },
                _ => (),
            }
        });
    }
}

struct RenderState {
    size: winit::dpi::PhysicalSize<u32>,
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_config: wgpu::SurfaceConfiguration,

    previous_ui_draw_time: Option<f32>,
    repaint_signal: std::sync::Arc<RepaintSignal>,
    platform: Platform,
    egui_render_pass: RenderPass,
}

impl RenderState {
    async fn new(event_loop: &EventLoop<RedrawEvent>, window: &winit::window::Window) -> Self {
        let backends = wgpu::Backends::VULKAN;
        let power_preference = wgpu::PowerPreference::HighPerformance;
        let present_mode = wgpu::PresentMode::Fifo;


        let size = window.inner_size();
        let instance = wgpu::Instance::new(backends);
        let surface = unsafe { instance.create_surface(window) };

        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }).await.unwrap();

        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                features: wgpu::Features::default(),
                limits: wgpu::Limits::default(),
                label: None,
            },
            None,
        ).await.unwrap();

        let surface_format = surface.get_preferred_format(&adapter).unwrap();
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode,
        };
        surface.configure(&device, &surface_config);

        let previous_ui_draw_time = None;
        let repaint_signal = std::sync::Arc::new(RepaintSignal(std::sync::Mutex::new(
            event_loop.create_proxy(),
        )));

        let platform = Platform::new(PlatformDescriptor {
            physical_width: size.width,
            physical_height: size.height,
            scale_factor: window.scale_factor(),
            font_definitions: FontDefinitions::default(),
            style: Default::default(),
        });

        let egui_render_pass = RenderPass::new(&device, surface_format, 1);

        RenderState {
            size,
            surface,
            device,
            queue,
            surface_config,

            previous_ui_draw_time,
            repaint_signal,
            platform,
            egui_render_pass,
        }
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.surface_config.width = new_size.width;
            self.surface_config.height = new_size.height;
            self.surface.configure(&self.device, &self.surface_config);
        }
    }

    fn update(&mut self, start_time: &std::time::Instant) {
        self.platform.update_time(start_time.elapsed().as_secs_f64());
    }

    fn render(&mut self, window: &winit::window::Window) {
        let output_frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Outdated) => { return; }
            Err(e) => {
                eprintln!("Dropped frame with error: {}", e);
                return;
            }
        };
        let output_view = output_frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

        // render the UI
        let ui_start_time = std::time::Instant::now();
        self.platform.begin_frame();
        let app_output = epi::backend::AppOutput::default();
        let _frame = epi::Frame::new(epi::backend::FrameData {
            info: epi::IntegrationInfo {
                name: "egui_wgpu",
                web_info: None,
                cpu_usage: self.previous_ui_draw_time,
                native_pixels_per_point: Some(window.scale_factor() as _),
                prefer_dark_mode: None,
            },
            output: app_output,
            repaint_signal: self.repaint_signal.clone(),
        });

        egui::SidePanel::left("left panel").show(&self.platform.context(), |ui| {
            ui.heading("Left side panel");
            ui.label(format!("Frame time: {} ms", self.previous_ui_draw_time.unwrap_or(0.0) * 1000.0));
            ui.horizontal(|ui| {
                let mut txt: String = "".into();
                ui.label("edit some text: ");
                ui.text_edit_singleline(&mut txt);
            });
            if ui.button("clicky thing").clicked() {
                ui.label("no touchy!");
            } else {
                ui.label("touch the button!");
            }
        });
        // egui::Window::new("mah window").show(&self.platform.context(), |ui| {
        //     ui.heading("this is a test window");
        // });

        // egui::Window::new("plot window").show(&platform.context(), |ui| {
        //     ui.heading("a beautiful plot");
        //     let plot = egui::plot::Plot::new("lines").legend(egui::plot::Legend::default());
        //     plot.show(ui, |plot_ui| {
        //         plot_ui.line(egui::plot::Line::new(egui::plot::Values::from_ys_f32(&ys[..])));
        //     });
        // });

        let (_output, paint_commands) = self.platform.end_frame(Some(&window));
        let paint_jobs = self.platform.context().tessellate(paint_commands);
        
        self.previous_ui_draw_time = Some(ui_start_time.elapsed().as_secs_f32());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("encoder"),
        });

        let screen_descriptor = ScreenDescriptor {
            physical_width: self.surface_config.width,
            physical_height: self.surface_config.height,
            scale_factor: window.scale_factor() as f32,
        };
        self.egui_render_pass.update_texture(&self.device, &self.queue, &self.platform.context().font_image());
        self.egui_render_pass.update_user_textures(&self.device, &self.queue);
        self.egui_render_pass.update_buffers(&self.device, &self.queue, &paint_jobs, &screen_descriptor);

        self.egui_render_pass.execute(
            &mut encoder,
            &output_view,
            &paint_jobs,
            &screen_descriptor,
            Some(wgpu::Color::BLACK),
        ).unwrap();

        self.queue.submit(std::iter::once(encoder.finish()));

        output_frame.present();
    }
}

fn main() {
    // let event_loop = EventLoop::with_user_event();
    let mut engine = pollster::block_on(Engine::load());
    engine.run();
    // let mut time = std::time::Instant::now();
    // let start_time = time;
    
    // event_loop.run(move |event, _, control_flow| {
    //     engine.render_state.platform.handle_event(&event);
    //     match event {
    //         RedrawRequested(..) => {
    //             let _dt = time.elapsed().as_secs_f32();
    //             time = std::time::Instant::now();

    //             engine.render_state.update(&start_time);
    //             engine.render_state.render(&engine.window);
    //         },
    //         MainEventsCleared | UserEvent(RedrawEvent::RequestRedraw) => {
    //             engine.window.request_redraw();
    //         },
    //         WindowEvent { event, ..} => match event {
    //             winit::event::WindowEvent::Resized(size) => {
    //                 engine.render_state.resize(size);
    //             }
    //             winit::event::WindowEvent::CloseRequested => {
    //                 *control_flow = ControlFlow::Exit;
    //             }
    //             _ => {}
    //         },
    //         _ => (),
    //     }
    // });
}
