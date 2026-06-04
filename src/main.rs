#![allow(unused)]

use crate::{
    lexer::{Lexer, TokenType}, lifter::{Ir51, Lifter}, parser::{Instruction, Parser}
};
use std::io;

mod common;
mod lexer;
mod parser;
mod lifter;

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let hex_file = &args[1];

    let mut lexer = Lexer::new(hex_file)?;
    let mut parser = Parser::new(&mut lexer);
    let mut lifter = Lifter::new(&mut parser);

    let irs = lifter.get_ir_instructions();

    for i in irs.iter() {
        println!("{i:?}");
    }

    Ok(())
}
