#![feature(bigint_helper_methods)]

mod chip8;

use std::time::{Duration, Instant};

use chip8::Chip8;
use clap::Parser;
use log::LevelFilter;
use pixels::{Error, Pixels, SurfaceTexture};
use simple_logger::SimpleLogger;
use winit::{
    dpi::LogicalSize,
    event::{Event, VirtualKeyCode},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use winit_input_helper::WinitInputHelper;

// How many pixel we display per vram pixel
const DISPLAY_WINDOW_SCALE: u32 = 10;
const WINDOW_WIDTH: u32 = chip8::DISPLAY_WIDTH as u32 * 10;
const WINDOW_HEIGHT: u32 = chip8::DISPLAY_HEIGHT as u32 * 10;

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
        .with_module_level(chip8::LOG_TARGET_INSTRUCTIONS, LevelFilter::Info)
        .with_module_level(chip8::LOG_TARGET_DRAWING, LevelFilter::Info)
        .with_module_level(chip8::LOG_TARGET_TIMER, LevelFilter::Info)
        .init()?;

    let args = Args::parse();

    let mut chip8 = Chip8::new();

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

    const TARGET_FREQUENCY: f32 = 600.0; // hz;
    let time_per_instruction: Duration = Duration::from_secs_f32(1.0 / TARGET_FREQUENCY);

    let mut delay_timer_decrease_counter = 0;

    event_loop.run(move |event, _, control_flow| {
        let start_time = Instant::now();

        // Draw the current frame
        if let Event::RedrawRequested(_) = event {
            chip8.step_cycle().unwrap();

            render_vram(&chip8.vram, &mut pixels).unwrap();

            // wait for some time so we can operate at our target frequency
            if start_time.elapsed() < time_per_instruction {
                let time_left = time_per_instruction - start_time.elapsed();
                if !time_left.is_zero() {
                    std::thread::sleep(time_left);
                }
            }

            // our chip interpreter runs at 600hz, so we decrease the 60hz timer every 10 instructions
            delay_timer_decrease_counter += 1;
            if delay_timer_decrease_counter == 1 {
                if chip8.delay_timer > 0 {
                    chip8.delay_timer -= 1;
                }
                delay_timer_decrease_counter = 0;
            }

            window.request_redraw();
        }

        // Handle input events
        if input.update(&event) {
            // Close events
            if input.key_pressed(VirtualKeyCode::Escape) || input.close_requested() {
                *control_flow = ControlFlow::Exit;
                return;
            }

            KEY_BINDINGS.iter().enumerate().for_each(|(i, key)| {
                if input.key_pressed(*key) {
                    chip8.keyboard.set_down(i as u8);
                } else if input.key_released(*key) {
                    chip8.keyboard.set_up(i as u8);
                }
            });

            // Resize the window
            if let Some(size) = input.window_resized() {
                if let Err(err) = pixels.resize_surface(size.width, size.height) {
                    log::error!("{err}");
                    *control_flow = ControlFlow::Exit;
                    return;
                }
            }

            // Update internal state and request a redraw
            window.request_redraw();
        }
    });

    Ok(())
}

/// Render the CHIP8 vram to the Pixels framebuffer
fn render_vram(vram: &[u8], pixels: &mut Pixels) -> Result<(), Error> {
    const ALPHA: u8 = 0xFF;
    const ON: [u8; 4] = [0x66, 0x66, 0x99, ALPHA];
    const OFF: [u8; 4] = [0x29, 0x29, 0x3d, ALPHA];

    let frame = pixels.frame_mut();

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
    pixels.render()
}

fn wait_for_input() {
    println!("Press enter to continue");
    let stdin = std::io::stdin();
    let mut inp = String::new();
    stdin.read_line(&mut inp).expect("failed to read stdin");
}
