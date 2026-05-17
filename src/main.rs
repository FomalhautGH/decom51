#![allow(unused)]

use crate::{
    lexer::{Lexer, TokenType},
    parser::{Instruction, Parser},
};
use std::io;

mod lexer;
mod parser;

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let hex_file = &args[1];

    let mut lexer = Lexer::new(hex_file)?;
    let mut parser = Parser::new(&mut lexer);

    let mut instr = Instruction::Nop;
    while instr != Instruction::End {
        instr = parser.parse_next();
        println!("{instr:?}")
    }

    Ok(())
}
