use std::collections::HashMap;

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

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Operand {
    Register(Register),

    Constant(Constant),

    Temporary(TempIndex),
    TempSSAVar { index: TempIndex, version: Version },

    Address(Addr, AddrSpace),
    SSAVar { addr: Addr, version: Version },
}

impl Operand {
    pub fn extract_ssa_version(&self) -> Version {
        match self {
            Operand::SSAVar { version, .. } => *version,
            Operand::TempSSAVar { version, .. } => *version,

            Operand::Register(_)
            | Operand::Constant(_)
            | Operand::Temporary(_)
            | Operand::Address(_, _) => unreachable!("{self:?}"),
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
    BorrowOut, // lhs < rhs
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

    Jump(usize),

    BranchIf {
        cond: Operand,
        true_jmp: usize,
        false_jmp: usize,
    },

    Call(usize),

    Return,

    Nop,
    InvalidIr,
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
