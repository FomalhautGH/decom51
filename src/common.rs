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
    // Sfr,
    xData,
    Code,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Operand {
    Register(Register),
    Address(u16, AddrSpace),
    Temporary(usize),
    Costant(u16),
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
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Ir51Type {
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
        ptr: Operand,
        value: Operand,
    },
    Load {
        space: AddrSpace,
        dest_ptr: Operand,
    },

    Jump(usize),
    BranchIf {
        cond: Operand,
        true_jmp: usize,
        false_jmp: usize,
    },
    Call(usize),
    Return,

    End,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Ir51 {
    pub index: usize,
    pub instr: Ir51Type,
}
