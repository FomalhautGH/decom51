use crate::{
    common::BlockId,
    lir::Size,
    mir::{Expr, Mir51},
};
use std::fmt::{self, Display, Formatter};

#[derive(Debug)]
pub enum Hir51 {
    Assign {
        declaration: bool,
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
        false_body: Vec<Hir51>,
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
            Mir51::Assign { dest, src } => Hir51::Assign {
                dest,
                src,
                declaration: false,
            },
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

impl Hir51 {
    fn fmt_recursive(&self, f: &mut Formatter<'_>, indent: usize) -> fmt::Result {
        let indent_str = "    ".repeat(indent);

        match self {
            Hir51::Assign {
                dest,
                src,
                declaration,
            } => {
                if *declaration && let Some(size) = dest.var_size() {
                    writeln!(
                        f,
                        "{}{} {} = {};",
                        indent_str,
                        type_from_size(size),
                        dest,
                        src
                    )?;
                } else {
                    writeln!(f, "{}{} = {};", indent_str, dest, src)?;
                }
            }

            Hir51::Store { dest_ptr, value } => {
                writeln!(f, "{}*{} = {};", indent_str, dest_ptr, value)?;
            }

            Hir51::While { cond, body } => {
                writeln!(f, "{}while ({}) {{", indent_str, cond)?;
                for child in body {
                    child.fmt_recursive(f, indent + 1)?;
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
                    child.fmt_recursive(f, indent + 1)?;
                }
                if !false_body.is_empty() {
                    writeln!(f, "{}}} else {{", indent_str)?;
                    for child in false_body {
                        child.fmt_recursive(f, indent + 1)?;
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

impl Display for Hir51 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_recursive(f, 0)
    }
}

fn type_from_size(size: Size) -> &'static str {
    match size {
        1 => "unsigned char",
        2 => "unsigned int",
        3 => "unsigned long",
        _ => unreachable!("Invalid size"),
    }
}
