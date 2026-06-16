#![allow(unused)]

use crate::{
    cfg_restorer::CFGRestorer, lexer::Lexer, lifter::Lifter, mem_normalizer::MemNormalizer,
    parser::Parser, ssa::SSABuilder,
};
use std::io;

mod cfg_restorer;
mod common;
mod lexer;
mod lifter;
mod mem_normalizer;
mod parser;
mod ssa;

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let hex_file = &args[1];

    let mut lexer = Lexer::new(hex_file)?;
    let mut parser = Parser::new(&mut lexer);

    let mut lifter = Lifter::new(&mut parser);
    let mut ir_code = lifter.get_ir_instructions();

    MemNormalizer::normalize(&mut ir_code);

    let mut graph = CFGRestorer::restore_raw_cfg(&ir_code);
    SSABuilder::build(&mut graph);

    Ok(())
}
