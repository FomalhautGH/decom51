use std::{
    collections::HashSet,
    fmt::{Display, Formatter, Result},
};

use crate::{
    common::BlockId,
    lir::{Operand, Register},
};
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

impl Expr {
    pub fn collect_dependencies(&self, vars: &mut HashSet<Operand>) {
        match self {
            Expr::Constant(_) => {}

            Expr::Add(lhs, rhs)
            | Expr::Sub(lhs, rhs)
            | Expr::Eq(lhs, rhs)
            | Expr::Lt(lhs, rhs)
            | Expr::Gt(lhs, rhs)
            | Expr::Le(lhs, rhs)
            | Expr::Ge(lhs, rhs)
            | Expr::Ne(lhs, rhs)
            | Expr::Or(lhs, rhs) => {
                lhs.collect_dependencies(vars);
                rhs.collect_dependencies(vars);
            }

            Expr::Register(reg) => {
                vars.insert(Operand::Register(*reg));
            }
            Expr::Var(id) => {
                vars.insert(Operand::Local(*id));
            }
            Expr::Temp(idx) => {
                vars.insert(Operand::Temp(*idx));
            }

            Expr::Deref(expr) => expr.collect_dependencies(vars),
            Expr::Concat(exprs) => {
                for expr in exprs {
                    expr.collect_dependencies(vars);
                }
            }
        }
    }

    pub fn get_size(&self) -> usize {
        match &self {
            Expr::Register(_) => 1,
            Expr::Constant(_) => 1,
            Expr::Var(_) => 1,
            Expr::Temp(_) => 1,
            Expr::Add(expr, expr1)
            | Expr::Sub(expr, expr1)
            | Expr::Eq(expr, expr1)
            | Expr::Lt(expr, expr1)
            | Expr::Gt(expr, expr1)
            | Expr::Le(expr, expr1)
            | Expr::Ge(expr, expr1)
            | Expr::Ne(expr, expr1)
            | Expr::Or(expr, expr1) => expr.get_size() + expr1.get_size(),
            Expr::Concat(exprs) => exprs.iter().map(|expr| expr.get_size()).sum(),
            Expr::Deref(expr) => expr.get_size(),
        }
    }
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
        true_jmp: BlockId,
        false_jmp: BlockId,
    },

    Call {
        jmp: BlockId,
        args: Vec<Expr>,
    },

    Goto(usize),
    Return,
}

impl Mir51 {
    // Checks if instruction is sp increment and returns its size
    pub fn is_sp_increment(&self) -> Option<usize> {
        match &self {
            Mir51::Assign {
                dest: Expr::Register(Register::SP),
                src: Expr::Add(sp, value),
            } if matches!(**sp, Expr::Register(Register::SP)) => {
                if let Expr::Constant(v) = **value {
                    Some(v as usize)
                } else {
                    None
                }
            }

            _ => None,
        }
    }

    pub fn is_sp_decrement(&self) -> Option<usize> {
        match &self {
            Mir51::Assign {
                dest: Expr::Register(Register::SP),
                src: Expr::Sub(sp, value),
            } if matches!(**sp, Expr::Register(Register::SP)) => {
                if let Expr::Constant(v) = **value {
                    Some(v as usize)
                } else {
                    None
                }
            }

            _ => None,
        }
    }
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
            Mir51::Call { jmp, args } => {
                write!(f, "call Block_{}", jmp)?;
                if !args.is_empty() {
                    write!(f, "(")?;
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", arg)?;
                    }
                    write!(f, ")")?;
                }
                Ok(())
            }
            Mir51::Return => write!(f, "ret"),
        }
    }
}
