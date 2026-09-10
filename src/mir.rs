use std::{
    collections::HashSet,
    fmt::{Display, Formatter, Result},
};

use crate::{
    common::BlockId,
    lir::{Operand, Register, Size},
};

use std::prelude::v1;

#[derive(Debug, Clone)]
pub enum Expr {
    Register(Register),
    Constant(u16),
    Var { id: usize, size: Size },
    Temp(usize),

    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    Mod(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Xor(Box<Expr>, Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),
    Lt(Box<Expr>, Box<Expr>),
    Gt(Box<Expr>, Box<Expr>),
    Le(Box<Expr>, Box<Expr>),
    Ge(Box<Expr>, Box<Expr>),
    Ne(Box<Expr>, Box<Expr>),

    Deref(Box<Expr>),
}

impl Expr {
    pub fn collect_dependencies(&self, vars: &mut HashSet<Operand>) {
        match self {
            Expr::Constant(_) => {}

            Expr::Add(lhs, rhs)
            | Expr::Sub(lhs, rhs)
            | Expr::Mul(lhs, rhs)
            | Expr::Div(lhs, rhs)
            | Expr::Mod(lhs, rhs)
            | Expr::And(lhs, rhs)
            | Expr::Or(lhs, rhs)
            | Expr::Xor(lhs, rhs)
            | Expr::Eq(lhs, rhs)
            | Expr::Lt(lhs, rhs)
            | Expr::Gt(lhs, rhs)
            | Expr::Le(lhs, rhs)
            | Expr::Ge(lhs, rhs)
            | Expr::Ne(lhs, rhs) => {
                lhs.collect_dependencies(vars);
                rhs.collect_dependencies(vars);
            }

            Expr::Register(reg) => {
                vars.insert(Operand::Register(*reg));
            }
            Expr::Var { id, size } => {
                vars.insert(Operand::Local {
                    id: *id,
                    size: *size,
                });
            }
            Expr::Temp(idx) => {
                vars.insert(Operand::Temp(*idx));
            }

            Expr::Deref(expr) => expr.collect_dependencies(vars),
        }
    }

    pub fn simplify(self) -> Self {
        match self {
            // (val != 0) -> val
            Expr::Ne(lhs, rhs) if let Expr::Constant(0) = rhs.as_ref() => lhs.simplify(),

            // (val == 0) -> !val
            Expr::Eq(lhs, rhs) if let Expr::Constant(0) = rhs.as_ref() => lhs.negate(),

            // (val +/- 0) -> val
            Expr::Add(lhs, rhs) if let Expr::Constant(0) = rhs.as_ref() => lhs.simplify(),
            Expr::Sub(lhs, rhs) if let Expr::Constant(0) = rhs.as_ref() => lhs.simplify(),

            // (intval + var) > extval -> val >= (extval - intval + 1)
            Expr::Gt(rhs, lhs) => {
                if let Expr::Add(add1, var) = rhs.as_ref()
                    && let (Expr::Constant(intval), Expr::Constant(extval)) =
                        (add1.as_ref(), lhs.as_ref())
                {
                    return Expr::Ge(
                        Box::new((**var).clone().simplify()),
                        Box::new(Expr::Constant(extval - intval + 1)),
                    );
                }

                Expr::Gt(rhs, lhs)
            }

            Expr::Le(lhs, rhs) => Expr::Le(Box::new(lhs.simplify()), Box::new(rhs.simplify())),

            _ => self,
        }
    }

    pub fn negate(self) -> Self {
        match self {
            Expr::Eq(lhs, rhs) if matches!(*rhs, Expr::Constant(0)) => *lhs,

            Expr::Lt(lhs, rhs) => Expr::Ge(lhs, rhs),
            Expr::Le(lhs, rhs) => Expr::Gt(lhs, rhs),
            Expr::Gt(lhs, rhs) => Expr::Le(lhs, rhs),
            Expr::Ge(lhs, rhs) => Expr::Lt(lhs, rhs),
            Expr::Ne(lhs, rhs) => Expr::Eq(lhs, rhs),

            e => Expr::Eq(Box::new(e), Box::new(Expr::Constant(0))),
        }
    }

    pub fn var_size(&self) -> Option<Size> {
        if let Expr::Var { size, .. } = self {
            Some(*size)
        } else {
            None
        }
    }
}

impl TryFrom<Operand> for Expr {
    type Error = String;

    fn try_from(op: Operand) -> v1::Result<Self, Self::Error> {
        match op {
            Operand::Constant(val) => Ok(Self::Constant(val)),
            Operand::Register(register) => Ok(Self::Register(register)),
            Operand::Local { id, size } => Ok(Self::Var { id, size }),
            Operand::Temp(idx) => Ok(Self::Temp(idx)),

            Operand::TempSSAVar { .. } | Operand::Var { .. } | Operand::SSAVar { .. } => {
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

    Goto(BlockId),
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

    pub fn extract_condition(self) -> Option<Expr> {
        if let Mir51::If { cond, .. } = self {
            return Some(cond);
        }

        None
    }
}

impl Display for Expr {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Expr::Register(reg) => write!(f, "{:?}", reg),
            Expr::Constant(c) => write!(f, "{c}"),
            Expr::Var { id, .. } => write!(f, "var{}", id),
            Expr::Temp(idx) => write!(f, "temp{}", idx),
            Expr::Add(lhs, rhs) => write!(f, "({} + {})", lhs, rhs),
            Expr::Sub(lhs, rhs) => write!(f, "({} - {})", lhs, rhs),
            Expr::Mul(lhs, rhs) => write!(f, "({} * {})", lhs, rhs),
            Expr::Div(lhs, rhs) => write!(f, "({} / {})", lhs, rhs),
            Expr::Mod(lhs, rhs) => write!(f, "({} % {})", lhs, rhs),
            Expr::And(lhs, rhs) => write!(f, "({} & {})", lhs, rhs),
            Expr::Or(lhs, rhs) => write!(f, "({} | {})", lhs, rhs),
            Expr::Xor(lhs, rhs) => write!(f, "({} ^ {})", lhs, rhs),
            Expr::Eq(lhs, rhs) => write!(f, "({} == {})", lhs, rhs),
            Expr::Lt(lhs, rhs) => write!(f, "({} < {})", lhs, rhs),
            Expr::Gt(lhs, rhs) => write!(f, "({} > {})", lhs, rhs),
            Expr::Le(lhs, rhs) => write!(f, "({} <= {})", lhs, rhs),
            Expr::Ge(lhs, rhs) => write!(f, "({} >= {})", lhs, rhs),
            Expr::Ne(lhs, rhs) => write!(f, "({} != {})", lhs, rhs),
            Expr::Deref(expr) => write!(f, "*({})", expr),
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
