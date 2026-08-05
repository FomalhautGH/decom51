use crate::hir::Hir51;

pub struct CodeGeneration;

impl CodeGeneration {
    pub fn generate(hirs: &[Hir51]) {
        println!("\nCodegen: ");
        print!("{}", Self::generate_to_string(hirs));
    }

    pub fn generate_to_string(hirs: &[Hir51]) -> String {
        let mut out = String::new();

        for hir in hirs {
            out.push_str(&hir.to_string());
        }

        out
    }
}
