use crate::{
    common::{BlockId, Ir51},
    opt::cfg_restorer::ControlFlowGraph,
};
use std::collections::HashSet;

#[derive(Default)]
pub struct DeadBlockElimination {
    block_queue: Vec<BlockId>,
    reached_blocks: HashSet<BlockId>,
}

impl DeadBlockElimination {
    pub fn execute(&mut self, cfg: &mut ControlFlowGraph) {
        self.init(cfg);
        self.visit_cfg(cfg);

        if cfg!(debug_assertions) {
            println!("======================== DBE ========================");
            println!("Reachable Blocks: {:?}", self.reached_blocks);
        }

        self.cleanup(cfg);
    }

    fn cleanup(&self, cfg: &mut ControlFlowGraph) {
        cfg.blocks.retain(|block_id, block| {
            if !self.reached_blocks.contains(block_id) {
                let range = block.ir_range.clone();

                for ir in &mut cfg.ir_code[range] {
                    *ir = Ir51::Nop;
                }

                false
            } else {
                true
            }
        });
    }

    fn visit_cfg(&mut self, cfg: &ControlFlowGraph) {
        while let Some(block_id) = self.block_queue.pop() {
            let successors = &cfg.blocks[&block_id].successors;

            for suc in successors {
                if !self.reached_blocks.contains(suc) {
                    self.reached_blocks.insert(*suc);
                    self.block_queue.push(*suc);
                }
            }
        }
    }

    fn init(&mut self, cfg: &ControlFlowGraph) {
        let start_block = cfg.start_block;
        self.block_queue.push(start_block);
        self.reached_blocks.insert(start_block);
    }
}
