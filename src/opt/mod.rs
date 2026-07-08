use crate::{
    common::Ir51,
    opt::{
        cfg_restorer::CFGRestorer, coalescing::VariableCoalescing,
        const_propagation::ConstPropagation, dbe::DeadBlockElimination, dce::DeadCodeElimination,
        interference::Interference, liveness::Liveness, phi_elimination::PhiElimination,
        rewriter::Rewriter, ssa::SSABuilder,
    },
};

mod cfg_restorer;
mod coalescing;
mod const_propagation;
mod dbe;
mod dce;
mod interference;
mod liveness;
mod phi_elimination;
mod rewriter;
mod ssa;

pub struct Optimizer;

impl Optimizer {
    pub fn optimize(ir_code: &mut [Ir51]) {
        let mut cfg = CFGRestorer::restore_raw_cfg(ir_code);

        let mut dbe = DeadBlockElimination::default();
        dbe.execute(&mut cfg);

        SSABuilder::build(&mut cfg);

        let mut const_prop = ConstPropagation::default();
        const_prop.execute(&cfg);

        let (known_values, known_temps) = (&const_prop.known_values, &const_prop.known_temps);
        let rewriter = Rewriter::new(known_values, known_temps);
        rewriter.rewrite(&mut cfg);

        let mut dce = DeadCodeElimination::default();
        dce.execute(&mut cfg);

        // TODO: Put de-ssa phases in a dedicated module and maybe put the cfg
        // directly in the fields of the respective structs
        let mut liveness = Liveness::new(&cfg);
        liveness.analysis(&cfg);

        let mut interference = Interference::new(&liveness.out_sets);
        let interference_graph = interference.build_graph(&cfg);

        let mut phi_elimination = PhiElimination::default();
        phi_elimination.eliminate(&mut cfg);

        let mut coalescing = VariableCoalescing::new(interference_graph);
        coalescing.execute(&mut cfg);
    }
}
