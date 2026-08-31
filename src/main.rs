#![allow(unused)]

use std::io;

use crate::{codegen::CodeGeneration, lifting::Lifting, opt::Optimizer, syntax::SyntaxAnalyzer};

mod codegen;
mod common;
mod hir;
mod lifting;
mod lir;
mod mir;
mod opt;
mod syntax;

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let hex_file = &args[1];

    let instructions = SyntaxAnalyzer::analyze(hex_file).unwrap();
    let mut ir_code = Lifting::lift(&instructions);
    let mut hirs = Optimizer::optimize(&mut ir_code);

    CodeGeneration::generate(&mut hirs);

    Ok(())
}
