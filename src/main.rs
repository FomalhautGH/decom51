#![allow(unused)]

use crate::{
    cfg_restorer::CFGRestorer, const_propagation::ConstPropagation, lexer::Lexer, lifter::Lifter,
    mem_normalizer::MemNormalizer, parser::Parser, ssa::SSABuilder,
};
use std::io;

mod cfg_restorer;
mod common;
mod const_propagation;
mod lexer;
mod lifter;
mod mem_normalizer;
mod parser;
mod ssa;

// TODO: Maybe for the most complicates functions try to explain them
// singularly with schemes ecc...
fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let hex_file = &args[1];

    let mut lexer = Lexer::new(hex_file)?;
    let mut parser = Parser::new(&mut lexer);

    let mut lifter = Lifter::new(&mut parser);
    let mut ir_code = lifter.get_ir_instructions();

    MemNormalizer::normalize(&mut ir_code);

    if cfg!(debug_assertions) {
        println!("======================== IR CODE NORMALIZED ========================");
        for (i, ir) in ir_code.iter().enumerate() {
            println!("{i}: {ir:?}")
        }
    }

    let mut cfg = CFGRestorer::restore_raw_cfg(&mut ir_code);
    SSABuilder::build(&mut cfg);

    let mut const_prop = ConstPropagation::default();
    const_prop.execute(&mut cfg);

    Ok(())
}
