use crate::opt::{
    cfg_restorer::ControlFlowGraph,
    dessa::{
        coalescing::VariableCoalescing, interference::Interference, liveness::Liveness,
        phi_elimination::PhiElimination, ssa_mapping::SSAMapping,
    },
};

pub struct DeSSA;

mod coalescing;
mod interference;
mod liveness;
mod phi_elimination;
mod ssa_mapping;

impl DeSSA {
    pub fn execute(cfg: &mut ControlFlowGraph) {
        // TODO: maybe put the cfg directly in the fields of the respective structs
        let mut liveness = Liveness::new(cfg);
        liveness.analysis(cfg);

        let mut interference = Interference::new(&liveness.out_sets);
        let interference_graph = interference.build_graph(cfg);

        let mut phi_elimination = PhiElimination::default();
        phi_elimination.eliminate(cfg);

        let mut coalescing = VariableCoalescing::new(interference_graph);
        coalescing.execute(cfg);

        // Recreate intereference graph after coalescing
        let mut liveness = Liveness::new(cfg);
        liveness.analysis(cfg);

        let mut interference = Interference::new(&liveness.out_sets);
        let interference_graph = interference.build_graph(cfg);

        let mut dessa = SSAMapping::new(interference_graph);
        dessa.execute(cfg);
    }
}
