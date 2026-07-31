use crate::hir::Hir51;

pub struct CodeGeneration;

impl CodeGeneration {
    pub fn generate(hirs: &Vec<Hir51>) {
        println!("\nCodegen: ");
        for hir in hirs {
            print!("{}", hir);
        }
    }
}
