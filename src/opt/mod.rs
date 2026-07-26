use crate::{
    common::Ir51,
    opt::{
        cfg_restorer::CFGRestorer, const_propagation::ConstPropagation, dbe::DeadBlockElimination,
        dce::DeadCodeElimination, dessa::DeSSA, ebe::EmptyBlockElimination,
        expr_aggregation::ExprAggregation, loop_recovery::LoopRecovery, rewriter::Rewriter,
        ssa::SSABuilder, vwa::VariableWidthAggregation,
    },
};

mod cfg_restorer;
mod const_propagation;
mod dbe;
mod dce;
mod dessa;
mod ebe;
mod expr_aggregation;
mod loop_recovery;
mod rewriter;
mod ssa;
mod vwa;

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

        DeSSA::execute(&mut cfg);

        EmptyBlockElimination::execute(&mut cfg);

        let mut dbe = DeadBlockElimination::default();
        dbe.execute(&mut cfg);

        let mut vwa = VariableWidthAggregation;
        vwa.execute(&mut cfg);

        let mut expragr = ExprAggregation::default();
        expragr.execute(&mut cfg);

        let mut lr = LoopRecovery;
        lr.execute(&cfg);
    }
}
