use std::fs::File;
use std::io::Read;

pub use lexer::Lexer;
pub use parser::{Argument, Instruction, Parser};

mod lexer;
mod parser;

pub struct SyntaxAnalyzer;

impl SyntaxAnalyzer {
    pub fn analyze(asm_code_path: &str) -> Option<Vec<Instruction>> {
        let mut file = File::open(asm_code_path).ok()?;
        let mut contents = String::new();

        file.read_to_string(&mut contents).ok()?;
        Self::analyze_str(&contents)
    }

    pub fn analyze_str(contents: &str) -> Option<Vec<Instruction>> {
        let mut lexer = Lexer::new(contents.to_string()).ok()?;
        let mut parser = Parser::new(&mut lexer);
        Some(parser.parse())
    }
}
