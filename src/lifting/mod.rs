use crate::{
    common::Ir51,
    lifting::{lifter::Lifter, mem_normalizer::MemNormalizer},
    syntax::Instruction,
};

mod lifter;
mod mem_normalizer;

pub struct Lifting;

impl Lifting {
    pub fn lift(instructions: &[Instruction]) -> Vec<Ir51> {
        let mut lifter = Lifter::new();
        let mut ir_code = lifter.get_ir_instructions(instructions);

        MemNormalizer::normalize(&mut ir_code);

        if cfg!(debug_assertions) {
            println!("======================== IR CODE NORMALIZED ========================");
            for (i, ir) in ir_code.iter().enumerate() {
                println!("{i}: {ir:?}")
            }
        }

        ir_code
    }
}
