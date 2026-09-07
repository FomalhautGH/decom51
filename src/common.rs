use std::{
    collections::HashMap,
    fmt::{Display, Formatter, Result},
    hash::Hash,
    hash::Hasher,
};

pub use crate::lir::{
    Addr, AddrSpace, BinOp, Constant, Ir51, Operand, Register, Size, TempIndex, VarId, Version,
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

#[derive(Debug, Clone, Copy, PartialOrd, Ord)]
pub struct SSAVar(pub Addr, pub Version, pub Size);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TempSSAVar(pub TempIndex, pub Version);

impl Eq for SSAVar {}
impl PartialEq for SSAVar {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0 && self.1 == other.1
    }
}

impl Hash for SSAVar {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
        self.1.hash(state);
    }
}

impl Display for SSAVar {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}_{}", crate::lir::format_addr(self.0), self.1)
    }
}

impl Display for TempSSAVar {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "t{}_{}", self.0, self.1)
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
