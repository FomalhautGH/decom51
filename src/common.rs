use std::{
    collections::HashMap,
    fmt::{Display, Formatter, Result},
};

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
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
#[derive(Debug, PartialEq, Clone, Copy)]
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

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Operand {
    Constant(Constant),

    Register(Register),

    Temporary(TempIndex),
    TempSSAVar { index: TempIndex, version: Version },

    Address(Addr, AddrSpace),
    SSAVar { addr: Addr, version: Version },

    Var(VarId),
    Concat(VarId, VarId),
}

impl Operand {
    pub fn extract_ssa_version(&self) -> Version {
        match self {
            Operand::SSAVar { version, .. } => *version,
            Operand::TempSSAVar { version, .. } => *version,

            Operand::Register(_)
            | Operand::Constant(_)
            | Operand::Temporary(_)
            | Operand::Address(_, _)
            | Operand::Var(_)
            | Operand::Concat(_, _) => unreachable!("{self:?}"),
        }
    }

    // TODO: Check every place i could use this function in
    pub fn to_ssavar(self) -> Option<SSAVar> {
        if let Operand::SSAVar { addr, version } = self {
            Some((addr, version))
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum BinOp {
    Add,
    Sub,
    Eq,
    Lt,
    Gt,
    Le,
    Ge,
    Ne,
    Or,
}

impl BinOp {
    pub fn is_math_op(self) -> bool {
        match self {
            BinOp::Add => true,
            BinOp::Sub => true,

            BinOp::Eq => false,
            BinOp::Lt => false,
            BinOp::Gt => false,
            BinOp::Le => false,
            BinOp::Ge => false,
            BinOp::Ne => false,
            BinOp::Or => false,
        }
    }

    pub fn is_glue_op(self) -> bool {
        match self {
            BinOp::Or => true,
            BinOp::Lt => true,

            BinOp::Add => false,
            BinOp::Sub => false,
            BinOp::Eq => false,
            BinOp::Gt => false,
            BinOp::Le => false,
            BinOp::Ge => false,
            BinOp::Ne => false,
        }
    }
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
}

pub type BlockId = usize;

// This type represents the predecessor blocks where the SSAVar comes from,
// SSAVar being represented by just its version since the address it's the same
// as in the operand. Edit: I changed to Operand insted for rewrite phase so
// TODO: fix the places where i extract the versione form the operand if can already
// take it from the incoming (do i still need phi.operand?)
pub type Incoming = HashMap<BlockId, Operand>;

#[derive(Debug, Clone)]
pub struct Phi {
    pub operand: Operand,
    pub incoming: Incoming,
}

pub type SSAVar = (Addr, Version);
pub type TempSSAVar = (TempIndex, Version);

// TODO: Maybe is it bettere to have SSAVar as a struct and implement PartialOrd
pub fn is_ssa_smaller(a: SSAVar, b: SSAVar) -> bool {
    if a.0 == b.0 { a.1 < b.1 } else { a.0 < b.0 }
}

// TODO: Maybe make addr a struct so that we can implement display/debug
pub fn format_addr(addr: Addr) -> String {
    match addr {
        0xE0 => "A".to_string(),
        0xF0 => "B".to_string(),
        0xD0 => "PSW".to_string(),
        0x81 => "SP".to_string(),
        0x82 => "DPL".to_string(),
        0x83 => "DPH".to_string(),
        0..=7 => format!("R{}", addr),
        _ => format!("iData[0x{:02X}]", addr),
    }
}

impl Display for Operand {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Operand::Register(reg) => write!(f, "{:?}", reg),
            Operand::Constant(c) => write!(f, "#0x{:02X}", c),
            Operand::Temporary(idx) => write!(f, "t{}", idx),
            Operand::TempSSAVar { index, version } => write!(f, "t{}_{}", index, version),
            Operand::Address(addr, space) => match space {
                AddrSpace::iData => write!(f, "{}", format_addr(*addr)),
                AddrSpace::xData => write!(f, "xData[0x{:04X}]", addr),
                AddrSpace::Code => write!(f, "code[0x{:04X}]", addr),
            },
            Operand::SSAVar { addr, version } => {
                write!(f, "{}_{}", format_addr(*addr), version)
            }
            Operand::Var(id) => write!(f, "v{}", id),
            Operand::Concat(low, high) => write!(f, "concat(v{}, v{})", low, high),
        }
    }
}

impl Display for BinOp {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            BinOp::Add => write!(f, "+"),
            BinOp::Sub => write!(f, "-"),
            BinOp::Eq => write!(f, "=="),
            BinOp::Lt => write!(f, "<"),
            BinOp::Gt => write!(f, ">"),
            BinOp::Le => write!(f, "<="),
            BinOp::Ge => write!(f, ">="),
            BinOp::Ne => write!(f, "!="),
            BinOp::Or => write!(f, "|"),
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
            Ir51::Jump(label) => write!(f, "goto Block_{}", label),
            Ir51::BranchIf {
                cond,
                true_jmp,
                false_jmp,
            } => {
                write!(
                    f,
                    "if ({}) goto Block_{} else goto Block_{}",
                    cond, true_jmp, false_jmp
                )
            }
            Ir51::Call(label) => write!(f, "call Block_{}", label),
            Ir51::Return => write!(f, "ret"),
            Ir51::Nop => write!(f, "nop"),
        }
    }
}

impl Display for Phi {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{} = phi {{ ", self.operand)?;
        let mut first = true;
        let mut keys: Vec<&BlockId> = self.incoming.keys().collect();
        keys.sort();
        for &k in keys {
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "Block_{}: {}", k, self.incoming[&k])?;
            first = false;
        }
        write!(f, " }}")
    }
}
