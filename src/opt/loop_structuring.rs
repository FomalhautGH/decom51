use crate::opt::{cfg::ControlFlowGraph, loop_recovery::LoopTree};

pub struct LoopStructuring;

impl LoopStructuring {
    pub fn execute(&mut self, cfg: &mut ControlFlowGraph, loop_tree: Vec<LoopTree>) {
        todo!()
    }

    fn collapse(&mut self, loop_tree: Vec<LoopTree>) {
        todo!()
    }
}
