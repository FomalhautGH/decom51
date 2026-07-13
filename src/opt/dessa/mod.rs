use crate::opt::{
    cfg_restorer::ControlFlowGraph,
    dessa::{
        coalescing::VariableCoalescing,
        dessa_mapping::DeSSAMapping,
        interference::{Interference, InterferenceGraph},
        liveness::Liveness,
        phi_elimination::PhiElimination,
    },
};

pub struct DeSSA;

mod coalescing;
mod dessa_mapping;
mod interference;
mod liveness;
mod phi_elimination;

impl DeSSA {
    pub fn execute(cfg: &mut ControlFlowGraph) {
        // TODO: maybe put the cfg directly in the fields of the respective structs
        let mut graph = Self::build_interference_graph(cfg);

        let mut coalescing = VariableCoalescing::new(graph);
        coalescing.execute(cfg);

        graph = Self::build_interference_graph(cfg);

        let mut dessa = DeSSAMapping::new(graph);
        dessa.execute(cfg);

        let mut phi_elimination = PhiElimination::new(cfg);
        phi_elimination.eliminate(cfg);
    }

    fn build_interference_graph(cfg: &ControlFlowGraph) -> InterferenceGraph {
        let mut liveness = Liveness::new(cfg);
        liveness.analysis(cfg);

        let mut interference = Interference::new(&liveness.out_sets);
        interference.build_graph(cfg)
    }
}
