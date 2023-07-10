#[derive(Debug)]
pub enum Instruction {
    ///00E0
    Clear,
    ///00EE
    Return,
    ///1NNN
    JumpToAddress { address: u16 },
    ///2NNN
    ExecuteSubroutine { address: u16 },
    ///6XNN
    StoreNumberInRegister { number: u8, register: u8 },
    ///ANNN
    SetAddressRegister { address: u16 },
    ///BNNN
    JumpOffsetV0 { address: u16 },
    ///D8B4
    DrawSprite {
        register_x: usize,
        register_y: usize,
        len: u8,
    },
    ///3XNN
    SkipIfRegisterEqTo { register: u8, value: u8 },
    ///4XNN
    SkipIfRegisterNeqTo { register: u8, value: u8 },
    ///5XY0
    SkipIfRegistersEq {
        register_x: usize,
        register_y: usize,
    },
    ///7XNN
    AddToRegister { register: u8, value: u8 },
    ///8XY0
    CopyRegister {
        register_x: usize,
        register_y: usize,
    },
    ///8XY1
    OrRegisters {
        register_x: usize,
        register_y: usize,
    },
    ///8XY2
    AndRegisters {
        register_x: usize,
        register_y: usize,
    },
    ///8XY3
    XorRegisters {
        register_x: usize,
        register_y: usize,
    },
    ///8XY4
    AddRegisters {
        register_x: usize,
        register_y: usize,
    },
    ///8XY5
    SubRegisters {
        register_x: usize,
        register_y: usize,
    },
    ///8XYE
    LeftShiftRegister {
        register_x: usize,
        register_y: usize,
    },
    ///8XY6
    RightShiftRegister {
        register_x: usize,
        register_y: usize,
    },
    ///8XY7
    SubRegistersOtherWayArround {
        register_x: usize,
        register_y: usize,
    },
    ///9XY0
    SkipIfRegistersNeq {
        register_x: usize,
        register_y: usize,
    },
    ///EX9E
    SkipIfKey { register_x: usize },
    ///EXA1
    SkipIfNotKey { register_x: usize },
    ///FX1E
    AddXtoI { register_x: usize },
    ///FX33
    BinaryCodedDecimal { register_x: usize },
    ///FX15
    SetDelayTimer { register_x: usize },
    ///FX07
    ReadDelayTimer { register_x: usize },
    ///FX0A
    WaitForKey { register_x: usize },
    ///FX55
    StoreRegisters { register_x: usize },
    ///FX65
    LoadRegisters { register_x: usize },
}

impl TryFrom<u16> for Instruction {
    type Error = anyhow::Error;

    fn try_from(value: u16) -> Result<Self, anyhow::Error> {
        let a = ((value & 0xF000) >> 12) as u8;
        let b = ((value & 0x0F00) >> 8) as u8;
        let c = ((value & 0x00F0) >> 4) as u8;
        let d = (value & 0x000F) as u8;

        let x = b as usize;
        let y = c as usize;

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
                register_x: x,
                register_y: y,
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
                register_x: x,
                register_y: y,
            }),
            (0x8, _, _, 0x1) => Ok(Instruction::OrRegisters {
                register_x: x,
                register_y: y,
            }),
            (0x8, _, _, 0x2) => Ok(Instruction::AndRegisters {
                register_x: x,
                register_y: y,
            }),
            (0x8, _, _, 0x3) => Ok(Instruction::XorRegisters {
                register_x: x,
                register_y: y,
            }),
            (0x8, _, _, 0x4) => Ok(Instruction::AddRegisters {
                register_x: x,
                register_y: y,
            }),
            (0x8, _, _, 0x5) => Ok(Instruction::SubRegisters {
                register_x: x,
                register_y: y,
            }),
            (0x8, _, _, 0x6) => Ok(Instruction::RightShiftRegister {
                register_x: x,
                register_y: y,
            }),
            (0x8, _, _, 0x7) => Ok(Instruction::SubRegistersOtherWayArround {
                register_x: x,
                register_y: y,
            }),
            (0x8, _, _, 0xE) => Ok(Instruction::LeftShiftRegister {
                register_x: x,
                register_y: y,
            }),
            (0x9, _, _, 0) => Ok(Instruction::SkipIfRegistersNeq {
                register_x: x,
                register_y: y,
            }),
            (0xA, _, _, _) => Ok(Instruction::SetAddressRegister {
                address: read_address(value),
            }),
            (0xB, _, _, _) => Ok(Instruction::JumpOffsetV0 {
                address: read_address(value),
            }),
            (0xD, _, _, _) => Ok(Instruction::DrawSprite {
                register_x: x,
                register_y: y,
                len: d,
            }),
            (0xE, _, 0x9, 0xE) => Ok(Instruction::SkipIfKey { register_x: x }),
            (0xE, _, 0xA, 0x1) => Ok(Instruction::SkipIfNotKey { register_x: x }),
            (0xF, _, 0x0, 0x7) => Ok(Instruction::ReadDelayTimer { register_x: x }),
            (0xF, _, 0x0, 0xA) => Ok(Instruction::WaitForKey { register_x: x }),
            (0xF, _, 0x1, 0x5) => Ok(Instruction::SetDelayTimer { register_x: x }),
            (0xF, _, 0x1, 0xE) => Ok(Instruction::AddXtoI { register_x: x }),
            (0xF, _, 0x5, 0x5) => Ok(Instruction::StoreRegisters { register_x: x }),
            (0xF, _, 0x6, 0x5) => Ok(Instruction::LoadRegisters { register_x: x }),
            (0xF, _, 0x3, 0x3) => Ok(Instruction::BinaryCodedDecimal { register_x: x }),
            _ => Err(anyhow::anyhow!("unknown instruction 0x{value:X}")),
        }
    }
}

fn read_address(instruction: u16) -> u16 {
    instruction & 0x0FFF
}

fn read_byte_operand(instruction: u16) -> u8 {
    (instruction & 0x00FF) as u8
}
