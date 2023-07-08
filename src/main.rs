#![feature(bigint_helper_methods)]

use std::path::Path;

use pixels::{Error, Pixels, SurfaceTexture};
use winit::{
    dpi::LogicalSize,
    event::{Event, VirtualKeyCode},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use winit_input_helper::WinitInputHelper;

/// Initital program counter value and the offset at which the rom is loaded into memory
const PC_INIT: u16 = 0x200;

const DISPLAY_WIDTH: u16 = 64;
const DISPLAY_HEIGHT: u16 = 32;

// How many pixel we display per vram pixel
const DISPLAY_WINDOW_SCALE: u32 = 10;
const WINDOW_WIDTH: u32 = DISPLAY_WIDTH as u32 * 10;
const WINDOW_HEIGHT: u32 = DISPLAY_HEIGHT as u32 * 10;

fn main() -> anyhow::Result<()> {
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

    let path = "./roms/timendus-test-suite/2-ibm-logo.ch8";
    //let path = "./roms/timendus-test-suite/1-chip8-logo.ch8";

    let mut memory = [0_u8; 4096];
    let mut registers = [0_u8; 16];
    let mut pc: u16 = PC_INIT;
    let mut address_register: u16 = 0;
    let mut vram = [0_u8; DISPLAY_WIDTH as usize * DISPLAY_HEIGHT as usize];

    let mut stack: Vec<u16> = Vec::new();

    load_rom(&mut memory, path)?;

    let mut cycle_counter = 0;

    event_loop.run(move |event, _, control_flow| {
        // Draw the current frame
        if let Event::RedrawRequested(_) = event {
            let instruction = read_instruction(&memory, pc);
            pc += 2; // as long as there are no relative jumps, this should be ok
            execute_instruction(
                instruction,
                &mut memory,
                &mut registers,
                &mut pc,
                &mut address_register,
                &mut vram,
                &mut stack,
            );

            println!("registers: {registers:X?}");

            cycle_counter += 1;
            println!("cycles: {cycle_counter}");

            render_vram(&vram, &mut pixels).unwrap();
        }

        // Handle input events
        if input.update(&event) {
            // Close events
            if input.key_pressed(VirtualKeyCode::Escape) || input.close_requested() {
                *control_flow = ControlFlow::Exit;
                return;
            }

            // Resize the window
            if let Some(size) = input.window_resized() {
                if let Err(err) = pixels.resize_surface(size.width, size.height) {
                    println!("{err}");
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
    const ON: u8 = 0xFF;
    const OFF: u8 = 0x00;

    let frame = pixels.frame_mut();

    for vram_y in 0..DISPLAY_HEIGHT {
        for vram_x in 0..DISPLAY_WIDTH {
            let color = if vram[vram_index(vram_x, vram_y)] == 1 {
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
                    frame[i] = color;
                    frame[i + 1] = color;
                    frame[i + 2] = color;
                    frame[i + 3] = ALPHA;
                }
            }
        }
    }
    pixels.render()
}

fn print_vram(vram: &[u8]) {
    println!("vram:");

    for y in 0..DISPLAY_HEIGHT {
        for x in 0..DISPLAY_WIDTH {
            if vram[vram_index(x, y)] == 1 {
                print!("□");
            } else {
                print!("■");
            }
        }
        println!();
    }
}

#[derive(Debug)]
enum Instruction {
    //00E0
    Clear,
    //00EE
    Return,
    //1NNN
    JumpToAddress {
        address: u16,
    },
    //2NNN
    ExecuteSubroutine {
        address: u16,
    },
    //6XNN
    StoreNumberInRegister {
        number: u8,
        register: u8,
    },
    //ANNN
    SetAddressRegister {
        address: u16,
    },
    //D8B4
    DrawSprite {
        register_x: u8,
        register_y: u8,
        len: u8,
    },
    //3XNN
    SkipIfRegisterEqTo {
        register: u8,
        value: u8,
    },
    //4XNN
    SkipIfRegisterNeqTo {
        register: u8,
        value: u8,
    },
    //5XY0
    SkipIfRegistersEq {
        register_x: u8,
        register_y: u8,
    },
    //7XNN
    AddToRegister {
        register: u8,
        value: u8,
    },
    //8XY0
    CopyRegister {
        register_x: u8,
        register_y: u8,
    },
    //8XY1
    OrRegisters {
        register_x: u8,
        register_y: u8,
    },
    //8XY2
    AndRegisters {
        register_x: u8,
        register_y: u8,
    },
    //8XY3
    XorRegisters {
        register_x: u8,
        register_y: u8,
    },
    //8XY4
    AddRegisters {
        register_x: u8,
        register_y: u8,
    },
    //8XY5
    SubRegisters {
        register_x: u8,
        register_y: u8,
    },
    //8XYE
    LeftShiftRegister {
        register_x: u8,
        register_y: u8,
    },
    //8XY6
    RightShiftRegister {
        register_x: u8,
        register_y: u8,
    },
    //9XY0
    SkipIfRegistersNeq {
        register_x: u8,
        register_y: u8,
    },
    //FX33
    BinaryCodedDecimal {
        register_x: u8,
    },
    //FX55
    StoreRegisters {
        register_x: u8,
    },
    //FX65
    LoadRegisters {
        register_x: u8,
    },
}

fn wait_for_input() {
    println!("Press enter to continue");
    let stdin = std::io::stdin();
    let mut inp = String::new();
    stdin.read_line(&mut inp).expect("failed to read stdin");
}

fn execute_instruction(
    instruction: Instruction,
    memory: &mut [u8],
    registers: &mut [u8],
    pc: &mut u16,
    address_register: &mut u16,
    vram: &mut [u8],
    stack: &mut Vec<u16>,
) {
    match instruction {
        Instruction::Clear => {
            for pixel in vram {
                *pixel = 0;
            }
        }

        Instruction::JumpToAddress { address } => {
            *pc = address;
        }
        Instruction::StoreNumberInRegister { number, register } => {
            registers[register as usize] = number;
        }
        Instruction::SetAddressRegister { address } => *address_register = address,
        Instruction::DrawSprite {
            register_x,
            register_y,
            len,
        } => {
            let start_x: u16 = registers[register_x as usize] as u16;
            let start_y: u16 = registers[register_y as usize] as u16;

            println!("drawing {len} bytes at {start_x},{start_y}");

            let mut x = start_x;
            let mut y = start_y;

            let lo = *address_register as usize;
            let hi = lo + len as usize;
            let sprite = &memory[lo..hi];

            assert_eq!(sprite.len(), len as usize);

            registers[0xF] = 0x00;

            for row in sprite {
                for i in (0..8).rev() {
                    let sprite_pixel = if row & 2_u8.pow(i) == 2_u8.pow(i) {
                        1
                    } else {
                        0
                    };
                    let old_pixel = get_pixel(vram, x, y);

                    let mut color = sprite_pixel;
                    if old_pixel == 1 && sprite_pixel == 1 {
                        registers[0xF] = 0x01;
                        color = 0;
                    }
                    set_pixel(vram, x, y, color == 1);
                    x += 1;
                }

                y += 1;
                x = start_x;
            }

            println!("Finished drawing. VF: {}", registers[0xF]);
            print_vram(vram);

            // wait_for_input();
        }
        Instruction::SkipIfRegisterEqTo { register, value } => {
            if registers[register as usize] == value {
                *pc += 2;
            }
        }
        Instruction::SkipIfRegisterNeqTo { register, value } => {
            if registers[register as usize] != value {
                *pc += 2;
            }
        }
        Instruction::SkipIfRegistersEq {
            register_x,
            register_y,
        } => {
            if registers[register_x as usize] == registers[register_y as usize] {
                *pc += 2;
            }
        }
        Instruction::AddToRegister { register, value } => {
            (registers[register as usize], _) = registers[register as usize].overflowing_add(value);
        }
        Instruction::SkipIfRegistersNeq {
            register_x,
            register_y,
        } => {
            if registers[register_x as usize] != registers[register_y as usize] {
                *pc += 2;
            }
        }
        Instruction::ExecuteSubroutine { address } => {
            stack.push(*pc);
            *pc = address;
        }
        Instruction::Return => {
            let address = stack.pop().expect("Can't return when stack is empty");
            *pc = address;
        }
        Instruction::CopyRegister {
            register_x,
            register_y,
        } => {
            registers[register_x as usize] = registers[register_y as usize];
        }
        Instruction::OrRegisters {
            register_x,
            register_y,
        } => {
            registers[register_x as usize] |= registers[register_y as usize];
        }
        Instruction::AndRegisters {
            register_x,
            register_y,
        } => {
            registers[register_x as usize] &= registers[register_y as usize];
        }
        Instruction::XorRegisters {
            register_x,
            register_y,
        } => {
            registers[register_x as usize] ^= registers[register_y as usize];
        }
        Instruction::AddRegisters {
            register_x,
            register_y,
        } => {
            let (result, carry) = u8::carrying_add(
                registers[register_x as usize],
                registers[register_y as usize],
                true,
            );

            registers[register_x as usize] = result;

            registers[0xF] = if carry { 0x01 } else { 0x00 };
        }
        Instruction::SubRegisters {
            register_x,
            register_y,
        } => {
            let (result, borrow) = u8::borrowing_sub(
                registers[register_x as usize],
                registers[register_y as usize],
                true,
            );

            registers[register_x as usize] = result;

            registers[0xF] = if borrow { 0x00 } else { 0x01 };
        }
        Instruction::LeftShiftRegister {
            register_x,
            register_y,
        } => {
            let value = registers[register_y as usize];
            registers[0xF] = value & 0b10000000;

            registers[register_x as usize] = value << 1;
        }
        Instruction::RightShiftRegister {
            register_x,
            register_y,
        } => {
            let value = registers[register_y as usize];
            registers[0xF] = value & 0b00000001;

            registers[register_x as usize] = value >> 1;
        }
        Instruction::StoreRegisters { register_x } => {
            for i in 0..=register_x as usize {
                memory[*address_register as usize + i] = registers[i]
            }

            *address_register += register_x as u16 + 1;
        }
        Instruction::LoadRegisters { register_x } => {
            for i in 0..=register_x as usize {
                registers[i] = memory[*address_register as usize + i];
            }

            *address_register += register_x as u16 + 1;
        }
        Instruction::BinaryCodedDecimal { register_x } => {
            let value = registers[register_x as usize];

            let hundred = value / 100;
            let ten = (value % 100) / 10;
            let one = value % 10;

            memory[*address_register as usize] = hundred;
            memory[*address_register as usize + 1] = ten;
            memory[*address_register as usize + 2] = one;
        }
    }
}

fn vram_index(x: u16, y: u16) -> usize {
    (DISPLAY_WIDTH * y + x) as usize
}

fn set_pixel(vram: &mut [u8], x: u16, y: u16, pixel: bool) {
    let x = x % DISPLAY_WIDTH;
    let y = y % DISPLAY_HEIGHT;

    let index = vram_index(x, y);
    vram[index] = if pixel { 1 } else { 0 };
}

fn get_pixel(vram: &[u8], x: u16, y: u16) -> u8 {
    let x = x % DISPLAY_WIDTH;
    let y = y % DISPLAY_HEIGHT;

    let index = vram_index(x, y);

    vram[index]
}

fn read_instruction(memory: &[u8], pc: u16) -> Instruction {
    let pc = pc as usize;

    let instruction: u16 = (memory[pc] as u16) << 8 | memory[pc + 1] as u16;

    let a = ((instruction & 0xF000) >> 12) as u8;
    let b = ((instruction & 0x0F00) >> 8) as u8;
    let c = ((instruction & 0x00F0) >> 4) as u8;
    let d = (instruction & 0x000F) as u8;

    println!("instruction: 0x{instruction:X}");

    match (a, b, c, d) {
        (0x0, 0x0, 0xE, 0x0) => Instruction::Clear,
        (0x0, 0x0, 0xE, 0xE) => Instruction::Return,
        (0x1, _, _, _) => Instruction::JumpToAddress {
            address: read_address(instruction),
        },
        (0x2, _, _, _) => Instruction::ExecuteSubroutine {
            address: read_address(instruction),
        },
        (0x3, _, _, _) => Instruction::SkipIfRegisterEqTo {
            register: b,
            value: read_byte_operand(instruction),
        },
        (0x4, _, _, _) => Instruction::SkipIfRegisterNeqTo {
            register: b,
            value: read_byte_operand(instruction),
        },
        (0x5, _, _, 0) => Instruction::SkipIfRegistersEq {
            register_x: b,
            register_y: c,
        },
        (0x6, _, _, _) => Instruction::StoreNumberInRegister {
            number: read_byte_operand(instruction),
            register: b,
        },
        (0x7, _, _, _) => Instruction::AddToRegister {
            register: b,
            value: read_byte_operand(instruction),
        },
        (0x8, _, _, 0x0) => Instruction::CopyRegister {
            register_x: b,
            register_y: c,
        },
        (0x8, _, _, 0x1) => Instruction::OrRegisters {
            register_x: b,
            register_y: c,
        },
        (0x8, _, _, 0x2) => Instruction::AndRegisters {
            register_x: b,
            register_y: c,
        },
        (0x8, _, _, 0x3) => Instruction::XorRegisters {
            register_x: b,
            register_y: c,
        },
        (0x8, _, _, 0x4) => Instruction::AddRegisters {
            register_x: b,
            register_y: c,
        },
        (0x8, _, _, 0x5) => Instruction::SubRegisters {
            register_x: b,
            register_y: c,
        },
        (0x8, _, _, 0x6) => Instruction::RightShiftRegister {
            register_x: b,
            register_y: c,
        },
        (0x8, _, _, 0xE) => Instruction::LeftShiftRegister {
            register_x: b,
            register_y: c,
        },
        (0x9, _, _, 0) => Instruction::SkipIfRegistersNeq {
            register_x: b,
            register_y: c,
        },
        (0xA, _, _, _) => Instruction::SetAddressRegister {
            address: read_address(instruction),
        },
        (0xD, _, _, _) => Instruction::DrawSprite {
            register_x: b,
            register_y: c,
            len: d,
        },
        (0xF, _, 0x5, 0x5) => Instruction::StoreRegisters { register_x: b },
        (0xF, _, 0x6, 0x5) => Instruction::LoadRegisters { register_x: b },
        (0xF, _, 0x3, 0x3) => Instruction::BinaryCodedDecimal { register_x: b },
        _ => todo!("unknown instruction 0x{instruction:X}"),
    }
}

fn read_address(instruction: u16) -> u16 {
    instruction & 0x0FFF
}

fn read_byte_operand(instruction: u16) -> u8 {
    (instruction & 0x00FF) as u8
}

fn load_rom(memory: &mut [u8; 4096], file_path: impl AsRef<Path>) -> anyhow::Result<()> {
    let rom = std::fs::read(file_path)?;

    let offset = PC_INIT as usize;
    memory[offset..(rom.len() + offset)].copy_from_slice(&rom[..]);

    Ok(())
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn test_nnn() {
        assert_eq!(read_address(0x0123), 0x123);
    }

    #[test]
    fn test_vram_index() {
        let x = 30;
        let y = 10;

        assert_eq!(vram_index(x, y), 670)
    }
}
