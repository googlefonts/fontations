use super::HintErrorKind;
use core::ops::Range;

/// Type alias for a TrueType opcode.
pub type Opcode = u8;

/// Describes the type of bytecode.
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
#[repr(u8)]
pub enum Program {
    /// Program that initializes the function and instruction tables. Stored
    /// in the `fpgm` table.
    #[default]
    Font = 0,
    /// Program that initializes CVT and storage based on font size and other
    /// parameters. Stored in the `prep` table.
    ControlValue = 1,
    /// Glyph specified program. Stored in the `glyf` table.
    Glyph = 2,
}

/// Code range and properties for a function or instruction definition.
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
#[repr(C)]
pub struct CodeDefinition {
    start: u32,
    end: u32,
    /// For an instruction definition, the assigned opcode.
    opcode: i16,
    /// Program that contains the definition.
    program: u8,
    is_active: u8,
}

impl CodeDefinition {
    pub fn new(program: Program, range: Range<usize>, opcode: Option<u8>) -> Self {
        Self {
            program: program as u8,
            // Table sizes are specified in u32 so valid ranges will
            // always fit.
            start: range.start as u32,
            end: range.end as u32,
            opcode: opcode.map(|x| x as i16).unwrap_or(-1),
            is_active: 1,
        }
    }

    /// Returns true if this is a function definition.
    pub fn is_function(&self) -> bool {
        self.opcode == -1
    }

    /// Returns true if this is an instruction definition.
    pub fn is_instruction(&self) -> bool {
        self.opcode != -1
    }

    /// Returns the program that contains this definition.
    pub fn program(&self) -> Program {
        match self.program {
            0 => Program::Font,
            1 => Program::ControlValue,
            2 => Program::Glyph,
            _ => Program::Font,
        }
    }

    /// Returns the byte range of the code for this definition in the source
    /// program.
    pub fn range(&self) -> Range<usize> {
        self.start as usize..self.end as usize
    }

    /// Returns true if this definition entry has been defined by a program.
    pub fn is_active(&self) -> bool {
        self.is_active != 0
    }

    /// For an instruction definition, returns the assigned opcode.
    pub fn opcode(&self) -> Option<u8> {
        self.opcode.try_into().ok()
    }
}

pub enum CodeDefinitionSlice<'a> {
    Ref(&'a [CodeDefinition]),
    Mut(&'a mut [CodeDefinition]),
}

impl<'a> CodeDefinitionSlice<'a> {
    pub fn len(&self) -> usize {
        match self {
            Self::Ref(defs) => defs.len(),
            Self::Mut(defs) => defs.len(),
        }
    }

    pub fn get(&self, index: usize) -> Result<CodeDefinition, HintErrorKind> {
        match self {
            Self::Ref(defs) => defs.get(index).copied(),
            Self::Mut(defs) => defs.get(index).copied(),
        }
        .ok_or(HintErrorKind::InvalidDefintionIndex(index))
    }

    pub fn set(&mut self, index: usize, value: CodeDefinition) -> Result<(), HintErrorKind> {
        match self {
            Self::Mut(defs) => {
                *defs
                    .get_mut(index)
                    .ok_or(HintErrorKind::InvalidDefintionIndex(index))? = value
            }
            _ => return Err(HintErrorKind::DefinitionInGlyphProgram),
        }
        Ok(())
    }

    pub fn reset(&mut self) {
        if let Self::Mut(defs) = self {
            defs.fill(Default::default())
        }
    }
}

/// Decoded TrueType instruction.
#[derive(Copy, Clone, Debug)]
pub struct Instruction<'a> {
    /// Program containing the instruction.
    pub program: Program,
    /// Raw opcode value.
    pub opcode: Opcode,
    /// Arguments to the instruction.
    pub arguments: Arguments<'a>,
    /// Program counter -- offset into the bytecode where this
    /// instruction was decoded.
    pub pc: usize,
}

impl<'a> Instruction<'a> {
    /// Returns the name of the instruction.
    pub fn name(&self) -> &'static str {
        NAME_TABLE
            .get(self.opcode as usize)
            .copied()
            .unwrap_or("??")
    }
}

/// Sequence of arguments for an instruction.
#[derive(Copy, Clone, Default, Debug)]
pub struct Arguments<'a> {
    raw: &'a [u8],
    is_words: bool,
    len: u8,
}

impl<'a> Arguments<'a> {
    /// Returns the number of arguments in the list.
    pub fn len(&self) -> usize {
        self.len as usize
    }

    /// Returns true if the argument list is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns an iterator over the argument values.
    pub fn values(&self) -> impl Iterator<Item = i32> + 'a + Clone {
        let bytes = if self.is_words { &[] } else { self.raw };
        let words = if self.is_words { self.raw } else { &[] };
        bytes
            .iter()
            .map(|byte| *byte as u32 as i32)
            .chain(words.chunks_exact(2).map(|chunk| {
                let word = ((chunk[0] as u16) << 8) | chunk[1] as u16;
                word as i16 as i32
            }))
    }
}

/// Decoder for TrueType bytecode.
#[derive(Copy, Clone)]
pub struct Decoder<'a> {
    /// The type of program.
    pub program: Program,
    /// The bytecode for the program.
    pub bytecode: &'a [u8],
    /// The "program counter" or current offset into the bytecode.
    pub pc: usize,
}

impl<'a> Decoder<'a> {
    /// Creates a new decoder for the given bytecode and program counter.
    pub fn new(program: Program, bytecode: &'a [u8], pc: usize) -> Self {
        Self {
            program,
            bytecode,
            pc,
        }
    }

    /// Decodes the next instruction.
    ///
    /// Returns `None` at the end of the bytecode stream.
    pub fn maybe_next(&mut self) -> Option<Result<Instruction<'a>, HintErrorKind>> {
        Some(self.next_inner(*self.bytecode.get(self.pc)?))
    }

    /// Decodes the next instruction.
    ///
    /// Returns `None` at the end of the bytecode stream.
    pub fn next(&mut self) -> Result<Instruction<'a>, HintErrorKind> {
        let opcode = *self
            .bytecode
            .get(self.pc)
            .ok_or(HintErrorKind::UnexpectedEndOfBytecode)?;
        self.next_inner(opcode)
    }

    fn next_inner(&mut self, opcode: Opcode) -> Result<Instruction<'a>, HintErrorKind> {
        let mut opcode_len = LENGTH_TABLE[opcode as usize] as i32;
        let mut arg_size = 0;
        if opcode_len < 0 {
            let next_byte = self
                .bytecode
                .get(self.pc + 1)
                .copied()
                .ok_or(HintErrorKind::UnexpectedEndOfBytecode)?;
            opcode_len = 2 - opcode_len * next_byte as i32;
            arg_size = 1;
        }
        let pc = self.pc;
        let next_pc = pc + opcode_len as usize;
        let mut arg_count = next_pc - pc - 1 - arg_size;
        let arg_start = pc + 1 + arg_size;
        let mut arguments = Arguments::default();
        if arg_count > 0 {
            arguments.raw = self
                .bytecode
                .get(arg_start..arg_start + arg_count)
                .ok_or(HintErrorKind::UnexpectedEndOfBytecode)?;
            arguments.is_words = (opcodes::PUSHW000..=opcodes::PUSHW111).contains(&opcode)
                || opcode == opcodes::NPUSHW;
            if arguments.is_words {
                arg_count /= 2;
            }
            arguments.len = arg_count as u8;
        }
        self.pc += opcode_len as usize;
        Ok(Instruction {
            program: self.program,
            pc,
            opcode,
            arguments,
        })
    }
}

/// Raw TrueType instruction opcodes.
pub mod opcodes {
    pub const SVTCA0: u8 = 0x00;
    pub const SFVTCA1: u8 = 0x05;
    pub const SPVTL0: u8 = 0x06;
    pub const SPVTL1: u8 = 0x07;
    pub const SFVTL1: u8 = 0x09;
    pub const SPVFS: u8 = 0x0A;
    pub const SFVFS: u8 = 0x0B;
    pub const GPV: u8 = 0x0C;
    pub const GFV: u8 = 0x0D;
    pub const SFVTPV: u8 = 0x0E;
    pub const ISECT: u8 = 0x0F;
    pub const SRP0: u8 = 0x10;
    pub const SRP1: u8 = 0x11;
    pub const SRP2: u8 = 0x12;
    pub const SZP0: u8 = 0x13;
    pub const SZP1: u8 = 0x14;
    pub const SZP2: u8 = 0x15;
    pub const SZPS: u8 = 0x16;
    pub const SLOOP: u8 = 0x17;
    pub const RTG: u8 = 0x18;
    pub const RTHG: u8 = 0x19;
    pub const SMD: u8 = 0x1A;
    pub const ELSE: u8 = 0x1B;
    pub const JMPR: u8 = 0x1C;
    pub const SCVTCI: u8 = 0x1D;
    pub const SSWCI: u8 = 0x1E;
    pub const SSW: u8 = 0x1F;
    pub const DUP: u8 = 0x20;
    pub const POP: u8 = 0x21;
    pub const CLEAR: u8 = 0x22;
    pub const SWAP: u8 = 0x23;
    pub const DEPTH: u8 = 0x24;
    pub const CINDEX: u8 = 0x25;
    pub const MINDEX: u8 = 0x26;
    pub const ALIGNPTS: u8 = 0x27;
    pub const UTP: u8 = 0x29;
    pub const LOOPCALL: u8 = 0x2A;
    pub const CALL: u8 = 0x2B;
    pub const FDEF: u8 = 0x2C;
    pub const ENDF: u8 = 0x2D;
    pub const MDAP0: u8 = 0x2E;
    pub const MDAP1: u8 = 0x2F;
    pub const IUP0: u8 = 0x30;
    pub const IUP1: u8 = 0x31;
    pub const SHP0: u8 = 0x32;
    pub const SHP1: u8 = 0x33;
    pub const SHC0: u8 = 0x34;
    pub const SHC1: u8 = 0x35;
    pub const SHZ0: u8 = 0x36;
    pub const SHZ1: u8 = 0x37;
    pub const SHPIX: u8 = 0x38;
    pub const IP: u8 = 0x39;
    pub const MSIRP0: u8 = 0x3A;
    pub const MSIRP1: u8 = 0x3B;
    pub const ALIGNRP: u8 = 0x3C;
    pub const RTDG: u8 = 0x3D;
    pub const MIAP0: u8 = 0x3E;
    pub const MIAP1: u8 = 0x3F;
    pub const NPUSHB: u8 = 0x40;
    pub const NPUSHW: u8 = 0x41;
    pub const WS: u8 = 0x42;
    pub const RS: u8 = 0x43;
    pub const WCVTP: u8 = 0x44;
    pub const RCVT: u8 = 0x45;
    pub const GC0: u8 = 0x46;
    pub const GC1: u8 = 0x47;
    pub const SCFS: u8 = 0x48;
    pub const MD0: u8 = 0x49;
    pub const MD1: u8 = 0x4A;
    pub const MPPEM: u8 = 0x4B;
    pub const MPS: u8 = 0x4C;
    pub const FLIPON: u8 = 0x4D;
    pub const FLIPOFF: u8 = 0x4E;
    pub const DEBUG: u8 = 0x4F;
    pub const LT: u8 = 0x50;
    pub const LTEQ: u8 = 0x51;
    pub const GT: u8 = 0x52;
    pub const GTEQ: u8 = 0x53;
    pub const EQ: u8 = 0x54;
    pub const NEQ: u8 = 0x55;
    pub const ODD: u8 = 0x56;
    pub const EVEN: u8 = 0x57;
    pub const IF: u8 = 0x58;
    pub const EIF: u8 = 0x59;
    pub const AND: u8 = 0x5A;
    pub const OR: u8 = 0x5B;
    pub const NOT: u8 = 0x5C;
    pub const DELTAP1: u8 = 0x5D;
    pub const SDB: u8 = 0x5E;
    pub const SDS: u8 = 0x5F;
    pub const ADD: u8 = 0x60;
    pub const SUB: u8 = 0x61;
    pub const DIV: u8 = 0x62;
    pub const MUL: u8 = 0x63;
    pub const ABS: u8 = 0x64;
    pub const NEG: u8 = 0x65;
    pub const FLOOR: u8 = 0x66;
    pub const CEILING: u8 = 0x67;
    pub const ROUND00: u8 = 0x68;
    pub const ROUND11: u8 = 0x6B;
    pub const NROUND00: u8 = 0x6C;
    pub const NROUND11: u8 = 0x6F;
    pub const WCVTF: u8 = 0x70;
    pub const DELTAP2: u8 = 0x71;
    pub const DELTAP3: u8 = 0x72;
    pub const DELTAC1: u8 = 0x73;
    pub const DELTAC2: u8 = 0x74;
    pub const DELTAC3: u8 = 0x75;
    pub const SROUND: u8 = 0x76;
    pub const S45ROUND: u8 = 0x77;
    pub const JROT: u8 = 0x78;
    pub const JROF: u8 = 0x79;
    pub const ROFF: u8 = 0x7A;
    pub const RUTG: u8 = 0x7C;
    pub const RDTG: u8 = 0x7D;
    pub const SANGW: u8 = 0x7E;
    pub const AA: u8 = 0x7F;
    pub const FLIPPT: u8 = 0x80;
    pub const FLIPRGON: u8 = 0x81;
    pub const FLIPRGOFF: u8 = 0x82;
    pub const SCANCTRL: u8 = 0x85;
    pub const SDPVTL0: u8 = 0x86;
    pub const SDPVTL1: u8 = 0x87;
    pub const GETINFO: u8 = 0x88;
    pub const IDEF: u8 = 0x89;
    pub const ROLL: u8 = 0x8A;
    pub const MAX: u8 = 0x8B;
    pub const MIN: u8 = 0x8C;
    pub const SCANTYPE: u8 = 0x8D;
    pub const INSTCTRL: u8 = 0x8E;
    pub const GETVAR: u8 = 0x91;
    pub const PUSHB000: u8 = 0xB0;
    pub const PUSHB111: u8 = 0xB7;
    pub const PUSHW000: u8 = 0xB8;
    pub const PUSHW111: u8 = 0xBF;
    pub const MDRP00000: u8 = 0xC0;
    pub const MDRP11111: u8 = 0xDF;
    pub const MIRP00000: u8 = 0xE0;
    pub const MIRP11111: u8 = 0xFF;
}

const LENGTH_TABLE: [i8; 256] = [
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    -1, -2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 3, 4, 5, 6, 7, 8, 9, 3, 5, 7, 9, 11, 13,
    15, 17, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1,
];

#[rustfmt::skip]
#[allow(clippy::eq_op, clippy::identity_op)]
const POP_PUSH_TABLE: [u8; 256] = [
    (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0,
    (2 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0,
    (0 << 4) | 2, (0 << 4) | 2, (0 << 4) | 0, (5 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0,
    (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0,
    (0 << 4) | 0, (0 << 4) | 0, (1 << 4) | 0, (0 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0,
    (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 2, (1 << 4) | 0, (0 << 4) | 0, (2 << 4) | 2,
    (0 << 4) | 1, (1 << 4) | 1, (1 << 4) | 0, (2 << 4) | 0, (0 << 4) | 0, (1 << 4) | 0,
    (2 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0, (0 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0,
    (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0,
    (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0, (0 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0,
    (0 << 4) | 0, (0 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0,
    (2 << 4) | 0, (1 << 4) | 1, (2 << 4) | 0, (1 << 4) | 1, (1 << 4) | 1, (1 << 4) | 1,
    (2 << 4) | 0, (2 << 4) | 1, (2 << 4) | 1, (0 << 4) | 1, (0 << 4) | 1, (0 << 4) | 0,
    (0 << 4) | 0, (1 << 4) | 0, (2 << 4) | 1, (2 << 4) | 1, (2 << 4) | 1, (2 << 4) | 1,
    (2 << 4) | 1, (2 << 4) | 1, (1 << 4) | 1, (1 << 4) | 1, (1 << 4) | 0, (0 << 4) | 0,
    (2 << 4) | 1, (2 << 4) | 1, (1 << 4) | 1, (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0,
    (2 << 4) | 1, (2 << 4) | 1, (2 << 4) | 1, (2 << 4) | 1, (1 << 4) | 1, (1 << 4) | 1,
    (1 << 4) | 1, (1 << 4) | 1, (1 << 4) | 1, (1 << 4) | 1, (1 << 4) | 1, (1 << 4) | 1,
    (1 << 4) | 1, (1 << 4) | 1, (1 << 4) | 1, (1 << 4) | 1, (2 << 4) | 0, (1 << 4) | 0,
    (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0,
    (2 << 4) | 0, (2 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0,
    (1 << 4) | 0, (1 << 4) | 0, (0 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0, (0 << 4) | 0,
    (0 << 4) | 0, (1 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0, (1 << 4) | 1, (1 << 4) | 0,
    (3 << 4) | 3, (2 << 4) | 1, (2 << 4) | 1, (1 << 4) | 0, (2 << 4) | 0, (0 << 4) | 0,
    (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 1, (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0,
    (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0,
    (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0,
    (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0,
    (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 0,
    (0 << 4) | 0, (0 << 4) | 0, (0 << 4) | 1, (0 << 4) | 2, (0 << 4) | 3, (0 << 4) | 4,
    (0 << 4) | 5, (0 << 4) | 6, (0 << 4) | 7, (0 << 4) | 8, (0 << 4) | 1, (0 << 4) | 2,
    (0 << 4) | 3, (0 << 4) | 4, (0 << 4) | 5, (0 << 4) | 6, (0 << 4) | 7, (0 << 4) | 8,
    (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0,
    (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0,
    (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0,
    (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0,
    (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0, (1 << 4) | 0,
    (1 << 4) | 0, (1 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0,
    (2 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0,
    (2 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0,
    (2 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0,
    (2 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0,
    (2 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0, (2 << 4) | 0,
];

#[rustfmt::skip]
const NAME_TABLE: [&str; 256] = [
    "SVTCA0", "SVTCA1", "SPVTCA0", "SPVTCA1", "SFVTCA0", "SFVTCA1", "SPVTL0", "SPVTL1", "SFVTL0",
    "SFVTL1", "SPVFS", "SFVFS", "GPV", "GFV", "SFVTPV", "ISECT", "SRP0", "SRP1", "SRP2", "SZP0",
    "SZP1", "SZP2", "SZPS", "SLOOP", "RTG", "RTHG", "SMD", "ELSE", "JMPR", "SCVTCI", "SSWCI",
    "SSW", "DUP", "POP", "CLEAR", "SWAP", "DEPTH", "CINDEX", "MINDEX", "ALIGNPTS", "OP28", "UTP",
    "LOOPCALL", "CALL", "FDEF", "ENDF", "MDAP0", "MDAP1", "IUP0", "IUP1", "SHP0", "SHP1", "SHC0",
    "SHC1", "SHZ0", "SHZ1", "SHPIX", "IP", "MSIRP0", "MSIRP1", "ALIGNRP", "RTDG", "MIAP0", "MIAP1",
    "NPUSHB", "NPUSHW", "WS", "RS", "WCVTP", "RCVT", "GC0", "GC1", "SCFS", "MD0", "MD1", "MPPEM",
    "MPS", "FLIPON", "FLIPOFF", "DEBUG", "LT", "LTEQ", "GT", "GTEQ", "EQ", "NEQ", "ODD", "EVEN",
    "IF", "EIF", "AND", "OR", "NOT", "DELTAP1", "SDB", "SDS", "ADD", "SUB", "DIV", "MUL", "ABS",
    "NEG", "FLOOR", "CEILING", "ROUND00", "ROUND01", "ROUND10", "ROUND11", "NROUND00", "NROUND01",
    "NROUND10", "NROUND11", "WCVTF", "DELTAP2", "DELTAP3", "DELTAC1", "DELTAC2", "DELTAC3",
    "SROUND", "S45ROUND", "JROT", "JROF", "ROFF", "OP7B", "RUTG", "RDTG", "SANGW", "AA", "FLIPPT",
    "FLIPRGON", "FLIPRGOFF", "OP83", "OP84", "SCANCTRL", "SDPVTL0", "SDPVTL1", "GETINFO", "IDEF",
    "ROLL", "MAX", "MIN", "SCANTYPE", "INSTCTRL", "OP8F", "OP90", "OP91", "OP92", "OP93", "OP94",
    "OP95", "OP96", "OP97", "OP98", "OP99", "OP9A", "OP9B", "OP9C", "OP9D", "OP9E", "OP9F", "OPA0",
    "OPA1", "OPA2", "OPA3", "OPA4", "OPA5", "OPA6", "OPA7", "OPA8", "OPA9", "OPAA", "OPAB", "OPAC",
    "OPAD", "OPAE", "OPAF", "PUSHB000", "PUSHB001", "PUSHB010", "PUSHB011", "PUSHB100", "PUSHB101",
    "PUSHB110", "PUSHB111", "PUSHW000", "PUSHW001", "PUSHW010", "PUSHW011", "PUSHW100", "PUSHW101",
    "PUSHW110", "PUSHW111", "MDRP00000", "MDRP00001", "MDRP00010", "MDRP00011", "MDRP00100",
    "MDRP00101", "MDRP00110", "MDRP00111", "MDRP01000", "MDRP01001", "MDRP01010", "MDRP01011",
    "MDRP01100", "MDRP01101", "MDRP01110", "MDRP01111", "MDRP10000", "MDRP10001", "MDRP10010",
    "MDRP10011", "MDRP10100", "MDRP10101", "MDRP10110", "MDRP10111", "MDRP11000", "MDRP11001",
    "MDRP11010", "MDRP11011", "MDRP11100", "MDRP11101", "MDRP11110", "MDRP11111", "MIRP00000",
    "MIRP00001", "MIRP00010", "MIRP00011", "MIRP00100", "MIRP00101", "MIRP00110", "MIRP00111",
    "MIRP01000", "MIRP01001", "MIRP01010", "MIRP01011", "MIRP01100", "MIRP01101", "MIRP01110",
    "MIRP01111", "MIRP10000", "MIRP10001", "MIRP10010", "MIRP10011", "MIRP10100", "MIRP10101",
    "MIRP10110", "MIRP10111", "MIRP11000", "MIRP11001", "MIRP11010", "MIRP11011", "MIRP11100",
    "MIRP11101", "MIRP11110", "MIRP11111",
];
