pub mod codegen;
pub mod common;
pub mod hir;
pub mod lifting;
pub mod lir;
pub mod mir;
pub mod opt;
pub mod syntax;

use crate::{codegen::CodeGeneration, lifting::Lifting, opt::Optimizer, syntax::SyntaxAnalyzer};

pub fn decompile_str(asm_code: &str) -> String {
    let instructions = SyntaxAnalyzer::analyze_str(asm_code).expect("Failed to analyze syntax");
    let mut ir_code = Lifting::lift(&instructions);
    let hirs = Optimizer::optimize(&mut ir_code);
    CodeGeneration::generate_to_string(&hirs)
}
