use std::fmt::{Display, Formatter, Result};

use crate::lir::{Operand, Register};
use std::prelude::v1;

#[derive(Debug, Clone)]
pub enum Expr {
    Register(Register),
    Constant(u16),
    Var(usize),
    Temp(usize),

    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),
    Lt(Box<Expr>, Box<Expr>),
    Gt(Box<Expr>, Box<Expr>),
    Le(Box<Expr>, Box<Expr>),
    Ge(Box<Expr>, Box<Expr>),
    Ne(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),

    Deref(Box<Expr>),
    Concat(Vec<Expr>),
}

impl TryFrom<Operand> for Expr {
    type Error = String;

    fn try_from(op: Operand) -> v1::Result<Self, Self::Error> {
        match op {
            Operand::Constant(val) => Ok(Self::Constant(val)),
            Operand::Register(register) => Ok(Self::Register(register)),
            Operand::Local(id) => Ok(Self::Var(id)),
            Operand::Temp(idx) => Ok(Self::Temp(idx)),
            Operand::Concat(id1, id2) => Ok(Self::Concat(vec![Self::Var(id1), Self::Var(id2)])),

            Operand::TempSSAVar { .. } | Operand::Var(_, _) | Operand::SSAVar { .. } => {
                Err("Illegal operand for Expr".to_string())
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum Mir51 {
    Assign {
        dest: Expr,
        src: Expr,
    },

    Store {
        dest_ptr: Expr,
        value: Expr,
    },

    If {
        cond: Expr,
        true_jmp: usize,
        false_jmp: usize,
    },

    Goto(usize),
    Call(usize),
    Return,
}

impl Display for Expr {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Expr::Register(reg) => write!(f, "{:?}", reg),
            Expr::Constant(c) => write!(f, "#0x{:02X}", c),
            Expr::Var(id) => write!(f, "v{}", id),
            Expr::Temp(idx) => write!(f, "t{}", idx),
            Expr::Add(lhs, rhs) => write!(f, "({} + {})", lhs, rhs),
            Expr::Sub(lhs, rhs) => write!(f, "({} - {})", lhs, rhs),
            Expr::Eq(lhs, rhs) => write!(f, "({} == {})", lhs, rhs),
            Expr::Lt(lhs, rhs) => write!(f, "({} < {})", lhs, rhs),
            Expr::Gt(lhs, rhs) => write!(f, "({} > {})", lhs, rhs),
            Expr::Le(lhs, rhs) => write!(f, "({} <= {})", lhs, rhs),
            Expr::Ge(lhs, rhs) => write!(f, "({} >= {})", lhs, rhs),
            Expr::Ne(lhs, rhs) => write!(f, "({} != {})", lhs, rhs),
            Expr::Or(lhs, rhs) => write!(f, "({} | {})", lhs, rhs),
            Expr::Deref(expr) => write!(f, "*({})", expr),
            Expr::Concat(exprs) => {
                write!(f, "concat(")?;
                for (i, expr) in exprs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", expr)?;
                }
                write!(f, ")")
            }
        }
    }
}

impl Display for Mir51 {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Mir51::Assign { dest, src } => write!(f, "{} = {}", dest, src),
            Mir51::Store { dest_ptr, value } => write!(f, "*{} = {}", dest_ptr, value),
            Mir51::If {
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
            Mir51::Goto(label) => write!(f, "goto Block_{}", label),
            Mir51::Call(label) => write!(f, "call Block_{}", label),
            Mir51::Return => write!(f, "ret"),
        }
    }
}
