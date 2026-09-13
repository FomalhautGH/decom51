use crate::{
    common::Ir51,
    hir::Hir51,
    opt::{
        call_recovery::CallRecovery, cfg_restorer::CFGRestorer, cle::ControlLogicExtraction,
        cond_structuring::ConditionStructuring, const_propagation::ConstPropagation,
        dbe::DeadBlockElimination, dce::DeadCodeElimination, dessa::DeSSA,
        dse::DeadStoreElimination, ebe::EmptyBlockElimination, expr_aggregation::ExprAggregation,
        loop_recovery::LoopRecovery, loop_structuring::LoopStructuring, rewriter::Rewriter,
        ssa::SSABuilder, structured_cfg::StructuredCFG, vwa::VariableWidthAggregation,
    },
};

mod call_recovery;
mod cfg;
mod cfg_restorer;
mod cle;
mod cond_structuring;
mod const_propagation;
mod dbe;
mod dce;
mod dessa;
mod dse;
mod ebe;
mod expr_aggregation;
mod loop_recovery;
mod loop_structuring;
mod rewriter;
mod ssa;
mod structured_cfg;
mod vwa;

pub struct Optimizer;

impl Optimizer {
    pub fn optimize(ir_code: &mut [Ir51]) -> Vec<Hir51> {
        let mut cfg = CFGRestorer::restore_raw_cfg(ir_code);

        let mut dbe = DeadBlockElimination::default();
        dbe.execute(&mut cfg);

        let mut vwa = VariableWidthAggregation::default();
        vwa.execute(&mut cfg);

        SSABuilder::build(&mut cfg);

        let mut const_prop = ConstPropagation::default();
        const_prop.execute(&cfg);

        let (known_values, known_temps) = (&const_prop.known_values, &const_prop.known_temps);
        let rewriter = Rewriter::new(known_values, known_temps);
        rewriter.rewrite(&mut cfg);

        let mut dbe = DeadBlockElimination::default();
        dbe.execute(&mut cfg);

        let mut dce = DeadCodeElimination::default();
        dce.execute(&mut cfg);

        DeSSA::execute(&mut cfg);

        EmptyBlockElimination::execute(&mut cfg);

        let mut dbe = DeadBlockElimination::default();
        dbe.execute(&mut cfg);

        let mut expragr = ExprAggregation::default();
        expragr.execute(&mut cfg);

        let call_recovery = CallRecovery;
        call_recovery.execute(&mut cfg);

        let mut lr = LoopRecovery;
        let loop_tree = lr.execute(&cfg);

        cfg.merge_blocks();

        let mut dse = DeadStoreElimination::default();
        dse.execute(&mut cfg);

        let mut structured_cfg = StructuredCFG::from(cfg);

        let mut ls = LoopStructuring::new(&structured_cfg);
        ls.execute(&mut structured_cfg, loop_tree);

        let mut cs = ConditionStructuring;
        cs.execute(&mut structured_cfg);

        let mut cle = ControlLogicExtraction::default();
        cle.execute(structured_cfg);

        cle.extracted_hirs
    }
}
