use std::collections::{HashMap, HashSet};

use crate::{
    common::BlockId,
    opt::cfg_restorer::{BasicBlock, ControlFlowGraph},
};

// Represents the back edges by their head and tail
type BackEdges = Vec<(BlockId, BlockId)>;
type MergedLoops = HashMap<BlockId, HashSet<BlockId>>;
type LoopBodies = Vec<HashSet<BlockId>>;
type DominatorSets = HashMap<BlockId, HashSet<BlockId>>;

#[derive(Default)]
pub struct LoopRecovery;

impl LoopRecovery {
    pub fn execute(&mut self, cfg: &ControlFlowGraph) {
        let dominators = self.compute_dominators(cfg);
        let back_edges = self.find_back_edges(cfg, &dominators);
        let loops = self.find_loops(cfg, &back_edges);
        let loops = self.merge_loops(cfg, back_edges, loops);

        println!("{:?}", loops);
    }

    // Merges the loops with the same head into one
    fn merge_loops(
        &self,
        cfg: &ControlFlowGraph,
        back_edges: BackEdges,
        loops: LoopBodies,
    ) -> MergedLoops {
        let mut merged_loops: MergedLoops = HashMap::new();

        for ((head, _), body) in back_edges.into_iter().zip(loops) {
            merged_loops.entry(head).or_default().extend(body);
        }

        merged_loops
    }

    // Finds every block that is part of the loop represented by its tail and head in
    // the back edge
    fn find_loops(&self, cfg: &ControlFlowGraph, back_edges: &BackEdges) -> LoopBodies {
        let mut loops = Vec::new();

        for &(head, tail) in back_edges {
            let loop_blocks = self.complete_loop(head, tail, cfg);
            loops.push(loop_blocks);
        }

        loops
    }

    fn complete_loop(
        &self,
        head: BlockId,
        tail: BlockId,
        cfg: &ControlFlowGraph,
    ) -> HashSet<BlockId> {
        let mut loop_blocks = HashSet::new();

        loop_blocks.insert(head);
        loop_blocks.insert(tail);

        if head == tail {
            return loop_blocks;
        }

        let mut worklist = Vec::new();
        worklist.push(tail);

        while let Some(block_id) = worklist.pop() {
            let block = &cfg.blocks[&block_id];
            for &pred in &block.predecessors {
                if !loop_blocks.contains(&pred) {
                    loop_blocks.insert(pred);
                    worklist.push(pred);
                }
            }
        }

        loop_blocks
    }

    // Finds the back edges of the graph, every back edge represents a block
    // that is dominated by one of its successors. This means that successor is the
    // 'head' of the loop and the block is that tail of the loop
    fn find_back_edges(&self, cfg: &ControlFlowGraph, dominators: &DominatorSets) -> BackEdges {
        let mut back_edges = Vec::new();

        for block in cfg.blocks.values() {
            let dominators = &dominators[&block.id];
            for &successor in &block.successors {
                if dominators.contains(&successor) {
                    back_edges.push((successor, block.id));
                }
            }
        }

        back_edges
    }

    // Calculates the dominators for all the blocks
    // TODO: We tecnically dont need this since we have the CHK algorithm that
    // calculates the immidiate dominators and we could calculate the dominators by
    // climbing the tree
    fn compute_dominators(&self, cfg: &ControlFlowGraph) -> DominatorSets {
        let mut dominators = DominatorSets::new();
        self.init_dominators(cfg, &mut dominators);

        let mut modified = true;
        while modified {
            modified = self.propagate_dominators(cfg, &mut dominators);
        }
        dominators
    }

    fn propagate_dominators(&self, cfg: &ControlFlowGraph, dominators: &mut DominatorSets) -> bool {
        let mut modified = false;

        for block in cfg.blocks.values() {
            modified |= self.update_block_dominators(block, dominators).is_some();
        }

        modified
    }

    fn update_block_dominators(
        &self,
        block: &BasicBlock,
        dominators: &mut DominatorSets,
    ) -> Option<()> {
        let old_dominators = dominators[&block.id].clone();
        let mut pred_iter = block.predecessors.iter();

        let first_pred = pred_iter.next()?;

        let mut current_dominators = dominators[first_pred].clone();

        for pred in pred_iter {
            let pred_doms = &dominators[pred];
            current_dominators.retain(|d| pred_doms.contains(d));
        }

        current_dominators.insert(block.id);
        if old_dominators != current_dominators {
            dominators.insert(block.id, current_dominators);
            Some(())
        } else {
            None
        }
    }

    fn init_dominators(&self, cfg: &ControlFlowGraph, dominators: &mut DominatorSets) {
        let block_ids: Vec<BlockId> = cfg.blocks.keys().copied().collect();
        self.add_dominator(cfg.start_block, cfg.start_block, dominators);

        for node_id in &block_ids {
            if *node_id != cfg.start_block {
                for &dom_candidate_id in &block_ids {
                    self.add_dominator(*node_id, dom_candidate_id, dominators);
                }
            }
        }
    }

    fn add_dominator(&self, bid: BlockId, dominator_id: BlockId, dominators: &mut DominatorSets) {
        dominators.entry(bid).or_default().insert(dominator_id);
    }
}
