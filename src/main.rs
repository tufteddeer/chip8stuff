#![feature(bigint_helper_methods)]

mod chip8;
mod debug_gui;

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use chip8::Chip8;
use clap::Parser;

use log::LevelFilter;
use pixels::{Pixels, SurfaceTexture};
use simple_logger::SimpleLogger;
use winit::{
    dpi::LogicalSize,
    event::{Event, VirtualKeyCode},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use winit_input_helper::WinitInputHelper;

use crate::{
    chip8::{instructions::Instruction, Mode},
    debug_gui::{DebugGui, EguiFramework},
};

// How many pixel we display per vram pixel
const DISPLAY_WINDOW_SCALE: u32 = 10;
const WINDOW_WIDTH: u32 = chip8::DISPLAY_WIDTH as u32 * 10;
const WINDOW_HEIGHT: u32 = chip8::DISPLAY_HEIGHT as u32 * 10;

// Instruction cycle frequency
const TARGET_FREQUENCY: f32 = 800.0; // hz;

const LOG_TARGET_WINIT_INPUT: &str = "WINIT_INPUT";
const LOG_TARGET_TIMING: &str = "TIMING";
const LOG_TARGET_RENDERING: &str = "RENDER";

const KEY_BINDINGS: [VirtualKeyCode; 16] = [
    VirtualKeyCode::Key0,
    VirtualKeyCode::Key1,
    VirtualKeyCode::Key2,
    VirtualKeyCode::Key3,
    VirtualKeyCode::Key4,
    VirtualKeyCode::Key5,
    VirtualKeyCode::Key6,
    VirtualKeyCode::Key7,
    VirtualKeyCode::Key8,
    VirtualKeyCode::Key9,
    VirtualKeyCode::A,
    VirtualKeyCode::B,
    VirtualKeyCode::C,
    VirtualKeyCode::D,
    VirtualKeyCode::E,
    VirtualKeyCode::F,
];

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    rom_file: String,
    /// Start interpreter in paused mode
    #[arg(short, long)]
    paused: bool,
}

fn main() -> anyhow::Result<()> {
    SimpleLogger::new()
        // dependencies
        .with_module_level("wgpu_core", LevelFilter::Warn)
        .with_module_level("mio", LevelFilter::Warn)
        .with_module_level("winit", LevelFilter::Warn)
        .with_module_level("wgpu_hal", LevelFilter::Warn)
        .with_module_level("naga", LevelFilter::Warn)
        // chip8 log targets
        .with_module_level(chip8::LOG_TARGET_INPUT, LevelFilter::Info)
        .with_module_level(chip8::LOG_TARGET_INSTRUCTIONS, LevelFilter::Trace)
        .with_module_level(chip8::LOG_TARGET_DRAWING, LevelFilter::Info)
        .with_module_level(chip8::LOG_TARGET_TIMER, LevelFilter::Info)
        // interpreter log targets
        .with_module_level(LOG_TARGET_RENDERING, LevelFilter::Warn)
        .with_module_level(LOG_TARGET_TIMING, LevelFilter::Warn)
        .with_module_level(LOG_TARGET_WINIT_INPUT, LevelFilter::Warn)
        .init()?;

    let args = Args::parse();

    let mut chip8 = Chip8::new();

    if args.paused {
        chip8.mode = Mode::Paused;
    }

    chip8.load_rom(&args.rom_file)?;

    log::info!("Loaded rom file {}", args.rom_file);

    let event_loop = EventLoop::new();
    let mut input = WinitInputHelper::new();
    let window = {
        let size = LogicalSize::new(WINDOW_WIDTH as f64, WINDOW_HEIGHT as f64);
        WindowBuilder::new()
            .with_title("CHIP8")
            .with_inner_size(size)
            .with_min_inner_size(size)
            .build(&event_loop)
            .unwrap()
    };

    let mut pixels = {
        let window_size = window.inner_size();
        let surface_texture = SurfaceTexture::new(window_size.width, window_size.height, &window);
        Pixels::new(WINDOW_WIDTH, WINDOW_HEIGHT, surface_texture)?
    };

    let mut framework = EguiFramework::new(
        &event_loop,
        WINDOW_WIDTH,
        WINDOW_HEIGHT,
        window.scale_factor() as f32,
        &pixels,
    );

    let framebuffer = [0_u8; (WINDOW_WIDTH * WINDOW_HEIGHT) as usize * 4];

    let time_per_instruction: Duration = Duration::from_secs_f32(1.0 / TARGET_FREQUENCY);

    let mut delay_timer_decrease_counter = 0;

    let chip8 = Arc::new(Mutex::new(chip8));

    // Framebuffer caches the scaled up vram pixels as they should be rendered.
    // it is copied into the Pixels framebuffer before rendering.
    // This avoids frequently redrawing the vram when the window is updated
    let framebuffer = Arc::new(Mutex::new(framebuffer));

    // Some channels to send information between the debugger ui and the chip8 interpreter

    let (new_mode_sender, new_mode_receiver) = std::sync::mpsc::channel();
    let (step_sender, step_receiver) = std::sync::mpsc::channel::<()>();
    let (instructions_sender, instructions_receiver) = std::sync::mpsc::channel::<Instruction>();

    std::thread::spawn({
        let chip8 = chip8.clone();
        let framebuffer = framebuffer.clone();
        move || loop {
            let last_cycle_finished = Instant::now();
            let mut chip8 = chip8.lock().unwrap();
            chip8.redraw = false;

            if let Ok(new_mode) = new_mode_receiver.try_recv() {
                chip8.mode = new_mode;
            }

            if chip8.mode == Mode::Running
                // if we are paused, wait until the next step is executed via debugger
                || chip8.mode == Mode::Paused && step_receiver.try_recv().is_ok()
            {
                let instruction = chip8.step_cycle().unwrap();
                instructions_sender.send(instruction).unwrap();

                // decrease the 60hz timer every x instructions, depending on our instruction execution frequency
                delay_timer_decrease_counter += 1;
                if delay_timer_decrease_counter
                    == (TARGET_FREQUENCY / chip8::DELAY_TIMER_FREQUENCY).floor() as i32
                {
                    if chip8.delay_timer > 0 {
                        chip8.delay_timer -= 1;
                    }
                    delay_timer_decrease_counter = 0;
                }

                if chip8.redraw {
                    println!("rendering into framebuffer");
                    let mut f = framebuffer.lock().unwrap();
                    render_vram(&chip8.vram, &mut *f);
                }
                chip8.redraw = false;
            }

            // decrease the 60hz timer every x instructions, depending on our instruction execution frequency
            delay_timer_decrease_counter += 1;
            if delay_timer_decrease_counter
                == (TARGET_FREQUENCY / chip8::DELAY_TIMER_FREQUENCY).floor() as i32
            {
                if chip8.delay_timer > 0 {
                    chip8.delay_timer -= 1;
                }
                delay_timer_decrease_counter = 0;
            }

            drop(chip8);

            // wait for some time so we can operate at our target frequency
            if last_cycle_finished.elapsed() < time_per_instruction {
                let time_left = time_per_instruction - last_cycle_finished.elapsed();
                log::trace!(target: LOG_TARGET_TIMING, "Sleeping for {time_left:?}");
                std::thread::sleep(time_left);
            } else {
                log::warn!(target:LOG_TARGET_TIMING, "Instruction execution took {:?}, falling behind our target execution frequency", last_cycle_finished.elapsed());
            }
        }
    });

    let c = chip8.lock().unwrap();
    let mut debug_gui = DebugGui {
        chip8_mode: c.mode,
        show_registers: false,
        registers: c.registers,
        set_mode: new_mode_sender,
        step_sender,
        instruction_history: Vec::new(),
        show_instruction_history_window: true,
        pc: c.pc,
        address_register: c.address_register,
    };
    drop(c);

    event_loop.run(move |event, _, control_flow| {
        // Handle input events
        if input.update(&event) {
            // Close events
            if input.key_pressed(VirtualKeyCode::Escape) || input.close_requested() {
                *control_flow = ControlFlow::Exit;
                return;
            }

            KEY_BINDINGS.iter().enumerate().for_each(|(i, key)| {
                let mut chip8 = chip8.lock().unwrap();

                if input.key_pressed(*key) {
                    chip8.keyboard.set_down(i as u8);

                    log::trace!(target: LOG_TARGET_WINIT_INPUT, "key down: 0x{i:X}");
                } else if input.key_released(*key) {
                    chip8.keyboard.set_up(i as u8);

                    if let Mode::WaitForKey { register } = chip8.mode {
                        chip8.registers[register] = i as u8;
                        chip8.mode = Mode::Running;
                    }

                    log::trace!(target: LOG_TARGET_WINIT_INPUT, "key up: 0x{i:X}");
                }
            });

            // Update the scale factor
            if let Some(scale_factor) = input.scale_factor() {
                framework.scale_factor(scale_factor);
            }

            // Resize the window
            if let Some(size) = input.window_resized() {
                if let Err(err) = pixels.resize_surface(size.width, size.height) {
                    log::error!("{err}");
                    *control_flow = ControlFlow::Exit;
                }
                framework.resize(size.width, size.height);
            }

            window.request_redraw();
        }

        // Draw the current frame
        match event {
            Event::RedrawRequested(_) => {
                // send instructions executed since the last update to the debugger
                for instruction in instructions_receiver.try_iter() {
                    debug_gui.instruction_history.push(instruction);
                }
                let chip8 = chip8.lock().unwrap();

                // sync chip8 state to the debugger
                debug_gui.chip8_mode = chip8.mode;
                debug_gui.registers = chip8.registers;
                debug_gui.pc = chip8.pc;
                debug_gui.address_register = chip8.address_register;
                drop(chip8);

                framework.prepare(&window, &mut debug_gui);

                log::trace!(target: LOG_TARGET_RENDERING, "Rendering window");

                let f = framebuffer.lock().unwrap();
                pixels.frame_mut().copy_from_slice(&*f);
                drop(f);
                // Render everything together
                pixels
                    .render_with(|encoder, render_target, context| {
                        // Render the world texture
                        context.scaling_renderer.render(encoder, render_target);

                        // Render egui
                        framework.render(encoder, render_target, context);

                        Ok(())
                    })
                    .unwrap();
            }
            Event::WindowEvent { window_id, event } => {
                framework.handle_event(&event);
            }
            _ => {}
        }
    });

    Ok(())
}

/// Render the CHIP8 vram to the Pixels framebuffer
fn render_vram(vram: &[u8], frame: &mut [u8]) {
    const ALPHA: u8 = 0xFF;
    const ON: [u8; 4] = [0x66, 0x66, 0x99, ALPHA];
    const OFF: [u8; 4] = [0x29, 0x29, 0x3d, ALPHA];

    for vram_y in 0..chip8::DISPLAY_HEIGHT {
        for vram_x in 0..chip8::DISPLAY_WIDTH {
            let color = if vram[chip8::vram_index(vram_x, vram_y).unwrap()] == 1 {
                OFF
            } else {
                ON
            };

            // every vram pixel is scaled up
            for x in 0..DISPLAY_WINDOW_SCALE {
                for y in 0..DISPLAY_WINDOW_SCALE {
                    let frame_x = vram_x as u32 * DISPLAY_WINDOW_SCALE + x;
                    let frame_y = vram_y as u32 * DISPLAY_WINDOW_SCALE + y;

                    let i = (frame_x as usize + WINDOW_WIDTH as usize * frame_y as usize) * 4;
                    frame[i] = color[0];
                    frame[i + 1] = color[1];
                    frame[i + 2] = color[2];
                    frame[i + 3] = color[3];
                }
            }
        }
    }
}

fn wait_for_input() {
    println!("Press enter to continue");
    let stdin = std::io::stdin();
    let mut inp = String::new();
    stdin.read_line(&mut inp).expect("failed to read stdin");
}
