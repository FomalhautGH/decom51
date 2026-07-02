use std::fs::File;
use std::io::Read;

use crate::syntax::lexer::Lexer;
pub use parser::{Argument, Instruction, Parser};

mod lexer;
mod parser;

pub struct SyntaxAnalyzer;

impl SyntaxAnalyzer {
    pub fn analyze(asm_code_path: &str) -> Option<Vec<Instruction>> {
        let mut file = File::open(asm_code_path).ok()?;
        let mut contents = String::new();
        file.read_to_string(&mut contents);

        let mut lexer = Lexer::new(contents).ok()?;
        let mut parser = Parser::new(&mut lexer);
        Some(parser.parse())
    }
}
