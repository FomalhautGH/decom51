use crate::mir::Mir51;
use std::fmt::{self, Display, Formatter};

pub enum Hir51 {
    Block {
        instr: Vec<Mir51>,
    },

    Loop {
        body: Vec<Hir51>,
    },

    IfElse {
        true_body: Vec<Hir51>,
        false_body: Option<Vec<Hir51>>,
    },
}

impl Display for Hir51 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let indent = f.width().unwrap_or(0);
        let indent_str = "    ".repeat(indent);

        match self {
            Hir51::Block { instr } => {
                for i in instr {
                    writeln!(f, "{}{}", indent_str, i)?;
                }
            }

            Hir51::Loop { body } => {
                writeln!(f, "{}loop {{", indent_str)?;
                for child in body {
                    write!(f, "{:width$}", child, width = indent + 1)?;
                }
                writeln!(f, "{}}}", indent_str)?;
            }

            Hir51::IfElse {
                true_body,
                false_body,
            } => {
                writeln!(f, "{}if (?) {{", indent_str)?;
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
        }

        Ok(())
    }
}
