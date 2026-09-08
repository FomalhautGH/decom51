use std::collections::{HashMap, HashSet};

use crate::{
    common::BlockId,
    opt::{
        cfg::{BasicBlock, ControlFlowGraph},
        ssa::CHKAlgorithm,
    },
};

// Represents a back edge from a latch block (source) to a loop header (target)
type BackEdge = (BlockId, BlockId); // (header, latch)
type BackEdges = Vec<BackEdge>;
type LoopBodies = Vec<HashSet<BlockId>>;
type LoopMap = HashMap<BlockId, HashSet<BlockId>>;
type DominatorMap = HashMap<BlockId, HashSet<BlockId>>;

// Represents a node in the loop nest tree/forest, capturing loop hierarchy, header, and body
#[derive(Debug, Clone)]
pub struct LoopTree {
    pub header: BlockId,
    pub body: HashSet<BlockId>,
    pub children: Vec<LoopTree>,
}

#[derive(Default)]
pub struct LoopRecovery;

impl LoopRecovery {
    pub fn execute(&mut self, cfg: &ControlFlowGraph) -> Vec<LoopTree> {
        let dominators = self.compute_dominators(cfg);
        let back_edges = self.find_back_edges(cfg, &dominators);
        let loops = self.find_loops(cfg, back_edges);
        self.build_loop_tree(loops)
    }

    // Phase 1: Dominator Data-flow Analysis
    // Computes all dominators for every basic block climing back the immediate dominators tree.
    fn compute_dominators(&self, cfg: &ControlFlowGraph) -> DominatorMap {
        let mut dominators = DominatorMap::new();

        let blocks: Vec<&BasicBlock> = cfg.blocks.values().collect();
        let immidiate_dominators = CHKAlgorithm::execute(&blocks);

        for block in blocks {
            let mut block_dominators = HashSet::new();
            block_dominators.insert(block.id);

            let mut it = block.id;
            while let Some(idom) = immidiate_dominators.get(&it) {
                if it == *idom {
                    break;
                }

                block_dominators.insert(*idom);
                it = *idom;
            }

            dominators.insert(block.id, block_dominators);
        }

        dominators
    }

    // Phase 2: Back-edge Identification
    // Identifies back-edges (latch -> header) where the target header dominates the source latch
    fn find_back_edges(&self, cfg: &ControlFlowGraph, dominators: &DominatorMap) -> BackEdges {
        let mut back_edges = Vec::new();

        for block in cfg.blocks.values() {
            let block_dominators = &dominators[&block.id];
            for &successor in &block.successors {
                if block_dominators.contains(&successor) {
                    back_edges.push((successor, block.id));
                }
            }
        }

        back_edges
    }

    // Phase 3: Natural Loop Body Reconstruction & Merging
    // Reconstructs all natural loop bodies from back-edges and merges loops sharing the same header
    fn find_loops(&self, cfg: &ControlFlowGraph, back_edges: BackEdges) -> LoopMap {
        let mut loop_bodies = Vec::new();

        for &(header, latch) in &back_edges {
            let body = self.collect_loop_body(header, latch, cfg);
            loop_bodies.push(body);
        }

        Self::merge_loops(back_edges, loop_bodies)
    }

    // Collects all basic blocks belonging to the natural loop defined by (header, latch)
    // via backward traversal
    fn collect_loop_body(
        &self,
        header: BlockId,
        latch: BlockId,
        cfg: &ControlFlowGraph,
    ) -> HashSet<BlockId> {
        let mut loop_blocks = HashSet::new();

        loop_blocks.insert(header);
        loop_blocks.insert(latch);

        if header == latch {
            return loop_blocks;
        }

        let mut worklist = Vec::new();
        worklist.push(latch);

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

    // Merges multiple natural loops that share the exact same header block
    fn merge_loops(back_edges: BackEdges, loop_bodies: LoopBodies) -> LoopMap {
        let mut merged_loops: LoopMap = HashMap::new();

        for ((header, _), body) in back_edges.into_iter().zip(loop_bodies) {
            merged_loops.entry(header).or_default().extend(body);
        }

        merged_loops
    }

    // Phase 4: Loop Nest Hierarchy (Loop Forest Construction)
    #[allow(clippy::needless_range_loop)]
    fn build_loop_tree(&self, loops: LoopMap) -> Vec<LoopTree> {
        let mut loop_tree_map = Self::init_loop_tree_map(loops.clone());
        let mut root_headers: HashSet<BlockId> = loop_tree_map.keys().copied().collect();

        let mut loops_by_size: Vec<(BlockId, usize)> = loops
            .iter()
            .map(|(header, body)| (*header, body.len()))
            .collect();
        loops_by_size.sort_by_key(|(_, len)| *len);

        for i in 0..loops_by_size.len() {
            let inner_header = loops_by_size[i].0;
            let inner_body = &loops[&inner_header];

            for j in (i + 1)..loops_by_size.len() {
                let outer_header = loops_by_size[j].0;
                let outer_body = &loops[&outer_header];

                if inner_body.is_subset(outer_body) {
                    let inner_tree = loop_tree_map.remove(&inner_header).unwrap();
                    let outer_tree = loop_tree_map.get_mut(&outer_header).unwrap();
                    outer_tree.children.push(inner_tree);

                    root_headers.remove(&inner_header);
                    break;
                }
            }
        }

        let mut loop_forest: Vec<LoopTree> = loop_tree_map.into_values().collect();
        loop_forest.retain(|lt| root_headers.contains(&lt.header));
        loop_forest
    }

    fn init_loop_tree_map(loops: LoopMap) -> HashMap<BlockId, LoopTree> {
        let mut loop_tree_map = HashMap::new();

        for (header, body) in loops {
            let loop_tree = LoopTree {
                header,
                body,
                children: Vec::new(),
            };
            loop_tree_map.insert(header, loop_tree);
        }

        loop_tree_map
    }
}
