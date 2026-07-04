use crate::{
    common::Ir51,
    opt::{
        cfg_restorer::CFGRestorer, const_propagation::ConstPropagation, dce::DeadCodeElimination,
        rewriter::Rewriter, ssa::SSABuilder,
    },
};

mod cfg_restorer;
mod const_propagation;
mod dce;
mod rewriter;
mod ssa;

pub struct Optimizer;

impl Optimizer {
    pub fn optimize(ir_code: &mut [Ir51]) {
        let mut cfg = CFGRestorer::restore_raw_cfg(ir_code);

        SSABuilder::build(&mut cfg);

        let mut const_prop = ConstPropagation::default();
        const_prop.execute(&cfg);
        let (known_values, known_temps) = (&const_prop.known_values, &const_prop.known_temps);

        let mut rewriter = Rewriter::new(known_values, known_temps);
        rewriter.rewrite(&mut cfg);

        let mut dce = DeadCodeElimination::default(&cfg);
        dce.dce(&mut cfg);
    }
}
