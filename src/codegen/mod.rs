use std::collections::HashSet;

use crate::{hir::Hir51, mir::Expr};

pub struct CodeGeneration;

impl CodeGeneration {
    pub fn generate(hirs: &mut [Hir51]) {
        println!("\nCodegen: ");
        Self::set_declarations(hirs);
        print!("{}", Self::generate_to_string(hirs));
    }

    fn set_declarations(hirs: &mut [Hir51]) {
        let mut found_vars = HashSet::new();

        for hir in hirs.iter_mut() {
            if let Hir51::Assign {
                declaration, dest, ..
            } = hir
                && let Expr::Var { id, .. } = dest
                && !found_vars.contains(id)
            {
                *declaration = true;
                found_vars.insert(id);
            }
        }
    }

    pub fn generate_to_string(hirs: &[Hir51]) -> String {
        let mut out = String::new();

        for hir in hirs {
            out.push_str(&hir.to_string());
        }

        out
    }
}
