pub mod instructions;

use std::path::Path;

use self::instructions::Instruction;

pub const DISPLAY_WIDTH: u16 = 64;
pub const DISPLAY_HEIGHT: u16 = 32;

/// Initital program counter value and the offset at which the rom is loaded into memory
pub const PC_INIT: usize = 0x200;

pub const DELAY_TIMER_FREQUENCY: f32 = 60.0; // hz;

pub const LOG_TARGET_INPUT: &str = "INPUT";
pub const LOG_TARGET_INSTRUCTIONS: &str = "INSTR";
pub const LOG_TARGET_DRAWING: &str = "DRAW";
pub const LOG_TARGET_TIMER: &str = "TIMER";

/// <https://github.com/mattmikolay/chip-8/wiki/CHIP%E2%80%908-Technical-Reference#fonts>
const FONT: [u8; 80] = [
    0xF0, 0x90, 0x90, 0x90, 0xF0, 0x20, 0x60, 0x20, 0x20, 0x70, 0xF0, 0x10, 0xF0, 0x80, 0xF0, 0xF0,
    0x10, 0xF0, 0x10, 0xF0, 0x90, 0x90, 0xF0, 0x10, 0x10, 0xF0, 0x80, 0xF0, 0x10, 0xF0, 0xF0, 0x80,
    0xF0, 0x90, 0xF0, 0xF0, 0x10, 0x20, 0x40, 0x40, 0xF0, 0x90, 0xF0, 0x90, 0xF0, 0xF0, 0x90, 0xF0,
    0x10, 0xF0, 0xF0, 0x90, 0xF0, 0x90, 0x90, 0xE0, 0x90, 0xE0, 0x90, 0xE0, 0xF0, 0x80, 0x80, 0x80,
    0xF0, 0xE0, 0x90, 0x90, 0x90, 0xE0, 0xF0, 0x80, 0xF0, 0x80, 0xF0, 0xF0, 0x80, 0xF0, 0x80, 0x80,
];

const FONT_START: usize = 0x0;
const FONT_BYTES_PER_CHAR: usize = 5;

#[derive(Default)]
pub struct Keyboard(u16);

impl Keyboard {
    pub fn set_down(&mut self, key: u8) {
        self.0 |= 2_u16.pow(u32::from(key));
    }

    pub fn set_up(&mut self, key: u8) {
        self.0 ^= 2_u16.pow(u32::from(key));
    }

    pub fn is_down(&self, key: u8) -> bool {
        let v = 2_u16.pow(u32::from(key));
        self.0 & v == v
    }

    pub fn reset(&mut self) {
        *self = Keyboard(0);
    }

    pub fn print(&self) {
        let mut s = String::from("[");
        for i in 0..16 {
            s.push_str(format!(" {i:X}: {}", self.is_down(i)).as_str());

            if i < 15 {
                s.push(',');
            }
        }
        s.push_str(" ]");

        log::trace!(target: LOG_TARGET_INPUT, "{s}");
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Mode {
    Running,
    WaitForKey { register: usize },
    Paused,
}

pub struct Chip8 {
    pub memory: [u8; 4096],
    pub registers: [u8; 16],
    pub pc: usize,
    pub address_register: u16,
    pub vram: [u8; DISPLAY_WIDTH as usize * DISPLAY_HEIGHT as usize],
    stack: Vec<usize>,
    pub keyboard: Keyboard,
    pub delay_timer: u8,
    /// indicates whether there was a change to the vram, indicating the screen should be
    /// re-rendered. The rendering application has to set this back to false after rendering,
    /// as this does not happen automatically
    pub redraw: bool,
    pub mode: Mode,
}

impl Chip8 {
    pub fn new() -> Self {
        let mut memory = [0_u8; 4096];

        for (i, data) in FONT.iter().enumerate() {
            memory[FONT_START + i] = *data;
        }

        Chip8 {
            memory,
            registers: [0_u8; 16],
            pc: PC_INIT,
            address_register: 0,
            vram: [0_u8; DISPLAY_WIDTH as usize * DISPLAY_HEIGHT as usize],
            stack: Vec::new(),
            keyboard: Keyboard::default(),
            delay_timer: 0,
            redraw: false,
            mode: Mode::Running,
        }
    }

    pub fn load_rom(&mut self, file_path: impl AsRef<Path>) -> anyhow::Result<()> {
        let rom = std::fs::read(file_path)?;

        let offset = PC_INIT;
        self.memory[offset..(rom.len() + offset)].copy_from_slice(&rom[..]);

        Ok(())
    }

    fn fetch_and_decode_instruction(&mut self) -> anyhow::Result<Instruction> {
        let instruction: u16 =
            u16::from(self.memory[self.pc]) << 8 | u16::from(self.memory[self.pc + 1]);

        self.pc += 2;

        let instr = Instruction::try_from(instruction);

        if let Ok(i) = &instr {
            log::trace!(target: LOG_TARGET_INSTRUCTIONS, "0x{instruction:X}: {:?}", i);
        }

        instr
    }

    fn execute_instruction(&mut self, instruction: Instruction) {
        match instruction {
            Instruction::Clear => {
                self.vram.fill(0);
                self.redraw = true;
            }

            Instruction::JumpToAddress { address } => {
                self.pc = address as usize;
            }
            Instruction::StoreNumberInRegister { number, register } => {
                self.registers[register as usize] = number;
            }
            Instruction::SetAddressRegister { address } => self.address_register = address,
            Instruction::DrawSprite {
                register_x,
                register_y,
                len,
            } => {
                let start_x: u16 = u16::from(self.registers[register_x]);
                let start_y: u16 = u16::from(self.registers[register_y]);

                let start_x = if start_x > 0x3F {
                    start_x % DISPLAY_WIDTH
                } else {
                    start_x
                };
                let start_y = if start_y > 0x1F {
                    start_y % DISPLAY_HEIGHT
                } else {
                    start_y
                };

                log::trace!(target: LOG_TARGET_DRAWING, "drawing {len} bytes at {start_x},{start_y}");

                let mut x = start_x;
                let mut y = start_y;

                let lo = self.address_register as usize;
                let hi = lo + len as usize;
                let sprite = &self.memory[lo..hi];

                assert_eq!(sprite.len(), len as usize);

                self.registers[0xF] = 0x00;

                for row in sprite {
                    for i in (0..8).rev() {
                        let sprite_pixel = u8::from(row & 2_u8.pow(i) == 2_u8.pow(i));

                        if let Some(old_pixel) = get_pixel(&self.vram, x, y) {
                            let new_pixel = old_pixel ^ sprite_pixel;

                            set_pixel(&mut self.vram, x, y, new_pixel == 1);

                            if old_pixel == 1 && new_pixel == 0 {
                                self.registers[0xF] = 0x01;
                            }
                        }

                        x += 1;
                    }

                    y += 1;
                    x = start_x;
                }

                log::trace!(target:LOG_TARGET_DRAWING, "Finished drawing. VF: {}", self.registers[0xF]);
                print_vram(&self.vram);

                self.redraw = true;

                // wait_for_input();
            }
            Instruction::SkipIfRegisterEqTo { register, value } => {
                if self.registers[register as usize] == value {
                    self.pc += 2;
                }
            }
            Instruction::SkipIfRegisterNeqTo { register, value } => {
                if self.registers[register as usize] != value {
                    self.pc += 2;
                }
            }
            Instruction::SkipIfRegistersEq {
                register_x,
                register_y,
            } => {
                if self.registers[register_x] == self.registers[register_y] {
                    self.pc += 2;
                }
            }
            Instruction::AddToRegister { register, value } => {
                self.registers[register as usize] =
                    self.registers[register as usize].wrapping_add(value);
            }
            Instruction::SkipIfRegistersNeq {
                register_x,
                register_y,
            } => {
                if self.registers[register_x] != self.registers[register_y] {
                    self.pc += 2;
                }
            }
            Instruction::ExecuteSubroutine { address } => {
                self.stack.push(self.pc);
                self.pc = address as usize;
            }
            Instruction::Return => {
                let address = self.stack.pop().expect("Can't return when stack is empty");
                self.pc = address;
            }
            Instruction::CopyRegister {
                register_x,
                register_y,
            } => {
                self.registers[register_x] = self.registers[register_y];
            }
            Instruction::OrRegisters {
                register_x,
                register_y,
            } => {
                self.registers[register_x] |= self.registers[register_y];

                // chip 8 quirk (see https://github.com/Timendus/chip8-test-suite/tree/main#the-test)
                self.registers[0xF] = 0;
            }
            Instruction::AndRegisters {
                register_x,
                register_y,
            } => {
                self.registers[register_x] &= self.registers[register_y];

                // chip 8 quirk (see https://github.com/Timendus/chip8-test-suite/tree/main#the-test)
                self.registers[0xF] = 0;
            }
            Instruction::XorRegisters {
                register_x,
                register_y,
            } => {
                self.registers[register_x] ^= self.registers[register_y];

                // chip 8 quirk (see https://github.com/Timendus/chip8-test-suite/tree/main#the-test)
                self.registers[0xF] = 0;
            }
            Instruction::AddRegisters {
                register_x,
                register_y,
            } => {
                let result: u16 =
                    u16::from(self.registers[register_x]) + u16::from(self.registers[register_y]);

                let carry = result > u16::from(u8::MAX);

                #[allow(clippy::cast_possible_truncation)]
                {
                    self.registers[register_x] = result as u8;
                }
                self.registers[0xF] = u8::from(carry);
            }
            Instruction::SubRegisters {
                register_x,
                register_y,
            } => {
                let x = self.registers[register_x];
                let y = self.registers[register_y];
                let result = x - y;

                self.registers[register_x] = result;

                let borrow = y > x;
                self.registers[0xF] = u8::from(!borrow);
            }
            Instruction::SubRegistersOtherWayArround {
                register_x,
                register_y,
            } => {
                let x = self.registers[register_x];
                let y = self.registers[register_y];
                let result = y - x;

                self.registers[register_x] = result;

                let borrow = x > y;
                self.registers[0xF] = u8::from(!borrow);
            }
            Instruction::LeftShiftRegister {
                register_x,
                register_y,
            } => {
                let value = self.registers[register_y];
                let vf_temp = value & 0b1000_0000;

                self.registers[register_x] = value << 1;
                self.registers[0xF] = u8::from(vf_temp == 0b1000_0000);
            }
            Instruction::RightShiftRegister {
                register_x,
                register_y,
            } => {
                let value = self.registers[register_y];
                let vf_temp = value & 0b0000_0001;

                self.registers[register_x] = value >> 1;
                self.registers[0xF] = u8::from(vf_temp == 0b0000_0001);
            }
            Instruction::StoreRegisters { register_x } => {
                for i in 0..=register_x {
                    self.memory[self.address_register as usize + i] = self.registers[i];
                }

                self.address_register += u16::try_from(register_x).unwrap() + 1;
            }
            Instruction::LoadRegisters { register_x } => {
                for i in 0..=register_x {
                    self.registers[i] = self.memory[self.address_register as usize + i];
                }

                self.address_register += u16::try_from(register_x).unwrap() + 1;
            }
            Instruction::BinaryCodedDecimal { register_x } => {
                let value = self.registers[register_x];

                let hundred = value / 100;
                let ten = (value % 100) / 10;
                let one = value % 10;

                self.memory[self.address_register as usize] = hundred;
                self.memory[self.address_register as usize + 1] = ten;
                self.memory[self.address_register as usize + 2] = one;
            }
            Instruction::AddXtoI { register_x } => {
                self.address_register += u16::from(self.registers[register_x]);
            }
            Instruction::SetDelayTimer { register_x } => {
                self.delay_timer = self.registers[register_x];
                log::trace!(target: LOG_TARGET_TIMER, "set delay timer to {}",self.delay_timer);
            }
            Instruction::ReadDelayTimer { register_x } => {
                self.registers[register_x] = self.delay_timer;
            }
            Instruction::SkipIfKey { register_x } => {
                let key = self.registers[register_x];

                log::trace!(target: LOG_TARGET_INPUT, "SkipIfKey: {key:X}");
                self.keyboard.print();

                if self.keyboard.is_down(key) {
                    self.pc += 2;
                }
            }
            Instruction::SkipIfNotKey { register_x } => {
                let key = self.registers[register_x];

                log::trace!(target: LOG_TARGET_INPUT, "SkipIfNotKey: {key:X}");
                self.keyboard.print();

                if !self.keyboard.is_down(key) {
                    self.pc += 2;
                }
            }
            Instruction::WaitForKey { register_x } => {
                self.mode = Mode::WaitForKey {
                    register: register_x,
                };
            }
            Instruction::JumpOffsetV0 { address } => {
                self.pc = (address + u16::from(self.registers[0x00])) as usize;
            }
            Instruction::LoadFontCharacter { register_x } => {
                self.address_register = u16::try_from(FONT_START).unwrap()
                    + (u16::try_from(FONT_BYTES_PER_CHAR).unwrap()
                        * u16::from(self.registers[register_x]));
            }
            Instruction::RandomNumber { register_x, mask } => {
                let r = rand::random::<u8>() & mask;
                self.registers[register_x] = r;
            }
        }
    }

    /// Load and execute the next instruction.
    /// Returns the instruction.
    pub fn step_cycle(&mut self) -> anyhow::Result<Instruction> {
        let instruction = self.fetch_and_decode_instruction()?;

        self.execute_instruction(instruction);

        Ok(instruction)
    }
}

/// Convert x and y coordinates to a linear index
/// Returns [None] when the coordinate is outside the screen bounds
pub fn vram_index(x: u16, y: u16) -> Option<usize> {
    if x >= DISPLAY_WIDTH || y >= DISPLAY_HEIGHT {
        None
    } else {
        Some((DISPLAY_WIDTH * y + x) as usize)
    }
}

/// Set the pixel at the given coordinates
/// Does nothing if the coordinate is outside the screen bounds
fn set_pixel(vram: &mut [u8], x: u16, y: u16, pixel: bool) {
    if let Some(index) = vram_index(x, y) {
        vram[index] = u8::from(pixel);
    }
}

/// Get the pixel color at the given coordinates
/// Returns [None] when the coordinate is outside the screen bounds
fn get_pixel(vram: &[u8], x: u16, y: u16) -> Option<u8> {
    vram_index(x, y).map(|index| vram[index])
}

fn print_vram(vram: &[u8]) {
    let mut s = String::new();

    for y in 0..DISPLAY_HEIGHT {
        for x in 0..DISPLAY_WIDTH {
            if vram[vram_index(x, y).unwrap()] == 1 {
                s.push('□');
            } else {
                s.push('■');
            }
        }
        s.push('\n');
    }

    log::trace!(target:LOG_TARGET_DRAWING, "vram:\n{s}");
}
