use crate::{
    common::BlockId,
    mir::{Expr, Mir51},
};
use std::fmt::{self, Display, Formatter};

#[derive(Debug)]
pub enum Hir51 {
    Assign {
        dest: Expr,
        src: Expr,
    },

    Store {
        dest_ptr: Expr,
        value: Expr,
    },

    While {
        cond: Expr,
        body: Vec<Hir51>,
    },

    IfElse {
        cond: Expr,
        true_body: Vec<Hir51>,
        false_body: Option<Vec<Hir51>>,
    },

    Goto {
        jmp: BlockId,
    },

    Call {
        jmp: BlockId,
        args: Vec<Expr>,
        ret_val: Option<Expr>,
    },

    Return,
}

impl From<Mir51> for Hir51 {
    fn from(value: Mir51) -> Self {
        match value {
            Mir51::Assign { dest, src } => Hir51::Assign { dest, src },
            Mir51::Store { dest_ptr, value } => Hir51::Store { dest_ptr, value },
            Mir51::Call { jmp, args } => Hir51::Call {
                jmp,
                args,
                ret_val: None,
            },
            Mir51::If { .. } => unreachable!(),
            Mir51::Goto(bid) => Hir51::Goto { jmp: bid },
            Mir51::Return => Hir51::Return,
        }
    }
}

impl Display for Hir51 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let indent = f.width().unwrap_or(0);
        let indent_str = "    ".repeat(indent);

        match self {
            Hir51::Assign { dest, src } => {
                writeln!(f, "{}{} = {};", indent_str, dest, src)?;
            }

            Hir51::Store { dest_ptr, value } => {
                writeln!(f, "{}*{} = {};", indent_str, dest_ptr, value)?;
            }

            Hir51::While { cond, body } => {
                writeln!(f, "{}while ({}) {{", indent_str, cond)?;
                for child in body {
                    write!(f, "{:width$}", child, width = indent + 1)?;
                }
                writeln!(f, "{}}}", indent_str)?;
            }

            Hir51::IfElse {
                cond,
                true_body,
                false_body,
            } => {
                writeln!(f, "{}if ({}) {{", indent_str, cond)?;
                for child in true_body {
                    write!(f, "{:width$}", child, width = indent + 1)?;
                }
                if let Some(fb) = false_body {
                    writeln!(f, "{}}} else {{", indent_str)?;
                    for child in fb {
                        write!(f, "{:width$}", child, width = indent + 1)?;
                    }
                }
                writeln!(f, "{}}}", indent_str)?;
            }

            Hir51::Goto { jmp } => {
                writeln!(f, "{}goto Block_{};", indent_str, jmp)?;
            }

            Hir51::Call { jmp, args, ret_val } => {
                if let Some(ret) = ret_val {
                    write!(f, "{}{} = Block_{}", indent_str, ret, jmp)?;
                } else {
                    write!(f, "{}Block_{}", indent_str, jmp)?;
                }

                write!(f, "(")?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                writeln!(f, ");")?;
            }

            Hir51::Return => {
                writeln!(f, "{}return;", indent_str)?;
            }
        }

        Ok(())
    }
}
