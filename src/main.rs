#![allow(unused)]

use crate::{
    cfg_restorer::CFGRestorer,
    lexer::{Lexer},
    lifter::Lifter,
    mem_normalizer::MemNormalizer,
    parser::{Parser},
};
use std::io;

mod cfg_restorer;
mod common;
mod lexer;
mod lifter;
mod mem_normalizer;
mod parser;

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let hex_file = &args[1];

    let mut lexer = Lexer::new(hex_file)?;
    let mut parser = Parser::new(&mut lexer);

    let mut lifter = Lifter::new(&mut parser);
    let mut ir_code = lifter.get_ir_instructions();

    MemNormalizer::normalize(&mut ir_code);

    for (i, ir) in ir_code.iter().enumerate() {
        println!("{i}: {ir:?}")
    }

    let graph = CFGRestorer::restore_raw_cfg(&ir_code);

    Ok(())
}
