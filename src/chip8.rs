use std::{fmt::Display, path::Path};

pub const DISPLAY_WIDTH: u16 = 64;
pub const DISPLAY_HEIGHT: u16 = 32;

/// Initital program counter value and the offset at which the rom is loaded into memory
const PC_INIT: usize = 0x200;

#[derive(Default)]
pub struct Keyboard(u16);

impl Keyboard {
    pub fn set_down(&mut self, key: u16) {
        self.0 = self.0 | key
    }

    pub fn set_up(&mut self, key: u16) {
        self.0 = self.0 ^ key
    }

    pub fn is_down(&self, key: u16) -> bool {
        self.0 & key == key
    }

    pub fn reset(&mut self) {
        *self = Keyboard(0);
    }

    pub fn print(&self) {
        print!("[");
        for i in 0..16 {
            let key = 2_u16.pow(i);
            print!(" {i:X}: {}", self.is_down(key));

            if i < 15 {
                print!(",")
            }
        }
        println!(" ]");
    }
}

mod test {
    use super::Keyboard;

    #[test]
    fn test_keyboard() {
        let mut kb = Keyboard::default();

        assert_eq!(kb.is_down(0xA), false);
        kb.set_down(0xA);
        assert_eq!(kb.is_down(0xA), true);
        kb.set_up(0xA);
        assert_eq!(kb.is_down(0xA), false);
    }
}

pub struct Chip8 {
    memory: [u8; 4096],
    registers: [u8; 16],
    pc: usize,
    address_register: u16,
    pub vram: [u8; DISPLAY_WIDTH as usize * DISPLAY_HEIGHT as usize],
    stack: Vec<usize>,
    pub keyboard: Keyboard,
}

impl Chip8 {
    pub fn new() -> Self {
        Chip8 {
            memory: [0_u8; 4096],
            registers: [0_u8; 16],
            pc: PC_INIT,
            address_register: 0,
            vram: [0_u8; DISPLAY_WIDTH as usize * DISPLAY_HEIGHT as usize],
            stack: Vec::new(),
            keyboard: Keyboard::default(),
        }
    }

    pub fn load_rom(&mut self, file_path: impl AsRef<Path>) -> anyhow::Result<()> {
        let rom = std::fs::read(file_path)?;

        let offset = PC_INIT;
        self.memory[offset..(rom.len() + offset)].copy_from_slice(&rom[..]);

        Ok(())
    }

    fn fetch_and_decode_instruction(&mut self) -> anyhow::Result<Instruction> {
        let instruction: u16 = (self.memory[self.pc] as u16) << 8 | self.memory[self.pc + 1] as u16;

        self.pc += 2;

        Instruction::try_from(instruction)
    }

    pub fn step_cycle(&mut self) -> anyhow::Result<()> {
        let instruction = self.fetch_and_decode_instruction()?;

        instruction.execute_instruction(
            &mut self.memory,
            &mut self.registers,
            &mut self.pc,
            &mut self.address_register,
            &mut self.vram,
            &mut self.stack,
        );

        Ok(())
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
    //8XY7
    SubRegistersOtherWayArround {
        register_x: u8,
        register_y: u8,
    },
    //9XY0
    SkipIfRegistersNeq {
        register_x: u8,
        register_y: u8,
    },
    //FX1E
    AddXtoI {
        register_x: u8,
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

impl TryFrom<u16> for Instruction {
    type Error = anyhow::Error;

    fn try_from(value: u16) -> Result<Self, anyhow::Error> {
        let a = ((value & 0xF000) >> 12) as u8;
        let b = ((value & 0x0F00) >> 8) as u8;
        let c = ((value & 0x00F0) >> 4) as u8;
        let d = (value & 0x000F) as u8;

        println!("instruction: 0x{value:X}");

        match (a, b, c, d) {
            (0x0, 0x0, 0xE, 0x0) => Ok(Instruction::Clear),
            (0x0, 0x0, 0xE, 0xE) => Ok(Instruction::Return),
            (0x1, _, _, _) => Ok(Instruction::JumpToAddress {
                address: read_address(value),
            }),
            (0x2, _, _, _) => Ok(Instruction::ExecuteSubroutine {
                address: read_address(value),
            }),
            (0x3, _, _, _) => Ok(Instruction::SkipIfRegisterEqTo {
                register: b,
                value: read_byte_operand(value),
            }),
            (0x4, _, _, _) => Ok(Instruction::SkipIfRegisterNeqTo {
                register: b,
                value: read_byte_operand(value),
            }),
            (0x5, _, _, 0) => Ok(Instruction::SkipIfRegistersEq {
                register_x: b,
                register_y: c,
            }),
            (0x6, _, _, _) => Ok(Instruction::StoreNumberInRegister {
                number: read_byte_operand(value),
                register: b,
            }),
            (0x7, _, _, _) => Ok(Instruction::AddToRegister {
                register: b,
                value: read_byte_operand(value),
            }),
            (0x8, _, _, 0x0) => Ok(Instruction::CopyRegister {
                register_x: b,
                register_y: c,
            }),
            (0x8, _, _, 0x1) => Ok(Instruction::OrRegisters {
                register_x: b,
                register_y: c,
            }),
            (0x8, _, _, 0x2) => Ok(Instruction::AndRegisters {
                register_x: b,
                register_y: c,
            }),
            (0x8, _, _, 0x3) => Ok(Instruction::XorRegisters {
                register_x: b,
                register_y: c,
            }),
            (0x8, _, _, 0x4) => Ok(Instruction::AddRegisters {
                register_x: b,
                register_y: c,
            }),
            (0x8, _, _, 0x5) => Ok(Instruction::SubRegisters {
                register_x: b,
                register_y: c,
            }),
            (0x8, _, _, 0x6) => Ok(Instruction::RightShiftRegister {
                register_x: b,
                register_y: c,
            }),
            (0x8, _, _, 0x7) => Ok(Instruction::SubRegistersOtherWayArround {
                register_x: b,
                register_y: c,
            }),
            (0x8, _, _, 0xE) => Ok(Instruction::LeftShiftRegister {
                register_x: b,
                register_y: c,
            }),
            (0x9, _, _, 0) => Ok(Instruction::SkipIfRegistersNeq {
                register_x: b,
                register_y: c,
            }),
            (0xA, _, _, _) => Ok(Instruction::SetAddressRegister {
                address: read_address(value),
            }),
            (0xD, _, _, _) => Ok(Instruction::DrawSprite {
                register_x: b,
                register_y: c,
                len: d,
            }),
            (0xF, _, 0x1, 0xE) => Ok(Instruction::AddXtoI { register_x: b }),
            (0xF, _, 0x5, 0x5) => Ok(Instruction::StoreRegisters { register_x: b }),
            (0xF, _, 0x6, 0x5) => Ok(Instruction::LoadRegisters { register_x: b }),
            (0xF, _, 0x3, 0x3) => Ok(Instruction::BinaryCodedDecimal { register_x: b }),
            _ => Err(anyhow::anyhow!("unknown instruction 0x{value:X}")),
        }
    }
}

impl Instruction {
    fn execute_instruction(
        self,
        memory: &mut [u8],
        registers: &mut [u8],
        pc: &mut usize,
        address_register: &mut u16,
        vram: &mut [u8],
        stack: &mut Vec<usize>,
    ) {
        match self {
            Instruction::Clear => {
                vram.fill(0);
            }

            Instruction::JumpToAddress { address } => {
                *pc = address as usize;
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

                        if let Some(old_pixel) = get_pixel(vram, x, y) {
                            let mut color = sprite_pixel;
                            if old_pixel == 1 && sprite_pixel == 1 {
                                registers[0xF] = 0x01;
                                color = 0;
                            }
                            set_pixel(vram, x, y, color == 1);
                        }

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
                registers[register as usize] = registers[register as usize].wrapping_add(value);
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
                *pc = address as usize;
            }
            Instruction::Return => {
                let address = stack.pop().expect("Can't return when stack is empty");
                *pc = address as usize;
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
                let result: u16 =
                    registers[register_x as usize] as u16 + registers[register_y as usize] as u16;

                let carry = result > u8::MAX as u16;

                registers[register_x as usize] = result as u8;
                registers[0xF] = if carry { 0x01 } else { 0x00 };
            }
            Instruction::SubRegisters {
                register_x,
                register_y,
            } => {
                let x = registers[register_x as usize];
                let y = registers[register_y as usize];
                let result = x - y;

                registers[register_x as usize] = result;

                let borrow = y > x;
                registers[0xF] = if borrow { 0x00 } else { 0x01 };
            }
            Instruction::SubRegistersOtherWayArround {
                register_x,
                register_y,
            } => {
                let x = registers[register_x as usize];
                let y = registers[register_y as usize];
                let result = y - x;

                registers[register_x as usize] = result;

                let borrow = x > y;
                registers[0xF] = if borrow { 0x00 } else { 0x01 };
            }
            Instruction::LeftShiftRegister {
                register_x,
                register_y,
            } => {
                let value = registers[register_y as usize];
                let vf_temp = value & 0b10000000;

                registers[register_x as usize] = value << 1;
                registers[0xF] = if vf_temp == 0b10000000 { 1 } else { 0 };
            }
            Instruction::RightShiftRegister {
                register_x,
                register_y,
            } => {
                let value = registers[register_y as usize];
                let vf_temp = value & 0b00000001;

                registers[register_x as usize] = value >> 1;
                registers[0xF] = if vf_temp == 0b00000001 { 1 } else { 0 };
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
            Instruction::AddXtoI { register_x } => {
                *address_register += registers[register_x as usize] as u16;
            }
        }
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
        vram[index] = if pixel { 1 } else { 0 };
    }
}

/// Get the pixel color at the given coordinates
/// Returns [None] when the coordinate is outside the screen bounds
fn get_pixel(vram: &[u8], x: u16, y: u16) -> Option<u8> {
    vram_index(x, y).map(|index| vram[index])
}

fn read_address(instruction: u16) -> u16 {
    instruction & 0x0FFF
}

fn read_byte_operand(instruction: u16) -> u8 {
    (instruction & 0x00FF) as u8
}

fn print_vram(vram: &[u8]) {
    println!("vram:");

    for y in 0..DISPLAY_HEIGHT {
        for x in 0..DISPLAY_WIDTH {
            if vram[vram_index(x, y).unwrap()] == 1 {
                print!("□");
            } else {
                print!("■");
            }
        }
        println!();
    }
}
