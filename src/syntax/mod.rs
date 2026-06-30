use crate::syntax::lexer::Lexer;
pub use parser::{Argument, Instruction, Parser};

mod lexer;
mod parser;

pub struct SyntaxAnalyzer;

impl SyntaxAnalyzer {
    pub fn analyze(asm_code_path: &str) -> Option<Vec<Instruction>> {
        let mut lexer = Lexer::new(asm_code_path).ok()?;
        let mut parser = Parser::new(&mut lexer);
        Some(parser.parse())
    }
}
