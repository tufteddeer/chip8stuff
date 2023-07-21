#![warn(clippy::pedantic)]
#![warn(clippy::style)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::many_single_char_names)]
#![feature(bigint_helper_methods)]

mod chip8;
mod debug_gui;

use std::{
    fs::{self, File},
    io::{Read, Seek},
    os::unix::prelude::FileExt,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use chip8::Chip8;
use chrono::Utc;
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

const EMBEDDED_ROM_TRAILER_MAGIC: u8 = 0xC8;
const EMBEDDED_ROM_TRAILER_LEN: usize = 3;

const KEY_BINDINGS: [VirtualKeyCode; 16] = [
    VirtualKeyCode::X,    // 0x0
    VirtualKeyCode::Key1, // 0x1
    VirtualKeyCode::Key2, // 0x2
    VirtualKeyCode::Key3, // 0x3
    VirtualKeyCode::Q,    // 0x4
    VirtualKeyCode::W,    // 0x5
    VirtualKeyCode::E,    // 0x6
    VirtualKeyCode::A,    // 0x7
    VirtualKeyCode::S,    // 0x8
    VirtualKeyCode::D,    // 0x9
    VirtualKeyCode::Y,    // 0xA
    VirtualKeyCode::C,    // 0xB
    VirtualKeyCode::Key4, // 0xC
    VirtualKeyCode::R,    // 0xD
    VirtualKeyCode::F,    // 0xE
    VirtualKeyCode::V,    // 0xF
];

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    rom_file: Option<String>,
    /// Start interpreter in paused mode
    #[arg(short, long)]
    paused: bool,
    /// Enable trace and debug logs
    #[arg(short, long)]
    verbose: bool,
    /// Create a new standalone executable that includes a copy of the given ROM file
    #[arg(long)]
    embed: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let log_level = if args.verbose {
        LevelFilter::Trace
    } else {
        LevelFilter::Info
    };

    SimpleLogger::new()
        // dependencies
        .with_module_level("wgpu_core", LevelFilter::Warn)
        .with_module_level("mio", LevelFilter::Warn)
        .with_module_level("winit", LevelFilter::Warn)
        .with_module_level("wgpu_hal", LevelFilter::Warn)
        .with_module_level("naga", LevelFilter::Warn)
        // chip8 log targets
        .with_module_level(chip8::LOG_TARGET_INPUT, log_level)
        .with_module_level(chip8::LOG_TARGET_INSTRUCTIONS, log_level)
        .with_module_level(chip8::LOG_TARGET_DRAWING, log_level)
        .with_module_level(chip8::LOG_TARGET_TIMER, log_level)
        // interpreter log targets
        .with_module_level(LOG_TARGET_RENDERING, log_level)
        .with_module_level(LOG_TARGET_TIMING, log_level)
        .with_module_level(LOG_TARGET_WINIT_INPUT, log_level)
        .init()?;

    if let Some(rom_file) = args.embed {
        log::info!("Embedding {rom_file}");

        let rom = std::fs::read(&rom_file)?;
        log::info!("Got {} bytes of ROM", rom.len());

        let exe_path = std::env::current_exe()?;

        let p = PathBuf::from(rom_file);
        let rom_name = p.file_name().unwrap().to_str().unwrap().clone();
        let new_exe_name = format!("chip8stuff_{rom_name}_player");

        fs::copy(exe_path, &new_exe_name)?;
        let exe = std::fs::OpenOptions::new()
            .append(true)
            .open(&new_exe_name)?;
        let file_len = fs::metadata(&new_exe_name)?.len();

        let rom_start = file_len - 1;
        log::info!("Writing rom at 0x{:X}", rom_start);

        exe.write_all_at(&rom, rom_start)?;
        log::info!("Done");
        log::info!("Writing trailer ");

        exe.write_all_at(
            &[
                EMBEDDED_ROM_TRAILER_MAGIC,
                ((rom.len() | 0xF) >> 8) as u8,
                rom.len() as u8,
            ],
            file_len + rom.len() as u64,
        )?;

        log::info!("Done");

        log::info!("Saved standalone player as {new_exe_name}");

        return Ok(());
    }

    let mut chip8 = Chip8::new();

    if args.paused {
        chip8.mode = Mode::Paused;
    }

    // If a file path is passed, load the rom
    if let Some(rom_file) = args.rom_file {
        chip8.load_rom(&rom_file)?;
        log::info!("Loaded rom file {}", rom_file);
    } else {
        // if there is no rom to load, check if there is a rom embedded in the executable
        load_embedded_rom(&mut chip8)?;
    }

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
    let (dump_memory_sender, dump_memory_receiver) = std::sync::mpsc::channel::<()>();

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

            if dump_memory_receiver.try_recv().is_ok() {
                let p = format!("memory_dump_{}.bin", Utc::now());

                std::fs::write(&p, chip8.memory).unwrap();
                log::info!("Saved memory to {p}");
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
                    log::trace!(target: LOG_TARGET_RENDERING, "rendering into framebuffer");
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
        show_instruction_history_window: false,
        pc: c.pc,
        address_register: c.address_register,
        dump_memory_sender,
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
                    chip8.keyboard.set_down(u8::try_from(i).unwrap());

                    log::trace!(target: LOG_TARGET_WINIT_INPUT, "key down: 0x{i:X}");
                } else if input.key_released(*key) {
                    chip8.keyboard.set_up(u8::try_from(i).unwrap());

                    if let Mode::WaitForKey { register } = chip8.mode {
                        chip8.registers[register] = u8::try_from(i).unwrap();
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
            Event::WindowEvent {
                window_id: _,
                event,
            } => {
                framework.handle_event(&event);
            }
            _ => {}
        }
    });
}

/// Check if there is a ROM embedded in the executable and load it into CHIP8 memory
fn load_embedded_rom(chip8: &mut Chip8) -> anyhow::Result<()> {
    let exe_path = std::env::current_exe()?;

    let mut exe = File::open(exe_path)?;

    let rom_len = get_embedded_rom_length(&mut exe);

    if let Err(e) = rom_len {
        log::error!("No ROM file passed and no embedded ROM. Use --help for usage");
        return Err(e);
    }

    let rom_len = rom_len.unwrap();

    log::info!("Loading {rom_len} bytes ROM included in this binary");

    let exe_path = std::env::current_exe()?;

    let meta = fs::metadata(exe_path)?;

    exe.seek(std::io::SeekFrom::Start(0))?;
    let mut exe_file = Vec::new();
    exe.read_to_end(&mut exe_file)?;

    let rom_start = usize::try_from(meta.len())? - EMBEDDED_ROM_TRAILER_LEN - (rom_len);

    log::info!("Loading rom from {rom_start:X}");

    chip8.memory[chip8::PC_INIT..(rom_len as usize + chip8::PC_INIT)]
        .copy_from_slice(&exe_file[rom_start..(rom_len as usize + rom_start)]);

    Ok(())
}

/// checks for the embedded rom trailer and reads the length, returning Err when there is no trailer
fn get_embedded_rom_length(exe: &mut File) -> anyhow::Result<usize> {
    exe.seek(std::io::SeekFrom::End(-3))?;

    let mut buf = [0_u8; 3];
    exe.read_exact(&mut buf)?;

    if buf[0] != EMBEDDED_ROM_TRAILER_MAGIC {
        return Err(anyhow::anyhow!("No ROM included in this binary"));
    }

    let rom_len = (u16::from(buf[1]) << 8) | u16::from(buf[2]);

    Ok(rom_len.into())
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
                    let frame_x = u32::from(vram_x) * DISPLAY_WINDOW_SCALE + x;
                    let frame_y = u32::from(vram_y) * DISPLAY_WINDOW_SCALE + y;

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
