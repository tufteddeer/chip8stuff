use egui::{ClippedPrimitive, Context, TexturesDelta, Ui};
use egui_wgpu::{renderer::ScreenDescriptor, wgpu, Renderer};

use pixels::PixelsContext;

use winit::event_loop::EventLoopWindowTarget;

use crate::chip8::{self, Mode};

pub struct EguiFramework {
    // State for egui.
    egui_ctx: Context,
    egui_state: egui_winit::State,
    screen_descriptor: ScreenDescriptor,
    renderer: Renderer,
    paint_jobs: Vec<ClippedPrimitive>,
    textures: TexturesDelta,
}

pub struct DebugGui {
    pub show_registers: bool,
    pub chip8_mode: chip8::Mode,
    pub registers: [u8; 16],
    pub set_mode: std::sync::mpsc::Sender<Mode>,
    pub step_sender: std::sync::mpsc::Sender<()>,
    pub instruction_history: Vec<chip8::instructions::Instruction>,
    pub show_instruction_history_window: bool,
}

impl EguiFramework {
    /// Create egui.
    pub(crate) fn new<T>(
        event_loop: &EventLoopWindowTarget<T>,
        width: u32,
        height: u32,
        scale_factor: f32,
        pixels: &pixels::Pixels,
    ) -> Self {
        let max_texture_size = pixels.device().limits().max_texture_dimension_2d as usize;

        let egui_ctx = Context::default();
        let mut egui_state = egui_winit::State::new(event_loop);
        egui_state.set_max_texture_side(max_texture_size);
        egui_state.set_pixels_per_point(scale_factor);
        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [width, height],
            pixels_per_point: scale_factor,
        };
        let renderer = Renderer::new(pixels.device(), pixels.render_texture_format(), None, 1);
        let textures = TexturesDelta::default();

        Self {
            egui_ctx,
            egui_state,
            screen_descriptor,
            renderer,
            paint_jobs: Vec::new(),
            textures,
        }
    }

    /// Handle input events from the window manager.
    pub(crate) fn handle_event(&mut self, event: &winit::event::WindowEvent) {
        let _ = self.egui_state.on_event(&self.egui_ctx, event);
    }

    /// Resize egui.
    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.screen_descriptor.size_in_pixels = [width, height];
        }
    }

    /// Update scaling factor.
    pub(crate) fn scale_factor(&mut self, scale_factor: f64) {
        self.screen_descriptor.pixels_per_point = scale_factor as f32;
    }

    /// Prepare egui.
    pub(crate) fn prepare(&mut self, window: &winit::window::Window, model: &mut DebugGui) {
        // Run the egui frame and create all paint jobs to prepare for rendering.
        let raw_input = self.egui_state.take_egui_input(window);
        let output = self.egui_ctx.run(raw_input, |egui_ctx| {
            // Draw the demo application.
            model.ui(egui_ctx);
        });

        self.textures.append(output.textures_delta);
        self.egui_state
            .handle_platform_output(window, &self.egui_ctx, output.platform_output);
        self.paint_jobs = self.egui_ctx.tessellate(output.shapes);
    }

    /// Render egui.
    pub(crate) fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        render_target: &wgpu::TextureView,
        context: &PixelsContext,
    ) {
        // Upload all resources to the GPU.
        for (id, image_delta) in &self.textures.set {
            self.renderer
                .update_texture(&context.device, &context.queue, *id, image_delta);
        }
        self.renderer.update_buffers(
            &context.device,
            &context.queue,
            encoder,
            &self.paint_jobs,
            &self.screen_descriptor,
        );

        // Render egui with WGPU
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: render_target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: true,
                    },
                })],
                depth_stencil_attachment: None,
            });

            self.renderer
                .render(&mut rpass, &self.paint_jobs, &self.screen_descriptor);
        }

        // Cleanup
        let textures = std::mem::take(&mut self.textures);
        for id in &textures.free {
            self.renderer.free_texture(id);
        }
    }
}

impl DebugGui {
    /// Create the UI using egui.
    fn ui(&mut self, ctx: &Context) {
        egui::TopBottomPanel::top("menubar_container").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                self.play_pause_step(ctx, ui);

                if ui.button("Registers").clicked() {
                    self.show_registers = !self.show_registers;
                }

                if ui.button("Instructions").clicked() {
                    self.show_instruction_history_window = !self.show_instruction_history_window;
                }
            });
        });

        self.register_window(ctx);

        self.instruction_history_window(ctx);
    }

    fn play_pause_step(&mut self, ctx: &Context, ui: &mut Ui) {
        let (label, new_mode) = match self.chip8_mode {
            Mode::Running => ("Pause", Mode::Paused),
            Mode::WaitForKey { register } => ("GETKEY", Mode::WaitForKey { register }),
            Mode::Paused => ("Play", Mode::Running),
        };

        if ui.button(label).clicked() {
            self.set_mode.send(new_mode).unwrap();
        }

        if self.chip8_mode == Mode::Paused && ui.button("Step").clicked() {
            self.step_sender.send(()).unwrap();
        }
    }

    fn register_window(&mut self, ctx: &Context) {
        egui::Window::new("Registers")
            .open(&mut self.show_registers)
            .show(ctx, |ui| {
                egui::Grid::new("register_grid").show(ui, |ui| {
                    for i in 0..16 {
                        ui.label(format!("{i:X}:"));
                        ui.label(format!("{:X}", self.registers[i]));
                        ui.end_row();
                    }
                });
            });
    }

    fn instruction_history_window(&mut self, ctx: &Context) {
        egui::Window::new("Instructions")
            .open(&mut self.show_instruction_history_window)
            .scroll2([false, true])
            .show(ctx, |ui| {
                for instruction in self.instruction_history.iter().rev().take(20).rev() {
                    ui.label(format!("{instruction:?}"));
                    ui.end_row();
                }
            });
    }
}
