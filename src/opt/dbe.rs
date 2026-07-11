use crate::{common::BlockId, opt::cfg_restorer::ControlFlowGraph};
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
            let mut sorted_reached: Vec<&BlockId> = self.reached_blocks.iter().collect();
            sorted_reached.sort();
            println!("Reachable Blocks: {:?}", sorted_reached);
        }

        self.cleanup(cfg);
    }

    fn cleanup(&self, cfg: &mut ControlFlowGraph) {
        cfg.blocks
            .retain(|block_id, _| self.reached_blocks.contains(block_id));

        // Removes any reference to the unreachable block from the other blocks
        for block in cfg.blocks.values_mut() {
            block
                .predecessors
                .retain(|blockid| self.reached_blocks.contains(blockid));
        }
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
