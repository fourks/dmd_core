use crate::bus::{AccessCode, Bus};
use crate::err::*;
use crate::instr::*;
use std::collections::HashMap;

///
/// PSW Flags and Offsets
///
const F_ET: u32 = 0x00000003;
const F_TM: u32 = 0x00000004;
const F_ISC: u32 = 0x00000078;
const F_I: u32 = 0x00000080;
const F_R: u32 = 0x00000100;
const F_PM: u32 = 0x00000600;
const F_CM: u32 = 0x00001800;
const F_C: u32 = 0x00040000;
const F_V: u32 = 0x00080000;
const F_Z: u32 = 0x00100000;
const F_N: u32 = 0x00200000;

const O_ET: u32 = 0;
const O_ISC: u32 = 3;

///
/// Register Indexes
///
const R_FP: usize = 9;
const R_AP: usize = 10;
const R_PSW: usize = 11;
const R_SP: usize = 12;
const R_PCBP: usize = 13;
const R_ISP: usize = 14;
const R_PC: usize = 15;

const IPL_TABLE: [u32; 64] = [
    0,  14, 14, 14, 14, 14, 14, 14,
    15, 15, 15, 15, 15, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15,
];

const WE32100_VERSION: u32 = 0x1a;

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum AddrMode {
    None,
    Absolute,
    AbsoluteDeferred,
    ByteDisplacement,
    ByteDisplacementDeferred,
    HalfwordDisplacement,
    HalfwordDisplacementDeferred,
    WordDisplacement,
    WordDisplacementDeferred,
    APShortOffset,
    FPShortOffset,
    ByteImmediate,
    HalfwordImmediate,
    WordImmediate,
    PositiveLiteral,
    NegativeLiteral,
    Register,
    RegisterDeferred,
    Expanded,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum OpType {
    Lit,
    Src,
    Dest,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Data {
    None,
    Byte,
    Half,
    Word,
    SByte,
    UHalf,
    UWord,
}

#[derive(Eq, PartialEq, Debug, Copy, Clone)]
pub enum CpuLevel {
    User,
    Supervisor,
    Executive,
    Kernel,
}

#[derive(Eq, PartialEq, Debug, Copy, Clone)]
pub enum ErrorContext {
    None,
    NormalGateVector,
    ProcessGatePcb,
    ProcessOldPcb,
    ProcessNewPcb,
    ResteGateVector,
    ResetSystemData,
    ResetIntStack,
    StackFault,
}

#[derive(Eq, PartialEq, Debug, Copy, Clone)]
pub struct Operand {
    pub size: u8,
    pub mode: AddrMode,
    data_type: Data,
    expanded_type: Option<Data>,
    pub register: Option<usize>,
    pub embedded: u32,
    pub data: u32, // Data moved to / from operand
}

impl Operand {
    fn new(
        size: u8,
        mode: AddrMode,
        data_type: Data,
        expanded_type: Option<Data>,
        register: Option<usize>,
        embedded: u32,
    ) -> Operand {
        Operand {
            size,
            mode,
            data_type,
            expanded_type,
            register,
            embedded,
            data: 0,
        }
    }

    fn data_type(&self) -> Data {
        match self.expanded_type {
            Some(t) => t,
            None => self.data_type,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Mnemonic {
    opcode: u16,
    dtype: Data,
    name: &'static str,
    ops: Vec<OpType>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Instruction {
    pub opcode: u16,
    pub name: &'static str,
    pub data_type: Data,
    pub bytes: u8,
    pub operand_count: u8,
    pub operands: [Operand; 4],
}

impl Instruction {
    pub fn decode(&self) -> String {
        format!("{}\t0x{:x}", self.name, 1000)
    }
}

macro_rules! mn {
    ($opcode:expr, $dtype:expr, $name:expr, $ops:expr) => {
        Mnemonic {
            opcode: $opcode,
            dtype: $dtype,
            name: $name,
            ops: $ops,
        }
    };
}

fn sign_extend_halfword(data: u16) -> u32 {
    ((data as i16) as i32) as u32
}

fn sign_extend_byte(data: u8) -> u32 {
    ((data as i8) as i32) as u32
}

fn add_offset(val: u32, offset: u32) -> u32 {
    ((val as i32).wrapping_add(offset as i32)) as u32
}

lazy_static! {
    static ref MNEMONICS: HashMap<u16, Mnemonic> = {
        let mut m = HashMap::new();

        m.insert(0x00, mn!(0x00, Data::None, "halt", vec!()));
        m.insert(0x02, mn!(0x02, Data::Word, "SPOPRD", vec!(OpType::Lit, OpType::Src)));
        m.insert(0x03, mn!(0x03, Data::Word, "SPOPRD2", vec!(OpType::Lit, OpType::Src, OpType::Dest)));
        m.insert(0x04, mn!(0x04, Data::Word, "MOVAW", vec!(OpType::Src, OpType::Dest)));
        m.insert(0x06, mn!(0x06, Data::Word, "SPOPRT", vec!(OpType::Lit, OpType::Src)));
        m.insert(0x07, mn!(0x07, Data::Word, "SPOPT2", vec!(OpType::Lit, OpType::Src, OpType::Dest)));
        m.insert(0x08, mn!(0x08, Data::None, "RET", vec!()));
        m.insert(0x0C, mn!(0x0C, Data::Word, "MOVTRW", vec!(OpType::Src, OpType::Dest)));
        m.insert(0x10, mn!(0x10, Data::Word, "SAVE", vec!(OpType::Src)));
        m.insert(0x13, mn!(0x13, Data::Word, "SPOPWD", vec!(OpType::Lit, OpType::Dest)));
        m.insert(0x14, mn!(0x14, Data::Byte, "EXTOP", vec!()));
        m.insert(0x17, mn!(0x17, Data::Word, "SPOPWT", vec!(OpType::Lit, OpType::Dest)));
        m.insert(0x18, mn!(0x18, Data::None, "RESTORE", vec!(OpType::Src)));
        m.insert(0x1C, mn!(0x1C, Data::Word, "SWAPWI", vec!(OpType::Dest)));
        m.insert(0x1E, mn!(0x1E, Data::Half, "SWAPHI", vec!(OpType::Dest)));
        m.insert(0x1F, mn!(0x1F, Data::Byte, "SWAPBI", vec!(OpType::Dest)));
        m.insert(0x20, mn!(0x20, Data::Word, "POPW", vec!(OpType::Src)));
        m.insert(0x22, mn!(0x22, Data::Word, "SPOPRS", vec!(OpType::Lit, OpType::Src)));
        m.insert(0x23, mn!(0x23, Data::Word, "SPOPS2", vec!(OpType::Lit, OpType::Src, OpType::Dest)));
        m.insert(0x24, mn!(0x24, Data::Word, "JMP", vec!(OpType::Dest)));
        m.insert(0x27, mn!(0x27, Data::None, "CFLUSH", vec!()));
        m.insert(0x28, mn!(0x28, Data::Word, "TSTW", vec!(OpType::Src)));
        m.insert(0x2A, mn!(0x2A, Data::Half, "TSTH", vec!(OpType::Src)));
        m.insert(0x2B, mn!(0x2B, Data::Byte, "TSTB", vec!(OpType::Src)));
        m.insert(0x2C, mn!(0x2C, Data::Word, "CALL", vec!(OpType::Src, OpType::Dest)));
        m.insert(0x2E, mn!(0x2E, Data::None, "BPT", vec!()));
        m.insert(0x2F, mn!(0x2F, Data::None, "WAIT", vec!()));
        m.insert(0x32, mn!(0x32, Data::Word, "SPOP", vec!(OpType::Lit)));
        m.insert(0x33, mn!(0x33, Data::Word, "SPOPWS", vec!(OpType::Lit, OpType::Dest)));
        m.insert(0x34, mn!(0x34, Data::Word, "JSB", vec!(OpType::Dest)));
        m.insert(0x36, mn!(0x36, Data::Half, "BSBH", vec!(OpType::Lit)));
        m.insert(0x37, mn!(0x37, Data::Byte, "BSBB", vec!(OpType::Lit)));
        m.insert(0x38, mn!(0x38, Data::Word, "BITW", vec!(OpType::Src, OpType::Src)));
        m.insert(0x3A, mn!(0x3A, Data::Half, "BITH", vec!(OpType::Src, OpType::Src)));
        m.insert(0x3B, mn!(0x3B, Data::Byte, "BITB", vec!(OpType::Src, OpType::Src)));
        m.insert(0x3C, mn!(0x3C, Data::Word, "CMPW", vec!(OpType::Src, OpType::Src)));
        m.insert(0x3E, mn!(0x3E, Data::Half, "CMPH", vec!(OpType::Src, OpType::Src)));
        m.insert(0x3F, mn!(0x3F, Data::Byte, "CMPB", vec!(OpType::Src, OpType::Src)));
        m.insert(0x40, mn!(0x40, Data::None, "RGEQ", vec!()));
        m.insert(0x42, mn!(0x42, Data::Half, "BGEH", vec!(OpType::Lit)));
        m.insert(0x43, mn!(0x43, Data::Byte, "BGEB", vec!(OpType::Lit)));
        m.insert(0x44, mn!(0x44, Data::None, "RGTR", vec!()));
        m.insert(0x46, mn!(0x46, Data::Half, "BGH", vec!(OpType::Lit)));
        m.insert(0x47, mn!(0x47, Data::Byte, "BGB", vec!(OpType::Lit)));
        m.insert(0x48, mn!(0x48, Data::None, "RLSS", vec!()));
        m.insert(0x4A, mn!(0x4A, Data::Half, "BLH", vec!(OpType::Lit)));
        m.insert(0x4B, mn!(0x4B, Data::Byte, "BLB", vec!(OpType::Lit)));
        m.insert(0x4C, mn!(0x4C, Data::None, "RLEQ", vec!()));
        m.insert(0x4E, mn!(0x4E, Data::Half, "BLEH", vec!(OpType::Lit)));
        m.insert(0x4F, mn!(0x4F, Data::Byte, "BLEB", vec!(OpType::Lit)));
        m.insert(0x50, mn!(0x50, Data::None, "RGEQU", vec!()));
        m.insert(0x52, mn!(0x52, Data::Half, "BGEUH", vec!(OpType::Lit)));
        m.insert(0x53, mn!(0x53, Data::Byte, "BGEUB", vec!(OpType::Lit)));
        m.insert(0x54, mn!(0x54, Data::None, "RGTRU", vec!()));
        m.insert(0x56, mn!(0x56, Data::Half, "BGUH", vec!(OpType::Lit)));
        m.insert(0x57, mn!(0x57, Data::Byte, "BGUB", vec!(OpType::Lit)));
        m.insert(0x58, mn!(0x58, Data::None, "RLSSU", vec!()));
        m.insert(0x5A, mn!(0x5A, Data::Half, "BLUH", vec!(OpType::Lit)));
        m.insert(0x5B, mn!(0x5B, Data::Byte, "BLUB", vec!(OpType::Lit)));
        m.insert(0x5C, mn!(0x5C, Data::None, "RLEQU", vec!()));
        m.insert(0x5E, mn!(0x5E, Data::Half, "BLEUH", vec!(OpType::Lit)));
        m.insert(0x5F, mn!(0x5F, Data::Byte, "BLEUB", vec!(OpType::Lit)));
        m.insert(0x60, mn!(0x60, Data::None, "RVC", vec!()));
        m.insert(0x62, mn!(0x62, Data::Half, "BVCH", vec!(OpType::Lit)));
        m.insert(0x63, mn!(0x63, Data::Byte, "BVCB", vec!(OpType::Lit)));
        m.insert(0x64, mn!(0x64, Data::None, "RNEQU", vec!()));
        m.insert(0x66, mn!(0x66, Data::Half, "BNEH", vec!(OpType::Lit)));
        m.insert(0x67, mn!(0x67, Data::Byte, "BNEB", vec!(OpType::Lit)));
        m.insert(0x68, mn!(0x68, Data::None, "RVS", vec!()));
        m.insert(0x6A, mn!(0x6A, Data::Half, "BVSH", vec!(OpType::Lit)));
        m.insert(0x6B, mn!(0x6B, Data::Byte, "BVSB", vec!(OpType::Lit)));
        m.insert(0x6C, mn!(0x6C, Data::None, "REQLU", vec!()));
        m.insert(0x6E, mn!(0x6E, Data::Half, "BEH", vec!(OpType::Lit)));
        m.insert(0x6F, mn!(0x6F, Data::Byte, "BEB", vec!(OpType::Lit)));
        m.insert(0x70, mn!(0x70, Data::None, "NOP", vec!()));
        m.insert(0x72, mn!(0x72, Data::None, "NOP3", vec!()));
        m.insert(0x73, mn!(0x73, Data::None, "NOP2", vec!()));
        m.insert(0x74, mn!(0x74, Data::None, "RNEQ", vec!()));
        m.insert(0x76, mn!(0x76, Data::Half, "BNEH", vec!(OpType::Lit)));
        m.insert(0x77, mn!(0x77, Data::Byte, "BNEB", vec!(OpType::Lit)));
        m.insert(0x78, mn!(0x78, Data::None, "RSB", vec!()));
        m.insert(0x7A, mn!(0x7A, Data::Half, "BRH", vec!(OpType::Lit)));
        m.insert(0x7B, mn!(0x7B, Data::Byte, "BRB", vec!(OpType::Lit)));
        m.insert(0x7C, mn!(0x7C, Data::None, "REQL", vec!()));
        m.insert(0x7E, mn!(0x7E, Data::Half, "BEH", vec!(OpType::Lit)));
        m.insert(0x7F, mn!(0x7F, Data::Byte, "BEB", vec!(OpType::Lit)));
        m.insert(0x80, mn!(0x80, Data::Word, "CLRW", vec!(OpType::Dest)));
        m.insert(0x82, mn!(0x82, Data::Half, "CLRH", vec!(OpType::Dest)));
        m.insert(0x83, mn!(0x83, Data::Byte, "CLRB", vec!(OpType::Dest)));
        m.insert(0x84, mn!(0x84, Data::Word, "MOVW", vec!(OpType::Src, OpType::Dest)));
        m.insert(0x86, mn!(0x86, Data::Half, "MOVH", vec!(OpType::Src, OpType::Dest)));
        m.insert(0x87, mn!(0x87, Data::Byte, "MOVB", vec!(OpType::Src, OpType::Dest)));
        m.insert(0x88, mn!(0x88, Data::Word, "MCOMW", vec!(OpType::Src, OpType::Dest)));
        m.insert(0x8A, mn!(0x8A, Data::Half, "MCOMH", vec!(OpType::Src, OpType::Dest)));
        m.insert(0x8B, mn!(0x8B, Data::Byte, "MCOMB", vec!(OpType::Src, OpType::Dest)));
        m.insert(0x8C, mn!(0x8C, Data::Word, "MNEGW", vec!(OpType::Src, OpType::Dest)));
        m.insert(0x8E, mn!(0x8E, Data::Half, "MNEGH", vec!(OpType::Src, OpType::Dest)));
        m.insert(0x8F, mn!(0x8F, Data::Byte, "MNEGB", vec!(OpType::Src, OpType::Dest)));
        m.insert(0x90, mn!(0x90, Data::Word, "INCW", vec!(OpType::Dest)));
        m.insert(0x92, mn!(0x92, Data::Half, "INCH", vec!(OpType::Dest)));
        m.insert(0x93, mn!(0x93, Data::Byte, "INCB", vec!(OpType::Dest)));
        m.insert(0x94, mn!(0x94, Data::Word, "DECW", vec!(OpType::Dest)));
        m.insert(0x96, mn!(0x96, Data::Half, "DECH", vec!(OpType::Dest)));
        m.insert(0x97, mn!(0x97, Data::Byte, "DECB", vec!(OpType::Dest)));
        m.insert(0x9C, mn!(0x9C, Data::Word, "ADDW2", vec!(OpType::Src, OpType::Dest)));
        m.insert(0x9E, mn!(0x9E, Data::Half, "ADDH2", vec!(OpType::Src, OpType::Dest)));
        m.insert(0x9F, mn!(0x9F, Data::Byte, "ADDB2", vec!(OpType::Src, OpType::Dest)));
        m.insert(0xA0, mn!(0xA0, Data::Word, "PUSHW", vec!(OpType::Src)));
        m.insert(0xA4, mn!(0xA4, Data::Word, "MODW2", vec!(OpType::Src, OpType::Dest)));
        m.insert(0xA6, mn!(0xA6, Data::Half, "MODH2", vec!(OpType::Src, OpType::Dest)));
        m.insert(0xA7, mn!(0xA7, Data::Byte, "MODB2", vec!(OpType::Src, OpType::Dest)));
        m.insert(0xA8, mn!(0xA8, Data::Word, "MULW2", vec!(OpType::Src, OpType::Dest)));
        m.insert(0xAA, mn!(0xAA, Data::Half, "MULH2", vec!(OpType::Src, OpType::Dest)));
        m.insert(0xAB, mn!(0xAB, Data::Byte, "MULB2", vec!(OpType::Src, OpType::Dest)));
        m.insert(0xAC, mn!(0xAC, Data::Word, "DIVW2", vec!(OpType::Src, OpType::Dest)));
        m.insert(0xAE, mn!(0xAE, Data::Half, "DIVH2", vec!(OpType::Src, OpType::Dest)));
        m.insert(0xAF, mn!(0xAF, Data::Byte, "DIVB2", vec!(OpType::Src, OpType::Dest)));
        m.insert(0xB0, mn!(0xB0, Data::Word, "ORW2", vec!(OpType::Src, OpType::Dest)));
        m.insert(0xB2, mn!(0xB2, Data::Half, "ORH2", vec!(OpType::Src, OpType::Dest)));
        m.insert(0xB3, mn!(0xB3, Data::Byte, "ORB2", vec!(OpType::Src, OpType::Dest)));
        m.insert(0xB4, mn!(0xB4, Data::Word, "XORW2", vec!(OpType::Src, OpType::Dest)));
        m.insert(0xB6, mn!(0xB6, Data::Half, "XORH2", vec!(OpType::Src, OpType::Dest)));
        m.insert(0xB7, mn!(0xB7, Data::Byte, "XORB2", vec!(OpType::Src, OpType::Dest)));
        m.insert(0xB8, mn!(0xB8, Data::Word, "ANDW2", vec!(OpType::Src, OpType::Dest)));
        m.insert(0xBA, mn!(0xBA, Data::Half, "ANDH2", vec!(OpType::Src, OpType::Dest)));
        m.insert(0xBB, mn!(0xBB, Data::Byte, "ANDB2", vec!(OpType::Src, OpType::Dest)));
        m.insert(0xBC, mn!(0xBC, Data::Word, "SUBW2", vec!(OpType::Src, OpType::Dest)));
        m.insert(0xBE, mn!(0xBE, Data::Half, "SUBH2", vec!(OpType::Src, OpType::Dest)));
        m.insert(0xBF, mn!(0xBF, Data::Byte, "SUBB2", vec!(OpType::Src, OpType::Dest)));
        m.insert(0xC0, mn!(0xC0, Data::Word, "ALSW3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xC4, mn!(0xC4, Data::Word, "ARSW3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xC6, mn!(0xC6, Data::Half, "ARSH3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xC7, mn!(0xC7, Data::Byte, "ARSB3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xC8, mn!(0xC8, Data::Word, "INSFW", vec!(OpType::Src, OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xCA, mn!(0xCA, Data::Half, "INSFH", vec!(OpType::Src, OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xCB, mn!(0xCB, Data::Byte, "INSFB", vec!(OpType::Src, OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xCC, mn!(0xCC, Data::Word, "EXTFW", vec!(OpType::Src, OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xCE, mn!(0xCE, Data::Half, "EXTFH", vec!(OpType::Src, OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xCF, mn!(0xCF, Data::Byte, "EXTFB", vec!(OpType::Src, OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xD0, mn!(0xD0, Data::Word, "LLSW3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xD2, mn!(0xD2, Data::Half, "LLSH3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xD3, mn!(0xD3, Data::Byte, "LLSB3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xD4, mn!(0xD4, Data::Word, "LRSW3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xD8, mn!(0xD8, Data::Word, "ROTW", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xDC, mn!(0xDC, Data::Word, "ADDW3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xDE, mn!(0xDE, Data::Half, "ADDH3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xDF, mn!(0xDF, Data::Byte, "ADDB3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xE0, mn!(0xE0, Data::Word, "PUSHAW", vec!(OpType::Src)));
        m.insert(0xE4, mn!(0xE4, Data::Word, "MODW3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xE6, mn!(0xE6, Data::Half, "MODH3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xE7, mn!(0xE7, Data::Byte, "MODB3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xE8, mn!(0xE8, Data::Word, "MULW3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xEA, mn!(0xEA, Data::Half, "MULH3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xEB, mn!(0xEB, Data::Byte, "MULB3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xEC, mn!(0xEC, Data::Word, "DIVW3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xEE, mn!(0xEE, Data::Half, "DIVH3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xEF, mn!(0xEF, Data::Byte, "DIVB3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xF0, mn!(0xF0, Data::Word, "ORW3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xF2, mn!(0xF2, Data::Half, "ORH3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xF3, mn!(0xF3, Data::Byte, "ORB3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xF4, mn!(0xF4, Data::Word, "XORW3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xF6, mn!(0xF6, Data::Half, "XORH3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xF7, mn!(0xF7, Data::Byte, "XORB3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xF8, mn!(0xF8, Data::Word, "ANDW3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xFA, mn!(0xFA, Data::Half, "ANDH3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xFB, mn!(0xFB, Data::Byte, "ANDB3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xFC, mn!(0xFC, Data::Word, "SUBW3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xFE, mn!(0xFE, Data::Half, "SUBH3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0xFF, mn!(0xFF, Data::Byte, "SUBB3", vec!(OpType::Src, OpType::Src, OpType::Dest)));
        m.insert(0x3009, mn!(0x3009, Data::None, "MVERNO", vec!()));
        m.insert(0x300d, mn!(0x300d, Data::None, "ENBVJMP", vec!()));
        m.insert(0x3013, mn!(0x3013, Data::None, "DISVJMP", vec!()));
        m.insert(0x3019, mn!(0x3019, Data::None, "MOVBLW", vec!()));
        m.insert(0x301f, mn!(0x301f, Data::None, "STREND", vec!()));
        m.insert(0x302f, mn!(0x302f, Data::None, "INTACK", vec!()));
        m.insert(0x303f, mn!(0x303f, Data::None, "STRCPY", vec!()));
        m.insert(0x3045, mn!(0x3045, Data::None, "RETG", vec!()));
        m.insert(0x3061, mn!(0x3061, Data::None, "GATE", vec!()));
        m.insert(0x30ac, mn!(0x30ac, Data::None, "CALLPS", vec!()));
        m.insert(0x30c8, mn!(0x30c8, Data::None, "RETPS", vec!()));

        m
    };
}

pub struct Cpu {
    //
    // Note that we store registers as an array of type u32 because
    // we often need to reference registers by index (0-15) when decoding
    // and executing instructions.
    //
    pub r: [u32; 16],
    error_context: ErrorContext,
    steps: u64,
    ir: Instruction,
}

impl Cpu {
    pub fn new() -> Cpu {
        Cpu {
            r: [0; 16],
            error_context: ErrorContext::None,
            steps: 0,
            ir: Instruction {
                opcode: 0,
                name: "???",
                data_type: Data::None,
                bytes: 0,
                operand_count: 0,
                operands: [
                    Operand::new(0, AddrMode::None, Data::None, None, None, 0),
                    Operand::new(0, AddrMode::None, Data::None, None, None, 0),
                    Operand::new(0, AddrMode::None, Data::None, None, None, 0),
                    Operand::new(0, AddrMode::None, Data::None, None, None, 0),
                ]
            }
        }
    }

    /// Reset the CPU.
    pub fn reset(&mut self, bus: &mut Bus) -> Result<(), BusError> {
        //
        // The WE32100 Manual, Page 2-52, describes the reset process
        //
        //  1. Change to physical address mode
        //  2. Fetch the word at physical address 0x80 and store it in
        //     the PCBP register.
        //  3. Fetch the word at the PCB address and store it in the
        //     PSW.
        //  4. Fetch the word at PCB address + 4 bytes and store it
        //     in the PC.
        //  5. Fetch the word at PCB address + 8 bytes and store it
        //     in the SP.
        //  6. Fetch the word at PCB address + 12 bytes and store it
        //     in the PCB, if bit I in PSW is set.
        //

        self.r[R_PCBP] = bus.read_word(0x80, AccessCode::AddressFetch)?;
        self.r[R_PSW] = bus.read_word(self.r[R_PCBP] as usize, AccessCode::AddressFetch)?;
        self.r[R_PC] = bus.read_word(self.r[R_PCBP] as usize + 4, AccessCode::AddressFetch)?;
        self.r[R_SP] = bus.read_word(self.r[R_PCBP] as usize + 8, AccessCode::AddressFetch)?;

        if self.r[R_PSW] & F_I != 0 {
            self.r[R_PSW] &= !F_I;
            self.r[R_PCBP] += 12;
        }

        self.set_isc(3); // Set ISC = 3

        Ok(())
    }

    /// Compute the effective address for an Operand.
    fn effective_address(&mut self, bus: &mut Bus, index: usize) -> Result<u32, CpuError> {

        let embedded = self.ir.operands[index].embedded;
        let mode = self.ir.operands[index].mode;
        let register = self.ir.operands[index].register;

        let addr: u32 = match mode {
            AddrMode::RegisterDeferred => {
                let r = match register {
                    Some(v) => v,
                    None => return Err(CpuError::Exception(CpuException::IllegalOpcode)),
                };
                self.r[r]
            }
            AddrMode::Absolute => embedded,
            AddrMode::AbsoluteDeferred => bus.read_word(embedded as usize, AccessCode::AddressFetch)?,
            AddrMode::FPShortOffset => add_offset(self.r[R_FP], sign_extend_byte(embedded as u8)),
            AddrMode::APShortOffset => add_offset(self.r[R_AP], sign_extend_byte(embedded as u8)),
            AddrMode::WordDisplacement => {
                let r = match register {
                    Some(v) => v,
                    None => return Err(CpuError::Exception(CpuException::IllegalOpcode)),
                };
                add_offset(self.r[r], embedded)
            }
            AddrMode::WordDisplacementDeferred => {
                let r = match register {
                    Some(v) => v,
                    None => return Err(CpuError::Exception(CpuException::IllegalOpcode)),
                };
                bus.read_word((add_offset(self.r[r], embedded)) as usize, AccessCode::AddressFetch)?
            }
            AddrMode::HalfwordDisplacement => {
                let r = match register {
                    Some(v) => v,
                    None => return Err(CpuError::Exception(CpuException::IllegalOpcode)),
                };
                add_offset(self.r[r], sign_extend_halfword(embedded as u16))
            }
            AddrMode::HalfwordDisplacementDeferred => {
                let r = match register {
                    Some(v) => v,
                    None => return Err(CpuError::Exception(CpuException::IllegalOpcode)),
                };
                bus.read_word((add_offset(self.r[r], sign_extend_halfword(embedded as u16))) as usize, AccessCode::AddressFetch)?
            }
            AddrMode::ByteDisplacement => {
                let r = match register {
                    Some(v) => v,
                    None => return Err(CpuError::Exception(CpuException::IllegalOpcode)),
                };
                add_offset(self.r[r], sign_extend_byte(embedded as u8))
            }
            AddrMode::ByteDisplacementDeferred => {
                let r = match register {
                    Some(v) => v,
                    None => return Err(CpuError::Exception(CpuException::IllegalOpcode)),
                };
                bus.read_word(add_offset(self.r[r], sign_extend_byte(embedded as u8)) as usize, AccessCode::AddressFetch)?
            }
            _ => return Err(CpuError::Exception(CpuException::IllegalOpcode)),
        };

        self.ir.operands[index].data = addr;

        Ok(addr)
    }

    /// Read the value pointed at by an Operand
    pub fn read_op(&mut self, bus: &mut Bus, index: usize) -> Result<u32, CpuError> {

        let mut op = self.ir.operands[index];

        let val: u32 = match op.mode {
            AddrMode::Register => {
                let r = match op.register {
                    Some(v) => v,
                    None => return Err(CpuError::Exception(CpuException::IllegalOpcode)),
                };

                match op.data_type() {
                    Data::Word | Data::UWord => self.r[r],
                    Data::Half => sign_extend_halfword(self.r[r] as u16),
                    Data::UHalf => (self.r[r] as u16) as u32,
                    Data::Byte => (self.r[r] as u8) as u32,
                    Data::SByte => sign_extend_byte(self.r[r] as u8),
                    _ => return Err(CpuError::Exception(CpuException::IllegalOpcode)),
                }
            }
            AddrMode::PositiveLiteral | AddrMode::NegativeLiteral => sign_extend_byte(op.embedded as u8),
            AddrMode::WordImmediate => op.embedded,
            AddrMode::HalfwordImmediate => sign_extend_halfword(op.embedded as u16),
            AddrMode::ByteImmediate => sign_extend_byte(op.embedded as u8),
            _ => {
                let eff = self.effective_address(bus, index)?;
                match op.data_type() {
                    Data::UWord | Data::Word => bus.read_word(eff as usize, AccessCode::InstrFetch)?,
                    Data::Half => sign_extend_halfword(bus.read_half(eff as usize, AccessCode::InstrFetch)?),
                    Data::UHalf => bus.read_half(eff as usize, AccessCode::InstrFetch)? as u32,
                    Data::Byte => bus.read_byte(eff as usize, AccessCode::InstrFetch)? as u32,
                    Data::SByte => sign_extend_byte(bus.read_byte(eff as usize, AccessCode::InstrFetch)?),
                    _ => return Err(CpuError::Exception(CpuException::IllegalOpcode)),
                }
            }
        };

        op.data = val;

        Ok(val)
    }

    /// Write a value to the location specified by an Operand
    pub fn write_op(&mut self, bus: &mut Bus, index: usize, val: u32) -> Result<(), CpuError> {
        let mode = self.ir.operands[index].mode;
        let register = self.ir.operands[index].register;
        let data_type = self.ir.operands[index].data_type();

        match mode {
            AddrMode::Register => match register {
                Some(r) => self.r[r] = val,
                None => return Err(CpuError::Exception(CpuException::IllegalOpcode)),
            },
            AddrMode::NegativeLiteral
            | AddrMode::PositiveLiteral
            | AddrMode::ByteImmediate
            | AddrMode::HalfwordImmediate
            | AddrMode::WordImmediate => {
                return Err(CpuError::Exception(CpuException::IllegalOpcode));
            }
            _ => {
                let eff = self.effective_address(bus, index)?;
                match data_type {
                    Data::UWord | Data::Word => bus.write_word(eff as usize, val)?,
                    Data::Half | Data::UHalf => bus.write_half(eff as usize, val as u16)?,
                    Data::Byte | Data::SByte => bus.write_byte(eff as usize, val as u8)?,
                    _ => return Err(CpuError::Exception(CpuException::IllegalOpcode)),
                }
            }
        };

        self.ir.operands[index].data = val;

        Ok(())
    }

    fn context_switch_1(&mut self, bus: &mut Bus, new_pcbp: u32) -> Result<(), CpuError> {
        // Save the current PC in the PCB
        bus.write_word((self.r[R_PCBP] + 4) as usize, self.r[R_PC])?;

        // Copy the 'R' flag from the new PSW to the old PSW
        self.r[R_PSW] &= !F_R;
        self.r[R_PSW] |= bus.read_word(new_pcbp as usize, AccessCode::AddressFetch)? & F_R;

        // Save the current PSW and SP in the old PCB
        bus.write_word(self.r[R_PCBP] as usize, self.r[R_PSW])?;
        bus.write_word((self.r[R_PCBP] + 8) as usize, self.r[R_SP])?;

        // If R is set, save the current R0-R8,FP,AP in the PCB
        if (self.r[R_PSW] & F_R) != 0 {
            bus.write_word((self.r[R_PCBP] + 24) as usize, self.r[R_FP])?;
            bus.write_word((self.r[R_PCBP] + 28) as usize, self.r[0])?;
            bus.write_word((self.r[R_PCBP] + 32) as usize, self.r[1])?;
            bus.write_word((self.r[R_PCBP] + 36) as usize, self.r[2])?;
            bus.write_word((self.r[R_PCBP] + 40) as usize, self.r[3])?;
            bus.write_word((self.r[R_PCBP] + 44) as usize, self.r[4])?;
            bus.write_word((self.r[R_PCBP] + 48) as usize, self.r[5])?;
            bus.write_word((self.r[R_PCBP] + 52) as usize, self.r[6])?;
            bus.write_word((self.r[R_PCBP] + 56) as usize, self.r[7])?;
            bus.write_word((self.r[R_PCBP] + 60) as usize, self.r[8])?;
            bus.write_word((self.r[R_PCBP] + 20) as usize, self.r[R_AP])?;

            self.r[R_FP] = self.r[R_PCBP] + 52;
        }

        Ok(())
    }

    fn context_switch_2(&mut self, bus: &mut Bus, new_pcbp: u32) -> Result<(), CpuError> {
        self.r[R_PCBP] = new_pcbp;

        // Put new PSW, PC, and SP values from PCB into registers
        self.r[R_PSW] = bus.read_word(self.r[R_PCBP] as usize, AccessCode::AddressFetch)?;
        self.r[R_PSW] &= !F_TM;
        self.r[R_PC] = bus.read_word((self.r[R_PCBP] + 4) as usize, AccessCode::AddressFetch)?;
        self.r[R_SP] = bus.read_word((self.r[R_PCBP] + 8) as usize, AccessCode::AddressFetch)?;

        // If the I-bit is set, increment the PCBP past initial context area
        if (self.r[R_PSW] & F_I) != 0 {
            self.r[R_PSW] &= !F_I;
            self.r[R_PCBP] += 12;
        }

        Ok(())
    }

    fn context_switch_3(&mut self, bus: &mut Bus) -> Result<(), CpuError> {
        if (self.r[R_PSW] & F_R) != 0 {
            self.r[0] = self.r[R_PCBP] + 64;
            self.r[2] = bus.read_word(self.r[0] as usize, AccessCode::AddressFetch)?;
            self.r[0] += 4;

            while self.r[2] != 0 {
                self.r[1] = bus.read_word(self.r[0] as usize, AccessCode::AddressFetch)?;
                self.r[0] += 4;

                // Execute MOVBLW instruction inside this loop
                while self.r[2] != 0 {
                    let tmp = bus.read_word(self.r[0] as usize, AccessCode::AddressFetch)?;
                    bus.write_word(self.r[1] as usize, tmp)?;
                    self.r[2] -= 1;
                    self.r[0] += 4;
                    self.r[1] += 4;
                }

                self.r[2] = bus.read_word(self.r[0] as usize, AccessCode::AddressFetch)?;
                self.r[0] += 4;
            }

            self.r[0] += 4;
        }

        Ok(())
    }

    fn add(&mut self, bus: &mut Bus, a: u32, b: u32, dst: usize) -> Result<(), CpuError> {
        let result: u64 = (a as u64).wrapping_add(b as u64);

        self.write_op(bus, dst, result as u32)?;

        self.set_nz_flags(result as u32, dst);

        let data_type = self.ir.operands[dst].data_type();

        match data_type {
            Data::Word | Data::UWord => {
                self.set_c_flag(result > 0xffffffff);
                self.set_v_flag((((a ^ !b) & (a ^ result as u32)) & 0x80000000) != 0);
            }
            Data::Half | Data::UHalf => {
                self.set_c_flag(result > 0xffff);
                self.set_v_flag((((a ^ !b) & (a ^ result as u32)) & 0x8000) != 0);
            }
            Data::Byte | Data::SByte => {
                self.set_c_flag(result > 0xff);
                self.set_v_flag((((a ^ !b) & (a ^ result as u32)) & 0x80) != 0);
            }
            _ => {
                return Err(CpuError::Exception(CpuException::IllegalOpcode));
            }
        }

        Ok(())
    }

    fn sub(&mut self, bus: &mut Bus, a: u32, b: u32, dst: usize) -> Result<(), CpuError> {
        let result: u64 = (a as u64).wrapping_sub(b as u64);

        self.write_op(bus, dst, result as u32)?;

        self.set_nz_flags(result as u32, dst);
        self.set_c_flag(b > a);
        self.set_v_flag_op(result as u32, dst);

        Ok(())
    }

    fn div(&mut self, a: u32, b: u32, _src: usize, dst: usize) -> u32 {
        match self.ir.operands[dst].data_type {
            Data::Word => (b as i32 / a as i32) as u32,
            Data::Half => (b as i16 / a as i16) as u32,
            Data::SByte => (b as i8 / a as i8) as u32,
            Data::UWord => b / a,
            Data::UHalf => (b as u16 / a as u16) as u32,
            Data::Byte => (b as u8 / a as u8) as u32,
            _ => b / a,
        }
    }

    fn modulo(&mut self, a: u32, b: u32, _src: usize, dst: usize) -> u32 {
        match self.ir.operands[dst].data_type {
            Data::Word => (b as i32 % a as i32) as u32,
            Data::Half => (b as i16 % a as i16) as u32,
            Data::SByte => (b as i8 % a as i8) as u32,
            Data::UWord => b % a,
            Data::UHalf => (b as u16 % a as u16) as u32,
            Data::Byte => (b as u8 % a as u8) as u32,
            _ => b % a,
        }
    }

    // TODO: Remove unwraps
    fn on_interrupt(&mut self, bus: &mut Bus, vector: u8) {
        let new_pcbp = bus.read_word((0x8c + (4 * (vector as u32))) as usize, AccessCode::AddressFetch).unwrap();
        self.irq_push(bus, self.r[R_PCBP]).unwrap();

        self.r[R_PSW] &= !(F_ISC | F_TM | F_ET);
        self.r[R_PSW] |= 1;

        self.context_switch_1(bus, new_pcbp).unwrap();
        self.context_switch_2(bus, new_pcbp).unwrap();

        self.r[R_PSW] &= !(F_ISC | F_TM | F_ET);
        self.r[R_PSW] |= 7 << 3;
        self.r[R_PSW] |= 3;

        self.context_switch_3(bus).unwrap();
    }

    fn dispatch(&mut self, bus: &mut Bus) -> Result<i32, CpuError> {
        self.steps += 1;

        // Update anything that needs updating.
        bus.service();

        let interrupt: Option<u8> = bus.get_interrupts();

        match interrupt {
            Some(val) => {
                let cpu_ipl = (self.r[R_PSW]) >> 13 & 0xf;
                if cpu_ipl < IPL_TABLE[(val & 0x3f) as usize] {
                    self.on_interrupt(bus, (!val) & 0x3f);
                }
            }
            None => {}
        }

        self.decode_instruction(bus)?;
        let mut pc_increment: i32 = self.ir.bytes as i32;

        match self.ir.opcode {
            NOP => {
                pc_increment = 1;
            }
            NOP2 => {
                pc_increment = 2;
            }
            NOP3 => {
                pc_increment = 3;
            }
            ADDW2 | ADDH2 | ADDB2 => {
                let a = self.read_op(bus, 0)?;
                let b = self.read_op(bus, 1)?;
                self.add(bus, a, b, 1)?;
            }
            ADDW3 | ADDH3 | ADDB3 => {
                let a = self.read_op(bus, 0)?;
                let b = self.read_op(bus, 1)?;
                self.add(bus, a, b, 2)?
            }
            ALSW3 => {
                let a = self.read_op(bus, 0)?;
                let b = self.read_op(bus, 1)?;
                let result = (b as u64) << (a & 0x1f);
                self.write_op(bus, 2, result as u32)?;

                self.set_nz_flags(result as u32, 2);
                self.set_c_flag(false);
                self.set_v_flag_op(result as u32, 2);
            }
            ANDW2 | ANDH2 | ANDB2 => {
                let a = self.read_op(bus, 0)?;
                let b = self.read_op(bus, 1)?;

                let result = a & b;

                self.write_op(bus, 1, result)?;

                self.set_nz_flags(result, 1);
                self.set_c_flag(false);
                self.set_v_flag_op(result, 1);
            }
            ANDW3 | ANDH3 | ANDB3 => {
                let a = self.read_op(bus, 0)?;
                let b = self.read_op(bus, 1)?;

                let result = a & b;

                self.write_op(bus, 2, result)?;

                self.set_nz_flags(result, 2);
                self.set_c_flag(false);
                self.set_v_flag_op(result, 2);
            }
            BEH | BEH_D => {
                if self.z_flag() {
                    pc_increment = sign_extend_halfword(self.ir.operands[0].embedded as u16) as i32;
                }
            }
            BEB | BEB_D => {
                if self.z_flag() {
                    pc_increment = sign_extend_byte(self.ir.operands[0].embedded as u8) as i32;
                }
            }
            BGH => {
                if !(self.n_flag() || self.z_flag()) {
                    pc_increment = sign_extend_halfword(self.ir.operands[0].embedded as u16) as i32;
                }
            }
            BGB => {
                if !(self.n_flag() || self.z_flag()) {
                    pc_increment = sign_extend_byte(self.ir.operands[0].embedded as u8) as i32;
                }
            }
            BGEH => {
                if !self.n_flag() || self.z_flag() {
                    pc_increment = sign_extend_halfword(self.ir.operands[0].embedded as u16) as i32;
                }
            }
            BGEB => {
                if !self.n_flag() || self.z_flag() {
                    pc_increment = sign_extend_byte(self.ir.operands[0].embedded as u8) as i32;
                }
            }
            BGEUH => {
                if !self.c_flag() {
                    pc_increment = sign_extend_halfword(self.ir.operands[0].embedded as u16) as i32;
                }
            }
            BGEUB => {
                if !self.c_flag() {
                    pc_increment = sign_extend_byte(self.ir.operands[0].embedded as u8) as i32;
                }
            }
            BGUH => {
                if !(self.c_flag() || self.z_flag()) {
                    pc_increment = sign_extend_halfword(self.ir.operands[0].embedded as u16) as i32;
                }
            }
            BGUB => {
                if !(self.c_flag() || self.z_flag()) {
                    pc_increment = sign_extend_byte(self.ir.operands[0].embedded as u8) as i32;
                }
            }
            BITW | BITH | BITB => {
                let a = self.read_op(bus, 0)?;
                let b = self.read_op(bus, 1)?;
                let result = a & b;
                self.set_nz_flags(result, 1);
                self.set_c_flag(false);
                self.set_v_flag(false);
            }
            BLH => {
                if self.n_flag() && !self.z_flag() {
                    pc_increment = sign_extend_halfword(self.ir.operands[0].embedded as u16) as i32;
                }
            }
            BLB => {
                if self.n_flag() && !self.z_flag() {
                    pc_increment = sign_extend_byte(self.ir.operands[0].embedded as u8) as i32;
                }
            }
            BLEH => {
                if self.n_flag() || self.z_flag() {
                    pc_increment = sign_extend_halfword(self.ir.operands[0].embedded as u16) as i32;
                }
            }
            BLEB => {
                if self.n_flag() || self.z_flag() {
                    pc_increment = sign_extend_byte(self.ir.operands[0].embedded as u8) as i32;
                }
            }
            BLEUH => {
                if self.c_flag() || self.z_flag() {
                    pc_increment = sign_extend_halfword(self.ir.operands[0].embedded as u16) as i32;
                }
            }
            BLEUB => {
                if self.c_flag() || self.z_flag() {
                    pc_increment = sign_extend_byte(self.ir.operands[0].embedded as u8) as i32;
                }
            }
            BLUH => {
                if self.c_flag() {
                    pc_increment = sign_extend_halfword(self.ir.operands[0].embedded as u16) as i32;
                }
            }
            BLUB => {
                if self.c_flag() {
                    pc_increment = sign_extend_byte(self.ir.operands[0].embedded as u8) as i32;
                }
            }
            BNEH | BNEH_D => {
                if !self.z_flag() {
                    pc_increment = sign_extend_halfword(self.ir.operands[0].embedded as u16) as i32;
                }
            }
            BNEB | BNEB_D => {
                if !self.z_flag() {
                    pc_increment = sign_extend_byte(self.ir.operands[0].embedded as u8) as i32;
                }
            }
            BPT | HALT => {
                // TODO: Breakpoint Trap
                unimplemented!()
            }
            BRH => {
                pc_increment = sign_extend_halfword(self.ir.operands[0].embedded as u16) as i32;
            }
            BRB => {
                pc_increment = sign_extend_byte(self.ir.operands[0].embedded as u8) as i32;
            }
            BSBH => {
                let offset = sign_extend_halfword(self.ir.operands[0].embedded as u16) as i32;
                let return_pc = (self.r[R_PC] as i32 + pc_increment) as u32;
                self.stack_push(bus, return_pc)?;
                pc_increment = offset;
            }
            BSBB => {
                let offset = sign_extend_byte(self.ir.operands[0].embedded as u8) as i32;
                let return_pc = (self.r[R_PC] as i32 + pc_increment) as u32;
                self.stack_push(bus, return_pc)?;
                pc_increment = offset;
            }
            BVCH => {
                if !self.v_flag() {
                    pc_increment = sign_extend_halfword(self.ir.operands[0].embedded as u16) as i32;
                }
            }
            BVCB => {
                if !self.v_flag() {
                    pc_increment = sign_extend_byte(self.ir.operands[0].embedded as u8) as i32;
                }
            }
            BVSH => {
                if self.v_flag() {
                    pc_increment = sign_extend_halfword(self.ir.operands[0].embedded as u16) as i32;
                }
            }
            BVSB => {
                if self.v_flag() {
                    pc_increment = sign_extend_byte(self.ir.operands[0].embedded as u8) as i32;
                }
            }
            CALL => {
                let a = self.effective_address(bus, 0)?;
                let b = self.effective_address(bus, 1)?;

                let return_pc = (self.r[R_PC] as i32 + pc_increment) as u32;

                bus.write_word((self.r[R_SP] + 4) as usize, self.r[R_AP])?;
                bus.write_word(self.r[R_SP] as usize, return_pc)?;

                self.r[R_SP] += 8;
                self.r[R_PC] = b;
                self.r[R_AP] = a;

                pc_increment = 0;
            }
            CFLUSH => {}
            CALLPS => {
                match self.priv_level() {
                    CpuLevel::Kernel => {
                        let a = self.r[0];
                        self.error_context = ErrorContext::ResetIntStack;

                        self.irq_push(bus, self.r[R_PCBP])?;

                        // Set the current PC to the start of the next instruction
                        // (always PC + 2)
                        pc_increment = 0;
                        self.r[R_PC] += 2;

                        // Set old PSW ISC, TM, and ET to 0, 0, 1
                        self.r[R_PSW] &= !(F_ISC | F_TM | F_ET);
                        self.r[R_PSW] |= 1 << O_ET;

                        self.context_switch_1(bus, a)?;
                        self.context_switch_2(bus, a)?;

                        self.r[R_PSW] &= !(F_ISC | F_TM | F_ET);
                        self.r[R_PSW] |= 7 << O_ISC;
                        self.r[R_PSW] |= 3 << O_ET;

                        self.context_switch_3(bus)?;

                        self.error_context = ErrorContext::None;
                    }
                    _ => return Err(CpuError::Exception(CpuException::PrivilegedOpcode)),
                }
            }
            CLRW | CLRH | CLRB => {
                self.write_op(bus, 0, 0)?;
                self.set_n_flag(false);
                self.set_z_flag(true);
                self.set_c_flag(false);
                self.set_v_flag(false);
            }
            CMPW => {
                let a = self.read_op(bus, 0)?;
                let b = self.read_op(bus, 1)?;

                self.set_z_flag(b == a);
                self.set_n_flag((b as i32) < (a as i32));
                self.set_c_flag(b < a);
                self.set_v_flag(false);
            }
            CMPH => {
                let a = self.read_op(bus, 0)?;
                let b = self.read_op(bus, 1)?;

                self.set_z_flag((b as u16) == (a as u16));
                self.set_n_flag((b as i16) < (a as i16));
                self.set_c_flag((b as u16) < (a as u16));
                self.set_v_flag(false);
            }
            CMPB => {
                let a = self.read_op(bus, 0)?;
                let b = self.read_op(bus, 1)?;

                self.set_z_flag((b as u8) == (a as u8));
                self.set_n_flag((b as i8) < (a as i8));
                self.set_c_flag((b as u8) < (a as u8));
                self.set_v_flag(false);
            }
            DECW | DECH | DECB => {
                let dst = 0;
                let a = self.read_op(bus, dst)?;
                self.sub(bus, a, 1, dst)?;
            }
            DIVW2 => {
                // TODO: Division needs to be revisited.
                let a = self.read_op(bus, 0)?;
                let b = self.read_op(bus, 1)?;

                if a == 0 {
                    return Err(CpuError::Exception(CpuException::IntegerZeroDivide));
                }

                if a == 0xffffffff && b == 0x80000000 {
                    self.set_v_flag(true);
                }

                let result = self.div(a, b, 0, 1);
                self.write_op(bus, 1, result)?;
                self.set_nz_flags(result, 1);
                self.set_c_flag(false);
            }
            DIVH2 => {
                let a = self.read_op(bus, 0)?;
                let b = self.read_op(bus, 1)?;

                if a == 0 {
                    return Err(CpuError::Exception(CpuException::IntegerZeroDivide));
                }

                if a == 0xffff && b == 0x8000 {
                    self.set_v_flag(true);
                }

                let result = self.div(a, b, 0, 1);
                self.write_op(bus, 1, result)?;
                self.set_nz_flags(result, 1);
                self.set_c_flag(false);
            }
            DIVB2 => {
                let a = self.read_op(bus, 0)?;
                let b = self.read_op(bus, 1)?;

                if a == 0 {
                    return Err(CpuError::Exception(CpuException::IntegerZeroDivide));
                }

                if a == 0xff && b == 0x80 {
                    self.set_v_flag(true);
                }

                let result = self.div(a, b, 0, 1);
                self.write_op(bus, 1, result)?;
                self.set_nz_flags(result, 1);
                self.set_c_flag(false);
            }
            DIVW3 => {
                let a = self.read_op(bus, 0)?;
                let b = self.read_op(bus, 1)?;

                if a == 0 {
                    return Err(CpuError::Exception(CpuException::IntegerZeroDivide));
                }

                if a == 0xffffffff && b == 0x80000000 {
                    self.set_v_flag(true);
                }

                let result = self.div(a, b, 0, 1);
                self.write_op(bus, 2, result)?;
                self.set_nz_flags(result, 2);
                self.set_c_flag(false);
            }
            DIVH3 => {
                let a = self.read_op(bus, 0)?;
                let b = self.read_op(bus, 1)?;

                if a == 0 {
                    return Err(CpuError::Exception(CpuException::IntegerZeroDivide));
                }

                if a == 0xffff && b == 0x8000 {
                    self.set_v_flag(true);
                }

                let result = self.div(a, b, 0, 1);
                self.write_op(bus, 2, result)?;
                self.set_nz_flags(result, 2);
                self.set_c_flag(false);
            }
            DIVB3 => {
                let a = self.read_op(bus, 0)?;
                let b = self.read_op(bus, 1)?;

                if a == 0 {
                    return Err(CpuError::Exception(CpuException::IntegerZeroDivide));
                }

                if a == 0xff && b == 0x80 {
                    self.set_v_flag(true);
                }

                let result = self.div(a, b, 0, 1);
                self.write_op(bus, 2, result)?;
                self.set_nz_flags(result, 2);
                self.set_c_flag(false);
            }
            MVERNO => {
                self.r[0] = WE32100_VERSION;
            }
            ENBVJMP => {
                match self.priv_level() {
                    CpuLevel::Kernel => {
                        // TODO: Enable MMU, if present
                        self.r[R_PC] = self.r[0];
                        pc_increment = 0;
                    }
                    _ => {
                        return Err(CpuError::Exception(CpuException::PrivilegedOpcode));
                    }
                }
            }
            DISVJMP => {
                match self.priv_level() {
                    CpuLevel::Kernel => {
                        // TODO: Disable MMU, if present
                        self.r[R_PC] = self.r[0];
                        pc_increment = 0;
                    }
                    _ => {
                        return Err(CpuError::Exception(CpuException::PrivilegedOpcode));
                    }
                }
            }
            EXTFW | EXTFH | EXTFB => {
                let width = (self.read_op(bus, 0)? & 0x1f) + 1;
                let offset = self.read_op(bus, 1)? & 0x1f;

                let mut mask = if width >= 32 {
                    0xffffffff
                } else {
                    (1 << width) - 1
                };

                mask = mask << offset;

                if width + offset > 32 {
                    mask |= 1 << ((width + offset) - 32) - 1;
                }

                let mut a = self.read_op(bus, 2)?;
                a &= mask;
                a = a >> offset;

                self.write_op(bus, 3, a)?;
                self.set_nz_flags(a, 3);
                self.set_c_flag(false);
                self.set_v_flag_op(a, 3);
            }
            INCW | INCH | INCB => {
                let a = self.read_op(bus, 0)?;
                self.add(bus, a, 1, 0)?;
            }
            INSFW | INSFH | INSFB => {
                let width = (self.read_op(bus, 0)? & 0x1f) + 1;
                let offset = self.read_op(bus, 1)? & 0x1f;

                let mask = if width >= 32 {
                    0xffffffff
                } else {
                    (1 << width) - 1
                };

                let a = self.read_op(bus, 2)? & mask;
                let mut b = self.read_op(bus, 3)?;

                b &= !(mask << offset);
                b |= a << offset;

                self.write_op(bus, 3, b)?;
                self.set_nz_flags(b, 3);
                self.set_c_flag(false);
                self.set_v_flag_op(b, 3);
            }
            JMP => {
                self.r[R_PC] = self.effective_address(bus, 0)?;
                pc_increment = 0;
            }
            JSB => {
                let dst = 0;
                self.stack_push(bus, (self.r[R_PC] as i32 + pc_increment) as u32)?;
                self.r[R_PC] = self.effective_address(bus, dst)?;
                pc_increment = 0;
            }
            LLSW3 | LLSH3 | LLSB3 => {
                let a: u64 = self.read_op(bus, 1)? as u64;
                let b = self.read_op(bus, 0)? & 0x1f;

                let result = (a << b) as u32;

                self.write_op(bus, 2, result)?;
                self.set_nz_flags(result, 2);
                self.set_c_flag(false);
                self.set_v_flag_op(result, 2);
            }
            ARSW3 | ARSH3 | ARSB3 => {
                let a = self.read_op(bus, 1)?;
                let b = self.read_op(bus, 0)? & 0x1f;
                let result = match self.ir.operands[0].data_type() {
                    Data::Word => (a as i32 >> b as i32) as u32,
                    Data::UWord => a >> b,
                    Data::Half => (a as i16 >> b as i16) as u32,
                    Data::UHalf => (a as u16 >> b as u16) as u32,
                    Data::Byte => (a as u8 >> b as u8) as u32,
                    Data::SByte => (a as i8 >> b as i8) as u32,
                    _ => 0,
                };
                self.write_op(bus, 2, result)?;
                self.set_nz_flags(result, 2);
                self.set_c_flag(false);
                self.set_v_flag(false);
            }
            LRSW3 => {
                let a = self.read_op(bus, 1)?;
                let b = self.read_op(bus, 0)? & 0x1f;
                let result = a >> b;
                self.write_op(bus, 2, result)?;
                self.set_nz_flags(result, 2);
                self.set_c_flag(false);
                self.set_v_flag_op(result, 2);
            }
            MCOMW | MCOMH | MCOMB => {
                let a = self.read_op(bus, 0)?;
                let result = !a;
                self.write_op(bus, 1, result)?;
                self.set_nz_flags(result, 1);
                self.set_c_flag(false);
                self.set_v_flag_op(result, 1);
            }
            MNEGW | MNEGH | MNEGB => {
                let a = self.read_op(bus, 0)?;
                let result = !a + 1;
                self.write_op(bus, 1, result)?;
                self.set_nz_flags(result, 1);
                self.set_c_flag(false);
                self.set_v_flag_op(result, 1);
            }
            MOVBLW => {
                while self.r[2] != 0 {
                    let a = bus.read_word(self.r[0] as usize, AccessCode::AddressFetch)?;
                    bus.write_word(self.r[1] as usize, a)?;
                    self.r[2] -= 1;
                    self.r[0] += 4;
                    self.r[1] += 4;
                }
            }
            STREND => {
                while bus.read_byte(self.r[0] as usize, AccessCode::AddressFetch)? != 0 {
                    self.r[0] += 1;
                }
            }
            SWAPWI | SWAPHI | SWAPBI => {
                let a = self.read_op(bus, 0)?;
                self.write_op(bus, 0, self.r[0])?;
                self.r[0] = a;
                self.set_n_flag((a as i32) < 0);
                self.set_z_flag(a == 0);
                self.set_c_flag(false);
                self.set_v_flag(false);
            }
            ROTW => {
                let a = self.read_op(bus, 0)? & 0x1f;
                let b = self.read_op(bus, 1)?;
                let result = b.rotate_right(a);
                self.write_op(bus, 2, result)?;
                self.set_nz_flags(result, 2);
                self.set_c_flag(false);
                self.set_v_flag(false);
            }
            MOVAW => {
                let result = self.effective_address(bus, 0)?;
                self.write_op(bus, 1, result)?;
            }
            MOVB | MOVH | MOVW => {
                let val = self.read_op(bus, 0)?;
                self.write_op(bus, 1, val)?;
                self.set_nz_flags(val, 1);
                self.set_c_flag(false);
                self.set_v_flag_op(val, 1);
            }
            MODW2 | MODH2 | MODB2 => {
                // TODO: Modulo needs to be revisited.
                let a = self.read_op(bus, 0)?;
                let b = self.read_op(bus, 1)?;
                if a == 0 {
                    return Err(CpuError::Exception(CpuException::IntegerZeroDivide));
                }
                let result = self.modulo(a, b, 0, 1);
                self.write_op(bus, 1, result)?;
                self.set_nz_flags(result, 1);
                self.set_c_flag(false);
                self.set_v_flag_op(result, 1);
            }
            MODW3 | MODH3 | MODB3 => {
                let a = self.read_op(bus, 0)?;
                let b = self.read_op(bus, 1)?;

                if a == 0 {
                    return Err(CpuError::Exception(CpuException::IntegerZeroDivide));
                }

                let result = self.modulo(a, b, 0, 1);
                self.write_op(bus, 2, result)?;
                self.set_nz_flags(result, 2);
                self.set_c_flag(false);
                self.set_v_flag_op(result, 2);
            }
            MULW2 | MULH2 | MULB2 => {
                let result = self.read_op(bus, 0)? * self.read_op(bus, 1)?;

                self.write_op(bus, 1, result)?;
                self.set_nz_flags(result, 1);
                self.set_c_flag(false);
                self.set_v_flag_op(result, 1);
            }
            MULW3 | MULH3 | MULB3 => {
                let result = self.read_op(bus, 0)? * self.read_op(bus, 1)?;

                self.write_op(bus, 2, result)?;
                self.set_nz_flags(result, 2);
                self.set_c_flag(false);
                self.set_v_flag_op(result, 2);
            }
            ORW2 | ORH2 | ORB2 => {
                let result = self.read_op(bus, 0)? | self.read_op(bus, 1)?;

                self.write_op(bus, 1, result)?;

                self.set_nz_flags(result, 1);
                self.set_c_flag(false);
                self.set_v_flag_op(result, 1);
            }
            ORW3 | ORH3 | ORB3 => {
                let result = self.read_op(bus, 0)? | self.read_op(bus, 1)?;

                self.write_op(bus, 2, result)?;
                self.set_nz_flags(result, 2);
                self.set_c_flag(false);
                self.set_v_flag_op(result, 2);
            }
            POPW => {
                let val = bus.read_word(self.r[R_SP] as usize - 4, AccessCode::AddressFetch)?;
                self.write_op(bus, 0, val)?;
                self.r[R_SP] -= 4;
                self.set_nz_flags(val, 0);
                self.set_c_flag(false);
                self.set_v_flag(false);
            }
            PUSHAW => {
                let val = self.effective_address(bus, 0)?;
                self.stack_push(bus, val)?;
                self.set_nz_flags(val, 0);
                self.set_c_flag(false);
                self.set_v_flag(false);
            }
            PUSHW => {
                let val = self.read_op(bus, 0)?;
                self.stack_push(bus, val)?;
                self.set_nz_flags(val, 0);
                self.set_c_flag(false);
                self.set_v_flag(false);
            }
            RESTORE => {
                let a = self.r[R_FP] - 28;
                let b = bus.read_word(a as usize, AccessCode::AddressFetch)?;
                let mut c = self.r[R_FP] - 24;

                let mut r = match self.ir.operands[0].register {
                    Some(r) => r,
                    None => return Err(CpuError::Exception(CpuException::IllegalOpcode)),
                };

                while r < R_FP {
                    self.r[r] = bus.read_word(c as usize, AccessCode::AddressFetch)?;
                    r += 1;
                    c += 4;
                }

                self.r[R_FP] = b;
                self.r[R_SP] = a;
            }
            RGEQ => {
                if !self.n_flag() || self.z_flag() {
                    self.r[R_PC] = self.stack_pop(bus)?;
                    pc_increment = 0;
                }
            }
            RGEQU => {
                if !self.c_flag() {
                    self.r[R_PC] = self.stack_pop(bus)?;
                    pc_increment = 0;
                }
            }
            RGTR => {
                if !self.n_flag() && !self.z_flag() {
                    self.r[R_PC] = self.stack_pop(bus)?;
                    pc_increment = 0;
                }
            }
            RNEQ | RNEQU => {
                if !self.z_flag() {
                    self.r[R_PC] = self.stack_pop(bus)?;
                    pc_increment = 0;
                }
            }
            RLEQ => {
                if self.n_flag() || self.z_flag() {
                    self.r[R_PC] = self.stack_pop(bus)?;
                    pc_increment = 0;
                }
            }
            RLEQU => {
                if self.c_flag() || self.z_flag() {
                    self.r[R_PC] = self.stack_pop(bus)?;
                    pc_increment = 0;
                }
            }
            RLSS => {
                if self.n_flag() || !self.z_flag() {
                    self.r[R_PC] = self.stack_pop(bus)?;
                    pc_increment = 0;
                }
            }
            REQL | REQLU => {
                if self.z_flag() {
                    self.r[R_PC] = self.stack_pop(bus)?;
                    pc_increment = 0;
                }
            }
            RSB => {
                self.r[R_PC] = self.stack_pop(bus)?;
                pc_increment = 0;
            }
            RET => {
                let a = self.r[R_AP];
                let b = bus.read_word((self.r[R_SP] - 4) as usize, AccessCode::AddressFetch)?;
                let c = bus.read_word((self.r[R_SP] - 8) as usize, AccessCode::AddressFetch)?;

                self.r[R_AP] = b;
                self.r[R_PC] = c;
                self.r[R_SP] = a;

                pc_increment = 0;
            }
            RETPS => {
                match self.priv_level() {
                    CpuLevel::Kernel => {
                        let new_pcbp = self.irq_pop(bus)?;
                        let new_psw = bus.read_word(new_pcbp as usize, AccessCode::AddressFetch)?;
                        self.r[R_PSW] &= !F_R;
                        self.r[R_PSW] |= new_psw & F_R;

                        self.context_switch_2(bus, new_pcbp)?;
                        self.context_switch_3(bus)?;

                        if self.r[R_PSW] & F_R != 0 {
                            self.r[R_FP] = bus.read_word((new_pcbp + 24) as usize, AccessCode::AddressFetch)?;
                            self.r[0] = bus.read_word((new_pcbp + 28) as usize, AccessCode::AddressFetch)?;
                            self.r[1] = bus.read_word((new_pcbp + 32) as usize, AccessCode::AddressFetch)?;
                            self.r[2] = bus.read_word((new_pcbp + 36) as usize, AccessCode::AddressFetch)?;
                            self.r[3] = bus.read_word((new_pcbp + 40) as usize, AccessCode::AddressFetch)?;
                            self.r[4] = bus.read_word((new_pcbp + 44) as usize, AccessCode::AddressFetch)?;
                            self.r[5] = bus.read_word((new_pcbp + 48) as usize, AccessCode::AddressFetch)?;
                            self.r[6] = bus.read_word((new_pcbp + 52) as usize, AccessCode::AddressFetch)?;
                            self.r[7] = bus.read_word((new_pcbp + 56) as usize, AccessCode::AddressFetch)?;
                            self.r[8] = bus.read_word((new_pcbp + 60) as usize, AccessCode::AddressFetch)?;
                            self.r[R_AP] = bus.read_word((new_pcbp + 20) as usize, AccessCode::AddressFetch)?;
                        }

                        pc_increment = 0;
                    },
                    _ => return Err(CpuError::Exception(CpuException::PrivilegedOpcode)),
                }
            }
            SAVE => {
                bus.write_word(self.r[R_SP] as usize, self.r[R_FP])?;

                let mut r = match self.ir.operands[0].register {
                    Some(r) => r,
                    None => return Err(CpuError::Exception(CpuException::IllegalOpcode)),
                };

                let mut stack_offset = 4;

                while r < R_FP {
                    bus.write_word(self.r[R_SP] as usize + stack_offset, self.r[r])?;
                    r += 1;
                    stack_offset += 4;
                }

                self.r[R_SP] = self.r[R_SP] + 28;
                self.r[R_FP] = self.r[R_SP];
            }
            SUBW2 | SUBH2 | SUBB2 => {
                let a = self.read_op(bus, 1)?;
                let b = self.read_op(bus, 0)?;
                self.sub(bus, a, b, 1)?;
            }
            SUBW3 | SUBH3 | SUBB3 => {
                let a = self.read_op(bus, 1)?;
                let b = self.read_op(bus, 0)?;
                self.sub(bus, a, b, 2)?;
            }
            TSTW => {
                let a = self.read_op(bus, 0)?;
                self.set_n_flag((a as i32) < 0);
                self.set_z_flag(a == 0);
                self.set_c_flag(false);
                self.set_v_flag(false);
            }
            TSTH => {
                let a = self.read_op(bus, 0)?;
                self.set_n_flag((a as i16) < 0);
                self.set_z_flag(a == 0);
                self.set_c_flag(false);
                self.set_v_flag(false);
            }
            TSTB => {
                let a = self.read_op(bus, 0)?;
                self.set_n_flag((a as i8) < 0);
                self.set_z_flag(a == 0);
                self.set_c_flag(false);
                self.set_v_flag(false);
            }
            XORW2 | XORH2 | XORB2 => {
                let a = self.read_op(bus, 0)?;
                let b = self.read_op(bus, 1)?;

                let result = a ^ b;

                self.write_op(bus, 1, result)?;
                self.set_nz_flags(result, 1);
                self.set_c_flag(false);
                self.set_v_flag_op(result, 1);
            }
            XORW3 | XORH3 | XORB3 => {
                let a = self.read_op(bus, 0)?;
                let b = self.read_op(bus, 1)?;

                let result = a ^ b;

                self.write_op(bus, 2, result)?;
                self.set_nz_flags(result, 2);
                self.set_c_flag(false);
                self.set_v_flag_op(result, 2);
            }
            _ => {
                return Err(CpuError::Exception(CpuException::IllegalOpcode));
            }
        };

        Ok(pc_increment)
    }

    /// Step the CPU by one instruction.
    pub fn step(&mut self, bus: &mut Bus) {
        // TODO: On CPU Exception or Bus Error, handle each error with the appropriate exception handler routine
        match self.dispatch(bus) {
            Ok(i) => self.r[R_PC] = (self.r[R_PC] as i32 + i) as u32,
            Err(CpuError::Bus(BusError::Alignment)) => {}
            Err(CpuError::Bus(BusError::Permission)) => {}
            Err(CpuError::Bus(BusError::NoDevice(_)))
            | Err(CpuError::Bus(BusError::Read(_)))
            | Err(CpuError::Bus(BusError::Write(_))) => {}
            Err(CpuError::Exception(CpuException::IllegalOpcode)) => {}
            Err(CpuError::Exception(CpuException::InvalidDescriptor)) => {}
            Err(CpuError::Exception(CpuException::PrivilegedOpcode)) => {}
            Err(_) => {}
        }
    }

    pub fn step_with_error(&mut self, bus: &mut Bus) -> Result<(), CpuError> {
        match self.dispatch(bus) {
            Ok(i) => self.r[R_PC] = (self.r[R_PC] as i32 + i) as u32,
            Err(e) => return Err(e),
        }

        Ok(())
    }

    /// Set the CPU's Program Counter to the specified value
    pub fn set_pc(&mut self, val: u32) {
        self.r[R_PC] = val;
    }

    fn set_operand(
        &mut self,
        index: usize,
        size: u8,
        mode: AddrMode,
        data_type: Data,
        expanded_type: Option<Data>,
        register: Option<usize>,
        embedded: u32
    ) {
        self.ir.operands[index].size = size;
        self.ir.operands[index].mode = mode;
        self.ir.operands[index].data_type = data_type;
        self.ir.operands[index].expanded_type = expanded_type;
        self.ir.operands[index].register = register;
        self.ir.operands[index].embedded = embedded;
    }

    /// Decode a literal Operand type.
    ///
    /// These operands belong to only certain instructions, where a word without
    /// a descriptor byte immediately follows the opcode.
    fn decode_literal_operand(&mut self, bus: &mut Bus, index: usize, mn: &Mnemonic, addr: usize) -> Result<(), CpuError> {
        match mn.dtype {
            Data::Byte => {
                let b: u8 = bus.read_byte(addr, AccessCode::OperandFetch)?;
                self.set_operand(index, 1, AddrMode::None, Data::Byte, None, None, b as u32);
            }
            Data::Half => {
                let h: u16 = bus.read_op_half(addr)?;
                self.set_operand(index, 2, AddrMode::None, Data::Half, None, None, h as u32);
            }
            Data::Word => {
                let w: u32 = bus.read_op_word(addr)?;
                self.set_operand(index, 4, AddrMode::None, Data::Word, None, None, w);
            }
            _ => return Err(CpuError::Exception(CpuException::IllegalOpcode)),
        }

        Ok(())
    }

    /// Decode a descriptor Operand type.
    fn decode_descriptor_operand(
        &mut self,
        bus: &mut Bus,
        index: usize,
        dtype: Data,
        etype: Option<Data>,
        addr: usize,
        recur: bool,
    ) -> Result<(), CpuError> {
        let descriptor_byte: u8 = bus.read_byte(addr, AccessCode::OperandFetch)?;

        let m = (descriptor_byte & 0xf0) >> 4;
        let r = descriptor_byte & 0xf;

        // The descriptor is either 1 or 2 bytes, depending on whether this is a recursive
        // call or not.
        let dsize = if recur {
            2
        } else {
            1
        };

        match m {
            0 | 1 | 2 | 3 => {
                // Positive Literal
                self.set_operand(index, dsize, AddrMode::PositiveLiteral, dtype, etype, None, descriptor_byte as u32);
            }
            4 => {
                match r {
                    15 => {
                        // Word Immediate
                        let w = bus.read_op_word(addr + 1)?;
                        self.set_operand(index, dsize + 4, AddrMode::WordImmediate, dtype, etype, None, w);
                    }
                    _ => {
                        // Register
                        self.set_operand(index, dsize, AddrMode::Register, dtype, etype, Some(r as usize), 0);
                    }
                }
            }
            5 => {
                match r {
                    15 => {
                        // Halfword Immediate
                        let h = bus.read_op_half(addr + 1)?;
                        self.set_operand(index, dsize + 2, AddrMode::HalfwordImmediate, dtype, etype, None, h as u32);
                    }
                    11 => {
                        // Illegal
                        return Err(CpuError::Exception(CpuException::IllegalOpcode))
                    }
                    _ => {
                        // Register Deferred Mode
                        self.set_operand(index, dsize, AddrMode::RegisterDeferred, dtype, etype, Some(r as usize), 0);
                    }
                }
            }
            6 => {
                match r {
                    15 => {
                        // Byte Immediate
                        let b = bus.read_byte(addr + 1, AccessCode::OperandFetch)?;
                        self.set_operand(index, dsize + 1, AddrMode::ByteImmediate, dtype, etype, None, b as u32);
                    }
                    _ => {
                        // FP Short Offset
                        self.set_operand(index, dsize, AddrMode::FPShortOffset, dtype, etype, Some(R_FP), r as u32);
                    }
                }
            }
            7 => {
                match r {
                    15 => {
                        // Absolute
                        let w = bus.read_op_word(addr + 1)?;
                        self.set_operand(index, dsize + 4, AddrMode::Absolute, dtype, etype, None, w);
                    }
                    _ => {
                        // AP Short Offset
                        self.set_operand(index, dsize, AddrMode::APShortOffset, dtype, etype, Some(R_AP), r as u32);
                    }
                }
            }
            8 => {
                match r {
                    11 => return Err(CpuError::Exception(CpuException::IllegalOpcode)),
                    _ => {
                        // Word Displacement
                        let disp = bus.read_op_word(addr + 1)?;
                        self.set_operand(index, dsize + 4, AddrMode::WordDisplacement, dtype, etype, Some(r as usize), disp);
                    }
                }
            }
            9 => {
                match r {
                    11 => return Err(CpuError::Exception(CpuException::IllegalOpcode)),
                    _ => {
                        // Word Displacement Deferred
                        let disp = bus.read_op_word(addr + 1)?;
                        self.set_operand(index, dsize + 4, AddrMode::WordDisplacementDeferred, dtype, etype, Some(r as usize), disp);
                    }
                }
            }
            10 => {
                match r {
                    11 => return Err(CpuError::Exception(CpuException::IllegalOpcode)),
                    _ => {
                        // Halfword Displacement
                        let disp = bus.read_op_half(addr + 1)?;
                        self.set_operand(index, dsize + 2, AddrMode::HalfwordDisplacement, dtype, etype, Some(r as usize), disp as u32);
                    }
                }
            }
            11 => {
                match r {
                    11 => return Err(CpuError::Exception(CpuException::IllegalOpcode)),
                    _ => {
                        // Halfword Displacement Deferred
                        let disp = bus.read_op_half(addr + 1)?;
                        self.set_operand(
                            index,
                            dsize + 2,
                            AddrMode::HalfwordDisplacementDeferred,
                            dtype,
                            etype,
                            Some(r as usize),
                            disp as u32,
                        );
                    }
                }
            }
            12 => {
                match r {
                    11 => return Err(CpuError::Exception(CpuException::IllegalOpcode)),
                    _ => {
                        // Byte Displacement
                        let disp = bus.read_byte(addr + 1, AccessCode::OperandFetch)?;
                        self.set_operand(index, dsize + 1, AddrMode::ByteDisplacement, dtype, etype, Some(r as usize), disp as u32);
                    }
                }
            }
            13 => {
                match r {
                    11 => return Err(CpuError::Exception(CpuException::IllegalOpcode)),
                    _ => {
                        // Byte Displacement Deferred
                        let disp = bus.read_byte(addr + 1, AccessCode::OperandFetch)?;
                        self.set_operand(index, dsize + 1, AddrMode::ByteDisplacementDeferred, dtype, etype, Some(r as usize), disp as u32);
                    }
                }
            }
            14 => match r {
                0 => self.decode_descriptor_operand(bus, index, dtype, Some(Data::UWord), addr + 1, true)?,
                2 => self.decode_descriptor_operand(bus, index, dtype, Some(Data::UHalf), addr + 1, true)?,
                3 => self.decode_descriptor_operand(bus, index, dtype, Some(Data::Byte), addr + 1, true)?,
                4 => self.decode_descriptor_operand(bus, index, dtype, Some(Data::Word), addr + 1, true)?,
                6 => self.decode_descriptor_operand(bus, index, dtype, Some(Data::Half), addr + 1, true)?,
                7 => self.decode_descriptor_operand(bus, index, dtype, Some(Data::SByte), addr + 1, true)?,
                15 => {
                    let w = bus.read_op_word(addr + 1)?;
                    self.set_operand(index, dsize + 4, AddrMode::AbsoluteDeferred, dtype, etype, None, w);
                }
                _ => { return Err(CpuError::Exception(CpuException::IllegalOpcode)); }
            },
            15 => {
                // Negative Literal
                self.set_operand(index, 1, AddrMode::NegativeLiteral, dtype, etype, None, descriptor_byte as u32);
            },
            _ => { return Err(CpuError::Exception(CpuException::IllegalOpcode)); }
        };

        Ok(())
    }

    /// Fully decode an Operand
    fn decode_operand(
        &mut self,
        bus: &mut Bus,
        index: usize,
        mn: &Mnemonic,
        ot: &OpType,
        etype: Option<Data>,
        addr: usize,
    ) -> Result<(), CpuError> {
        match *ot {
            OpType::Lit => self.decode_literal_operand(bus, index, mn, addr),
            OpType::Src | OpType::Dest => self.decode_descriptor_operand(bus, index, mn.dtype, etype, addr, false),
        }
    }

    /// Decode the instruction currently pointed at by the Program Counter.
    /// Returns the number of bytes consumed, or a CpuError.
    fn decode_instruction(&mut self, bus: &mut Bus) -> Result<(), CpuError> {
        // The next address to read from is pointed to by the PC
        let mut addr = self.r[R_PC] as usize;
        let initial_addr = addr;

        // Read the first byte of the instruction. Most instructions are only
        // one byte, so this is usually enough.
        let b1 = bus.read_byte(addr, AccessCode::InstrFetch)?;
        addr += 1;

        // Map the Mnemonic to the  opcode we just read. But there's a special
        // case if the value we read was '0x30'. This indicates that the instruction
        // we're reading is a halfword, requiring two bytes.
        let index: u16 = if b1 == 0x30 {
            // Special case for half-word opcodes
            let b2 = bus.read_byte(addr, AccessCode::InstrFetch)?;
            addr += 1;
            ((b1 as u16) << 8) | b2 as u16
        } else {
            b1 as u16
        };

        let mn = MNEMONICS.get(&index);

        // If we found a valid mnemonic, read in and decode all of its operands.
        // Otherwise, we must return a CpuException::IllegalOpcode
        match mn {
            Some(mn) => {
                let mut etype: Option<Data> = None;

                for (index, ot) in mn.ops.iter().enumerate() {
                    // Push a decoded operand
                    self.decode_operand(bus, index, mn, ot, etype, addr)?;
                    etype = self.ir.operands[index].expanded_type;
                    addr += self.ir.operands[index].size as usize;
                }

                let total_bytes = addr - initial_addr;

                self.ir.opcode = mn.opcode;
                self.ir.name = mn.name;
                self.ir.data_type = mn.dtype;
                self.ir.bytes = total_bytes as u8;
                self.ir.operand_count = mn.ops.len() as u8;
            }
            None => return Err(CpuError::Exception(CpuException::IllegalOpcode)),
        }

        return Ok(())
    }

    /// Convenience operations on flags.
    fn set_v_flag_op(&mut self, val: u32, index: usize) {
        match self.ir.operands[index].data_type {
            Data::Word | Data::UWord => self.set_v_flag(false),
            Data::Half | Data::UHalf => self.set_v_flag(val > 0xffff),
            Data::Byte | Data::SByte => self.set_v_flag(val > 0xff),
            Data::None => {
                // Intentionally ignored
            }
        }
    }

    fn set_nz_flags(&mut self, val: u32, index: usize) {
        match self.ir.operands[index].data_type {
            Data::Word | Data::UWord => {
                self.set_n_flag((val & 0x80000000) != 0);
                self.set_z_flag(val == 0);
            }
            Data::Half | Data::UHalf => {
                self.set_n_flag((val & 0x8000) != 0);
                self.set_z_flag((val & 0xffff) == 0);
            }
            Data::Byte | Data::SByte => {
                self.set_n_flag((val & 0x80) != 0);
                self.set_z_flag((val & 0xff) == 0);
            }
            Data::None => {
                // Intentionally ignored
            }
        }
    }

    fn set_c_flag(&mut self, set: bool) {
        if set {
            self.r[R_PSW] |= F_C;
        } else {
            self.r[R_PSW] &= !F_C;
        }
    }

    fn c_flag(&self) -> bool {
        ((self.r[R_PSW] & F_C) >> 18) == 1
    }

    fn set_v_flag(&mut self, set: bool) {
        if set {
            self.r[R_PSW] |= F_V;
        } else {
            self.r[R_PSW] &= !F_V;
        }
    }

    fn v_flag(&self) -> bool {
        ((self.r[R_PSW] & F_V) >> 19) == 1
    }

    fn set_z_flag(&mut self, set: bool) {
        if set {
            self.r[R_PSW] |= F_Z;
        } else {
            self.r[R_PSW] &= !F_Z;
        }
    }

    fn z_flag(&self) -> bool {
        ((self.r[R_PSW] & F_Z) >> 20) == 1
    }

    fn set_n_flag(&mut self, set: bool) {
        if set {
            self.r[R_PSW] |= F_N;
        } else {
            self.r[R_PSW] &= !F_N;
        }
    }

    fn n_flag(&self) -> bool {
        ((self.r[R_PSW] & F_N) >> 21) == 1
    }

    pub fn set_isc(&mut self, val: u32) {
        self.r[R_PSW] &= !F_ISC; // Clear existing value
        self.r[R_PSW] |= (val & 0xf) << 3; // Set new value
    }

    pub fn set_priv_level(&mut self, level: CpuLevel) {
        let val = match level {
            CpuLevel::Kernel => 0,
            CpuLevel::Executive => 1,
            CpuLevel::Supervisor => 2,
            CpuLevel::User => 3,
        };
        let old_level = (self.r[R_PSW] & F_CM) >> 11;
        self.r[R_PSW] &= !F_PM; // Clear PM
        self.r[R_PSW] |= (old_level & 3) << 9; // Set PM
        self.r[R_PSW] &= !F_CM; // Clear CM
        self.r[R_PSW] |= (val & 3) << 11; // Set CM
    }

    pub fn priv_level(&self) -> CpuLevel {
        let cm = ((self.r[R_PSW] & F_CM) >> 11) & 3;
        match cm {
            0 => CpuLevel::Kernel,
            1 => CpuLevel::Executive,
            2 => CpuLevel::Supervisor,
            3 | _ => CpuLevel::User,
        }
    }

    pub fn stack_push(&mut self, bus: &mut Bus, val: u32) -> Result<(), CpuError> {
        bus.write_word(self.r[R_SP] as usize, val)?;
        self.r[R_SP] += 4;
        Ok(())
    }

    pub fn stack_pop(&mut self, bus: &mut Bus) -> Result<u32, CpuError> {
        let result = bus.read_word((self.r[R_SP] - 4) as usize, AccessCode::AddressFetch)?;
        self.r[R_SP] -= 4;
        Ok(result)
    }

    pub fn irq_push(&mut self, bus: &mut Bus, val: u32) -> Result<(), CpuError> {
        bus.write_word(self.r[R_ISP] as usize, val)?;
        self.r[R_ISP] += 4;
        Ok(())
    }

    pub fn irq_pop(&mut self, bus: &mut Bus) -> Result<u32, CpuError> {
        self.r[R_ISP] -= 4;
        let result = bus.read_word((self.r[R_ISP]) as usize, AccessCode::AddressFetch)?;
        Ok(result)
    }

    pub fn get_pc(&self) -> u32 {
        self.r[R_PC]
    }

    pub fn get_ap(&self) -> u32 {
        self.r[R_AP]
    }

    pub fn get_psw(&self) -> u32 {
        self.r[R_PSW]
    }

    pub fn get_steps(&self) -> u64 {
        self.steps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::Bus;

    const BASE: usize = 0x700000;

    /// Helper function to set up and prepare a cpu and bus
    /// with a supplied program.
    fn do_with_program<F>(program: &[u8], test: F)
    where
        F: Fn(&mut Cpu, &mut Bus),
    {
        let mut cpu: Cpu = Cpu::new();
        let mut bus: Bus = Bus::new(0x10000);

        bus.load(BASE, &program).unwrap();
        cpu.r[R_PC] = BASE as u32;

        test(&mut cpu, &mut bus);
    }

    #[test]
    fn sign_extension() {
        assert_eq!(0xffff8000, sign_extend_halfword(0x8000));
        assert_eq!(0xffffff80, sign_extend_byte(0x80));
    }

    #[test]
    fn can_set_and_clear_nzvc_flags() {
        let mut cpu = Cpu::new();
        cpu.set_c_flag(true);
        assert_eq!(cpu.r[R_PSW], F_C);
        cpu.set_v_flag(true);
        assert_eq!(cpu.r[R_PSW], F_C | F_V);
        cpu.set_z_flag(true);
        assert_eq!(cpu.r[R_PSW], F_C | F_V | F_Z);
        cpu.set_n_flag(true);
        assert_eq!(cpu.r[R_PSW], F_C | F_V | F_Z | F_N);
        cpu.set_c_flag(false);
        assert_eq!(cpu.r[R_PSW], F_V | F_Z | F_N);
        cpu.set_v_flag(false);
        assert_eq!(cpu.r[R_PSW], F_Z | F_N);
        cpu.set_z_flag(false);
        assert_eq!(cpu.r[R_PSW], F_N);
        cpu.set_n_flag(false);
        assert_eq!(cpu.r[R_PSW], 0);
    }

    #[test]
    fn can_set_isc_flag() {
        let mut cpu = Cpu::new();

        for i in 0..15 {
            cpu.set_isc(i);
            assert_eq!(i << 3, cpu.r[R_PSW]);
        }

        cpu.set_isc(16); // Out of range, should fail
        assert_eq!(0, cpu.r[R_PSW]);
    }

    #[test]
    fn decodes_byte_literal_operand() {
        let program: [u8; 2] = [0x4f, 0x06]; // BLEB 0x6

        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_literal_operand(&mut bus, 0, MNEMONICS.get(&0x4F).unwrap(), BASE + 1).unwrap();
            assert_eq!(cpu.ir.operands[0], Operand::new(1, AddrMode::None, Data::Byte, None, None, 6));
        })
    }

    #[test]
    fn decodes_halfword_literal_operand() {
        let program: [u8; 3] = [0x4e, 0xff, 0x0f]; // BLEH 0xfff

        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_literal_operand(&mut bus, 0, MNEMONICS.get(&0x4e).unwrap(), BASE + 1).unwrap();
            assert_eq!(cpu.ir.operands[0], Operand::new(2, AddrMode::None, Data::Half, None, None, 0xfff));
        })
    }

    #[test]
    fn decodes_word_literal_operand() {
        let program: [u8; 5] = [0x32, 0xff, 0x4f, 0x00, 0x00]; // SPOP 0x4fff

        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_literal_operand(&mut bus, 0, MNEMONICS.get(&0x32).unwrap(), BASE + 1).unwrap();
            assert_eq!(cpu.ir.operands[0], Operand::new(4, AddrMode::None, Data::Word, None, None, 0x4fff));
        });
    }

    #[test]
    fn decodes_positive_literal_operand() {
        let program: [u8; 3] = [0x87, 0x04, 0x44]; // MOVB &4,%r4

        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 1, false).unwrap();
            assert_eq!(cpu.ir.operands[0], Operand::new(1, AddrMode::PositiveLiteral, Data::Byte, None, None, 0x04));
        });
    }

    #[test]
    fn decodes_word_immediate_operand() {
        let program = [0x84, 0x4f, 0x78, 0x56, 0x34, 0x12, 0x43]; // MOVW &0x12345678,%r3

        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Word, None, BASE + 1, false).unwrap();
            assert_eq!(cpu.ir.operands[0], Operand::new(5, AddrMode::WordImmediate, Data::Word, None, None, 0x12345678));
        });
    }

    #[test]
    fn decodes_register_operand() {
        let program: [u8; 3] = [0x87, 0x04, 0x44]; // MOVB &4,%r4

        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 2, false).unwrap();
            assert_eq!(cpu.ir.operands[0], Operand::new(1, AddrMode::Register, Data::Byte, None, Some(4), 0));
        });
    }

    #[test]
    fn decodes_halfword_immediate_operand() {
        let program = [0x84, 0x5f, 0x34, 0x12, 0x42]; // MOVW &0x1234,%r2

        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Word, None, BASE + 1, false).unwrap();
            assert_eq!(cpu.ir.operands[0], Operand::new(3, AddrMode::HalfwordImmediate, Data::Word, None, None, 0x1234,));
        });
    }

    #[test]
    fn decodes_register_deferred_operand() {
        let program: [u8; 3] = [0x86, 0x52, 0x41]; // MOVH (%r2),%r1

        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Half, None, BASE + 1, false).unwrap();
            assert_eq!(cpu.ir.operands[0], Operand::new(1, AddrMode::RegisterDeferred, Data::Half, None, Some(2), 0));
        });
    }

    #[test]
    fn decodes_byte_immediate_operand() {
        let program: [u8; 4] = [0x84, 0x6f, 0x28, 0x46]; // MOVW &40,%r6

        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Word, None, BASE + 1, false).unwrap();
            assert_eq!(cpu.ir.operands[0], Operand::new(2, AddrMode::ByteImmediate, Data::Word, None, None, 40));
        });
    }

    #[test]
    fn decodes_fp_short_offset_operand() {
        let program: [u8; 3] = [0x84, 0x6C, 0x40]; // MOVW 12(%fp),%r0

        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Word, None, BASE + 1, false).unwrap();
            assert_eq!(cpu.ir.operands[0], Operand::new(1, AddrMode::FPShortOffset, Data::Word, None, Some(R_FP), 12));
        });
    }

    #[test]
    fn decodes_absolute_operand() {
        let program: [u8; 7] = [0x87, 0x7f, 0x00, 0x01, 0x00, 0x00, 0x40]; // MOVB $0x100, %r0

        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 1, false).unwrap();
            assert_eq!(cpu.ir.operands[0], Operand::new(5, AddrMode::Absolute, Data::Byte, None, None, 0x00000100));
        });
    }

    #[test]
    fn decodes_absolute_deferred_operand() {
        let program = [0x87, 0xef, 0x00, 0x01, 0x00, 0x00, 0x40]; // MOVB *$0x100,%r0

        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 1, false).unwrap();
            assert_eq!(cpu.ir.operands[0], Operand::new(5, AddrMode::AbsoluteDeferred, Data::Byte, None, None, 0x00000100));
        });
    }

    #[test]
    fn decodes_ap_short_offset_operand() {
        let program: [u8; 3] = [0x84, 0x74, 0x43]; // MOVW 4(%ap),%r3

        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Word, None, BASE + 1, false).unwrap();
            assert_eq!(cpu.ir.operands[0], Operand::new(1, AddrMode::APShortOffset, Data::Word, None, Some(R_AP), 4));
        });
    }

    #[test]
    fn decodes_word_displacement_operand() {
        let program: [u8; 7] = [0x87, 0x82, 0x34, 0x12, 0x00, 0x00, 0x44]; // MOVB 0x1234(%r2),%r4

        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 1, false).unwrap();
            assert_eq!(cpu.ir.operands[0], Operand::new(5, AddrMode::WordDisplacement, Data::Byte, None, Some(2), 0x1234,));
        });
    }

    #[test]
    fn decodes_word_displacement_deferred_operand() {
        let program: [u8; 7] = [0x87, 0x92, 0x50, 0x40, 0x00, 0x00, 0x40]; // MOVB *0x4050(%r2),%r0

        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 1, false).unwrap();
            assert_eq!(cpu.ir.operands[0], Operand::new(5, AddrMode::WordDisplacementDeferred, Data::Byte, None, Some(2), 0x4050,));
        });
    }

    #[test]
    fn decodes_halfword_displacement_operand() {
        let program: [u8; 5] = [0x87, 0xa2, 0x34, 0x12, 0x44]; // MOVB 0x1234(%r2),%r4

        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 1, false).unwrap();
            assert_eq!(cpu.ir.operands[0], Operand::new(3, AddrMode::HalfwordDisplacement, Data::Byte, None, Some(2), 0x1234,));
        });
    }

    #[test]
    fn decodes_halfword_displacement_deferred_operand() {
        let program: [u8; 5] = [0x87, 0xb2, 0x50, 0x40, 0x40]; // MOVB *0x4050(%r2),%r0

        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 1, false).unwrap();
            assert_eq!(cpu.ir.operands[0], Operand::new(3, AddrMode::HalfwordDisplacementDeferred, Data::Byte, None, Some(2), 0x4050,));
        });
    }

    #[test]
    fn decodes_byte_displacement_operand() {
        let program: [u8; 4] = [0x87, 0xc1, 0x06, 0x40]; // MOVB 6(%r1),%r0

        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 1, false).unwrap();
            assert_eq!(cpu.ir.operands[0], Operand::new(2, AddrMode::ByteDisplacement, Data::Byte, None, Some(1), 6));
        });
    }

    #[test]
    fn decodes_byte_displacement_deferred_operand() {
        let program: [u8; 4] = [0x87, 0xd2, 0x30, 0x43]; // MOVB *0x30(%r2),%r3

        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 1, false).unwrap();
            assert_eq!(cpu.ir.operands[0], Operand::new(2, AddrMode::ByteDisplacementDeferred, Data::Byte, None, Some(2), 0x30));
        });
    }

    #[test]
    fn decodes_expanded_type_operand() {
        let program: [u8; 6] = [0x87, 0xe7, 0x40, 0xe2, 0xc1, 0x04]; // MOVB {sbyte}%r0,{uhalf}4(%r1)

        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 1, false).unwrap();
            cpu.decode_descriptor_operand(&mut bus, 1, Data::Byte, None, BASE + 3, false).unwrap();

            assert_eq!(cpu.ir.operands[0], Operand::new(2, AddrMode::Register, Data::Byte, Some(Data::SByte), Some(0), 0,));
            assert_eq!(cpu.ir.operands[1], Operand::new(3, AddrMode::ByteDisplacement, Data::Byte, Some(Data::UHalf), Some(1), 4,));
        });
    }

    #[test]
    fn decodes_negative_literal_operand() {
        let program: [u8; 3] = [0x87, 0xff, 0x40]; // MOVB &-1,%r0

        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 1, false).unwrap();
            assert_eq!(cpu.ir.operands[0], Operand::new(1, AddrMode::NegativeLiteral, Data::Byte, None, None, 0xff));
        });
    }

    fn assert_instruction(cpu: &Cpu, opcode: u16, size: u8, name: &'static str, data_type: Data, operand_count: u8) {
        assert_eq!(cpu.ir.opcode, opcode);
        assert_eq!(cpu.ir.bytes, size);
        assert_eq!(cpu.ir.name, name);
        assert_eq!(cpu.ir.data_type, data_type);
        assert_eq!(cpu.ir.operand_count, operand_count);
    }

    #[test]
    fn decodes_halfword_instructions() {
        let program = [0x30, 0x0d]; // ENBVJMP
        do_with_program(&program, |cpu, bus| {
            cpu.decode_instruction(bus).unwrap();
            assert_instruction(cpu, 0x300d, 2, "ENBVJMP", Data::None, 0);
        })
    }

    #[test]
    fn decodes_instructions() {
        let program: [u8; 10] = [
            0x87, 0xe7, 0x40, 0xe2, 0xc1, 0x04, // MOVB {sbyte}%r0,{uhalf}4(%r1)
            0x87, 0xd2, 0x30, 0x43, // MOVB *0x30(%r2),%r3
        ];

        do_with_program(&program, |cpu, bus| {
            {
                cpu.set_pc(BASE as u32);
                cpu.decode_instruction(bus).unwrap();
                let expected_operands = vec![
                    Operand::new(2, AddrMode::Register, Data::Byte, Some(Data::SByte), Some(0), 0),
                    Operand::new(3, AddrMode::ByteDisplacement, Data::Byte, Some(Data::UHalf), Some(1), 4),
                ];
                assert_instruction(cpu, 0x87, 6, "MOVB", Data::Byte, 2);
                assert_eq!(cpu.ir.operands[0], expected_operands[0]);
                assert_eq!(cpu.ir.operands[1], expected_operands[1]);
            }
            {
                cpu.set_pc((BASE + 6) as u32);
                cpu.decode_instruction(bus).unwrap();
                let expected_operands = vec![
                    Operand::new(2, AddrMode::ByteDisplacementDeferred, Data::Byte, None, Some(2), 0x30),
                    Operand::new(1, AddrMode::Register, Data::Byte, None, Some(3), 0),
                ];
                assert_instruction(cpu, 0x87, 4, "MOVB", Data::Byte, 2);
                assert_eq!(cpu.ir.operands[0], expected_operands[0]);
                assert_eq!(cpu.ir.operands[1], expected_operands[1]);
            }
        })
    }

    #[test]
    fn reads_register_operand_data() {
        {
            let program = [0x87, 0xe7, 0x40, 0xe2, 0x41]; // MOVB {sbyte}%r0,{uhalf}%r1
            do_with_program(&program, |cpu, mut bus| {
                cpu.r[0] = 0xff;
                cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 1, false).unwrap();
                assert_eq!(0xffffffff, cpu.read_op(bus, 0).unwrap());
            });
        }

        {
            let program = [0x87, 0x40, 0x41]; // MOVB %r0,%r1
            do_with_program(&program, |cpu, mut bus| {
                cpu.r[0] = 0xff;
                cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 1, false).unwrap();
                assert_eq!(0xff, cpu.read_op(bus, 0).unwrap());
            });
        }
    }

    #[test]
    fn reads_positive_literal_operand_data() {
        let program = [0x87, 0x04, 0x44];
        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 1, false).unwrap();
            assert_eq!(4, cpu.read_op(bus, 0).unwrap() as i8);
        });
    }

    #[test]
    fn reads_negative_literal_operand_data() {
        let program = [0x87, 0xff, 0x44];
        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 1, false).unwrap();
            assert_eq!(-1, cpu.read_op(bus, 0).unwrap() as i8);
        });
    }

    #[test]
    fn reads_word_immediate_operand_data() {
        let program = [0x84, 0x4f, 0x78, 0x56, 0x34, 0x12, 0x43]; // MOVW &0x12345678,%r3
        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Word, None, BASE + 1, false).unwrap();
            assert_eq!(0x12345678, cpu.read_op(bus, 0).unwrap())
        });
    }

    #[test]
    fn reads_halfword_immediate_operand_data() {
        let program = [0x84, 0x5f, 0x34, 0x12, 0x42]; // MOVW &0x1234,%r2
        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Word, None, BASE + 1, false).unwrap();
            assert_eq!(0x1234, cpu.read_op(bus, 0).unwrap())
        });
    }

    #[test]
    fn reads_negative_halfword_immediate_operand_data() {
        let program = [0x84, 0x5f, 0x00, 0x80, 0x42]; // MOVW &0x8000,%r2
        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Word, None, BASE + 1, false).unwrap();
            assert_eq!(0xffff8000, cpu.read_op(bus, 0).unwrap())
        });
    }

    #[test]
    fn reads_byte_immediate_operand_data() {
        let program = [0x84, 0x6f, 0x28, 0x42]; // MOVW &40,%r2
        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Word, None, BASE + 1, false).unwrap();
            assert_eq!(40, cpu.read_op(bus, 0).unwrap())
        });
    }

    #[test]
    fn reads_negative_byte_immediate_operand_data() {
        let program = [0x84, 0x6f, 0xff, 0x42]; // MOVW &-1,%r2
        do_with_program(&program, |cpu, mut bus| {
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Word, None, BASE + 1, false).unwrap();
            assert_eq!(-1, cpu.read_op(bus, 0).unwrap() as i32)
        });
    }

    #[test]
    fn reads_absolute_operand_data() {
        let program = [0x87, 0x7f, 0x00, 0x02, 0x70, 0x00, 0x04]; // MOVB $0x700200,%r0
        do_with_program(&program, |cpu, mut bus| {
            bus.write_byte(0x700200, 0x5a).unwrap();
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 1, false).unwrap();
            assert_eq!(0x5a, cpu.read_op(bus, 0).unwrap());
        });
    }

    #[test]
    fn reads_absolute_deferred_operand_data() {
        let program = [0x87, 0xef, 0x00, 0x01, 0x70, 0x00, 0x41]; // MOVB *$0x700100,%r0
        do_with_program(&program, |cpu, mut bus| {
            bus.write_word(0x700100, 0x700300).unwrap();
            bus.write_byte(0x700300, 0x1f).unwrap();
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 1, false).unwrap();
            assert_eq!(0x1f, cpu.read_op(bus, 0).unwrap());
        });
    }

    #[test]
    fn reads_byte_displacement_operand_data() {
        let program = [
            0x87, 0xc1, 0x06, 0x40, // MOVB 6(%r1),%r0
            0x87, 0xc1, 0xfe, 0x40, // MOVB -2(%r1),%r0
        ];
        do_with_program(&program, |cpu, mut bus| {
            cpu.r[1] = 0x700200;
            bus.write_byte(0x700206, 0x1f).unwrap();
            bus.write_byte(0x7001fe, 0xc5).unwrap();
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 1, false).unwrap();
            assert_eq!(0x1f, cpu.read_op(bus, 0).unwrap());
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 5, false).unwrap();
            assert_eq!(0xc5, cpu.read_op(bus, 0).unwrap());
        });
    }

    #[test]
    fn reads_byte_displacement_deferred_operand_data() {
        let program = [0x87, 0xd2, 0x30, 0x43]; // MOVB *0x30(%r2),%r3
        do_with_program(&program, |cpu, mut bus| {
            cpu.r[2] = 0x700200;
            bus.write_word(0x700230, 0x700300).unwrap();
            bus.write_byte(0x700300, 0x5a).unwrap();
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 1, false).unwrap();
            assert_eq!(0x5a, cpu.read_op(bus, 0).unwrap());
        })
    }

    #[test]
    fn reads_halword_displacement_operand_data() {
        let program = [0x87, 0xa2, 0x01, 0x11, 0x48]; // MOVB 0x1101(%r2),%r8
        do_with_program(&program, |cpu, mut bus| {
            cpu.r[2] = 0x700000;
            bus.write_byte(0x701101, 0x1f).unwrap();
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 1, false).unwrap();
            assert_eq!(0x1f, cpu.read_op(bus, 0).unwrap());
        });
    }

    #[test]
    fn reads_halfword_displacement_deferred_operand_data() {
        let program = [0x87, 0xb2, 0x00, 0x02, 0x46]; // MOVB *0x200(%r2),%r6
        do_with_program(&program, |cpu, mut bus| {
            cpu.r[2] = 0x700000;
            bus.write_word(0x700200, 0x700500).unwrap();
            bus.write_byte(0x700500, 0x5a).unwrap();
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 1, false).unwrap();
            assert_eq!(0x5a, cpu.read_op(bus, 0).unwrap());
        })
    }

    #[test]
    fn reads_word_displacement_operand_data() {
        let program = [0x87, 0x82, 0x01, 0x11, 0x00, 0x00, 0x48]; // MOVB 0x1101(%r2),%r8
        do_with_program(&program, |cpu, mut bus| {
            cpu.r[2] = 0x700000;
            bus.write_byte(0x701101, 0x1f).unwrap();
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 1, false).unwrap();
            assert_eq!(0x1f, cpu.read_op(bus, 0).unwrap());
        });
    }

    #[test]
    fn reads_word_displacement_deferred_operand_data() {
        let program = [0x87, 0x92, 0x00, 0x02, 0x00, 0x00, 0x46]; // MOVB *0x200(%r2),%r6
        do_with_program(&program, |cpu, mut bus| {
            cpu.r[2] = 0x700000;
            bus.write_word(0x700200, 0x700500).unwrap();
            bus.write_byte(0x700500, 0x5a).unwrap();
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 1, false).unwrap();
            assert_eq!(0x5a, cpu.read_op(bus, 0).unwrap());
        })
    }

    #[test]
    fn reads_ap_short_offset_operand_data() {
        let program = [0x84, 0x74, 0x43]; // MOVW 4(%ap),%r3
        do_with_program(&program, |cpu, mut bus| {
            cpu.r[R_AP] = 0x700500;
            bus.write_word(0x700504, 0x12345678).unwrap();
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Word, None, BASE + 1, false).unwrap();
            assert_eq!(0x12345678, cpu.read_op(bus, 0).unwrap());
        });
    }

    #[test]
    fn reads_fp_short_offset_operand_data() {
        let program = [0x84, 0x6c, 0x40]; // MOVW 12(%fp),%r0
        do_with_program(&program, |cpu, mut bus| {
            cpu.r[R_FP] = 0x700200;
            bus.write_word(0x70020c, 0x12345678).unwrap();
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Word, None, BASE + 1, false).unwrap();
            assert_eq!(0x12345678, cpu.read_op(bus, 0).unwrap());
        });
    }

    #[test]
    fn writes_register_operand_data() {
        let program = [0x40];
        do_with_program(&program, |cpu, mut bus| {
            cpu.r[0] = 0;
            cpu.decode_descriptor_operand(&mut bus, 0, Data::Byte, None, BASE + 0, false).unwrap();
            cpu.write_op(bus, 0, 0x5a).unwrap();
            assert_eq!(0x5a, cpu.r[0]);
        });
    }
}
