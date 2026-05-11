#![allow(unused)]

use crate::parser::{Lexer, TokenType};
use std::io;

mod parser;

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let hex_file = &args[1];

    let mut lexer = Lexer::new(hex_file)?;

    let mut token = TokenType::Mov;
    while token != TokenType::End {
        token = lexer.parse();
        println!("{token:?}")
    }

    Ok(())
}
