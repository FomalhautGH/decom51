#![allow(unused)]

use std::io;

use crate::{lifting::Lifting, opt::Optimizer, syntax::SyntaxAnalyzer};

mod codegen;
mod common;
mod lifting;
mod lir;
mod mir;
mod opt;
mod syntax;

// TODO: Maybe for the most complicates functions try to explain them
// singularly with schemes ecc...
fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let hex_file = &args[1];

    let instructions = SyntaxAnalyzer::analyze(hex_file).unwrap();
    let mut ir_code = Lifting::lift(&instructions);
    Optimizer::optimize(&mut ir_code);

    Ok(())
}
