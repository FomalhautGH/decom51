use std::{
    collections::HashMap,
    fmt::{Display, Formatter, Result},
};

pub use crate::lir::{
    Addr, AddrSpace, BinOp, Constant, Ir51, Operand, Register, TempIndex, VarId, Version,
};

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
