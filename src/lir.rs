use crate::common::SSAVar;
use std::fmt::{Display, Formatter, Result};

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum Register {
    R0,
    R1,
    R2,
    R3,
    R4,
    R5,
    R6,
    R7,
    A,
    B,
    C,
    PSW,
    SP,
    DPTR,
    DPL,
    DPH,
    P0,
    P1,
    P2,
    P3,
    PCON,
    TCON,
    TMOD,
    TL0,
    TL1,
    TH0,
    TH1,
    SCON,
    SBUF,
    IE,
    IP,
}

#[allow(non_camel_case_types)]
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
// TODO: controllare la prensenza di altri spazi di memoria
pub enum AddrSpace {
    iData,
    xData, // <-- Not supported like Code as well
    // Sfr,
    Code,
}

pub type Addr = u16;
pub type Constant = u16;
pub type Version = usize;
pub type TempIndex = usize;
pub type VarId = usize;
pub type Size = usize;

#[derive(Debug, PartialEq, Clone, Copy, Eq, Hash)]
pub enum Operand {
    Constant(Constant),
    Register(Register),

    Temp(TempIndex),
    TempSSAVar {
        index: TempIndex,
        version: Version,
    },

    Var {
        addr: Addr,
        space: AddrSpace,
        size: Size,
    },

    Local {
        id: VarId,
        size: Size,
    },

    SSAVar {
        addr: Addr,
        version: Version,
        size: Size,
    },
}

impl Operand {
    pub fn extract_ssa_version(&self) -> Version {
        match self {
            Operand::SSAVar { version, .. } => *version,
            Operand::TempSSAVar { version, .. } => *version,

            Operand::Register(_)
            | Operand::Constant(_)
            | Operand::Temp(_)
            | Operand::Var { .. }
            | Operand::Local { .. } => unreachable!("{self:?}"),
        }
    }

    // TODO: Check every place i could use this function in
    pub fn to_ssavar(self) -> Option<SSAVar> {
        if let Operand::SSAVar {
            addr,
            version,
            size,
        } = self
        {
            Some((addr, version, size))
        } else {
            None
        }
    }

    pub fn to_var_addr(self) -> Option<Addr> {
        if let Operand::Var { addr, .. } = self {
            Some(addr)
        } else {
            None
        }
    }

    pub fn does_overlap(&self, rhs: &Operand) -> bool {
        if let (
            Operand::Var {
                addr: a1,
                space: s1,
                size: sz1,
            },
            Operand::Var {
                addr: a2,
                space: s2,
                size: sz2,
            },
        ) = (self, rhs)
            && s1 == s2
        {
            return *a1 < (*a2 + *sz2 as u16) && *a2 < (*a1 + *sz1 as u16);
        }

        self == rhs
    }
}

#[derive(Debug, PartialEq, Clone, Copy, Hash, Eq)]
pub enum BinOp {
    Add,
    Addc,
    Sub,
    Subb,
    Mul,
    Div,
    Mod,
    And,
    Or,
    Xor,
    Eq,
    Lt,
    Gt,
    Le,
    Ge,
    Ne,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Ir51 {
    Move {
        dest: Operand,
        src: Operand,
    },
    BinOp {
        op: BinOp,
        dest: Operand,
        lhs: Operand,
        rhs: Operand,
    },

    Store {
        space: AddrSpace,
        dest_ptr: Operand,
        value: Operand,
    },
    Load {
        space: AddrSpace,
        dest: Operand,
        src_ptr: Operand,
    },

    BranchIf {
        cond: Operand,
        true_jmp: usize,
        false_jmp: usize,
    },

    Jump(usize),

    Call(usize),
    Return,
    Nop,
}

impl Ir51 {
    pub fn is_flow_terminator(&self) -> bool {
        match self {
            Ir51::Jump(_) | Ir51::Return => true,

            Ir51::Move { .. }
            | Ir51::BinOp { .. }
            | Ir51::Store { .. }
            | Ir51::Load { .. }
            | Ir51::BranchIf { .. }
            | Ir51::Call(_)
            | Ir51::Nop => false,
        }
    }

    pub fn generate_push(src: Operand, size: u16) -> Vec<Self> {
        vec![
            Self::BinOp {
                op: BinOp::Add,
                dest: Operand::Register(Register::SP),
                lhs: Operand::Register(Register::SP),
                rhs: Operand::Constant(size),
            },
            Self::Load {
                space: AddrSpace::iData,
                dest: Operand::Temp(0),
                src_ptr: src,
            },
            Self::Store {
                space: AddrSpace::iData,
                dest_ptr: Operand::Register(Register::SP),
                value: Operand::Temp(0),
            },
        ]
    }

    pub fn generate_pop(dest: Operand, size: u16) -> Vec<Self> {
        vec![
            Self::Load {
                space: AddrSpace::iData,
                dest,
                src_ptr: Operand::Register(Register::SP),
            },
            Self::BinOp {
                op: BinOp::Sub,
                dest: Operand::Register(Register::SP),
                lhs: Operand::Register(Register::SP),
                rhs: Operand::Constant(size),
            },
        ]
    }
}

impl Display for Operand {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Operand::Register(reg) => write!(f, "{:?}", reg),
            Operand::Constant(c) => write!(f, "#0x{:02X}", c),
            Operand::Temp(idx) => write!(f, "t{}", idx),
            Operand::TempSSAVar { index, version } => write!(f, "t{}_{}", index, version),
            Operand::Var { addr, space, size } => match space {
                AddrSpace::iData => {
                    if *size > 1 {
                        write!(f, "{}[size {}]", format_addr(*addr), size)
                    } else {
                        write!(f, "{}", format_addr(*addr))
                    }
                }
                AddrSpace::xData => write!(f, "xData[0x{:04X}]", addr),
                AddrSpace::Code => write!(f, "code[0x{:04X}]", addr),
            },
            Operand::SSAVar { addr, version, .. } => {
                write!(f, "{}_{}", format_addr(*addr), version)
            }
            Operand::Local { id, .. } => write!(f, "v{}", id),
        }
    }
}

impl Display for BinOp {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            BinOp::Add => write!(f, "+"),
            BinOp::Addc => write!(f, "+c"),
            BinOp::Sub => write!(f, "-"),
            BinOp::Subb => write!(f, "-b"),
            BinOp::Mul => write!(f, "*"),
            BinOp::Div => write!(f, "/"),
            BinOp::Mod => write!(f, "%"),
            BinOp::And => write!(f, "&"),
            BinOp::Or => write!(f, "|"),
            BinOp::Xor => write!(f, "^"),
            BinOp::Eq => write!(f, "=="),
            BinOp::Lt => write!(f, "<"),
            BinOp::Gt => write!(f, ">"),
            BinOp::Le => write!(f, "<="),
            BinOp::Ge => write!(f, ">="),
            BinOp::Ne => write!(f, "!="),
        }
    }
}

impl Display for Ir51 {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Ir51::Move { dest, src } => write!(f, "{} = {}", dest, src),
            Ir51::BinOp { op, dest, lhs, rhs } => {
                write!(f, "{} = {} {} {}", dest, lhs, op, rhs)
            }
            Ir51::Store {
                space,
                dest_ptr,
                value,
            } => {
                write!(f, "*{:?}[{}] = {}", space, dest_ptr, value)
            }
            Ir51::Load {
                space,
                dest,
                src_ptr,
            } => {
                write!(f, "{} = *{:?}[{}]", dest, space, src_ptr)
            }
            Ir51::Jump(label) => write!(f, "jump Block_{}", label),
            Ir51::BranchIf {
                cond,
                true_jmp,
                false_jmp,
            } => {
                write!(
                    f,
                    "if ({}) jump Block_{} else jump Block_{}",
                    cond, true_jmp, false_jmp
                )
            }
            Ir51::Call(label) => write!(f, "call Block_{}", label),
            Ir51::Return => write!(f, "ret"),
            Ir51::Nop => write!(f, "nop"),
        }
    }
}

// TODO: Maybe make addr a struct so that we can implement display/debug
pub fn format_addr(addr: Addr) -> String {
    match addr {
        0xE0 => "A".to_string(),
        0xF0 => "B".to_string(),
        0xD7 => "C".to_string(),
        0xD0 => "PSW".to_string(),
        0x81 => "SP".to_string(),
        0x82 => "DPL".to_string(),
        0x83 => "DPH".to_string(),
        0..=7 => format!("R{}", addr),
        _ => format!("iData[0x{:02X}]", addr),
    }
}

// TODO: Check that the addresses are correct
pub fn is_iram_addr(addr: Addr) -> bool {
    (0x08..=0x7F).contains(&addr)
}
