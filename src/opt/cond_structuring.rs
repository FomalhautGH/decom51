use std::collections::{HashMap, HashSet};

use crate::{
    common::BlockId,
    opt::{
        ssa::{CHKAlgorithm, GraphNode},
        structured_cfg::StructuredCFG,
    },
};

const VIRTUAL_EXIT_ID: BlockId = usize::MAX;

type PostDominatorMap = HashMap<BlockId, HashSet<BlockId>>;
type InvertedEdges = HashMap<BlockId, (Vec<BlockId>, Vec<BlockId>)>;

#[derive(Default)]
pub struct ConditionStructuring;

impl ConditionStructuring {
    pub fn execute(&mut self, cfg: &mut StructuredCFG) {
        let post_dominators = self.compute_post_dominators(cfg);

        if cfg!(debug_assertions) {
            println!("======================== COND STRUCTURING ========================");
            println!("{:?}", post_dominators);
            print!("{}", cfg);
        }
    }

    // Creates an inverted adjacency representation of the Structured CFG
    // with a virtual exit node whose ID is `VIRTUAL_EXIT_ID` serving as the root
    // for the CHK Algorithm to calculate post-dominators.
    fn get_inverted_edges(cfg: &StructuredCFG) -> InvertedEdges {
        let mut edges = InvertedEdges::new();

        let mut virtual_exit_successors = HashSet::new();

        for block in cfg.blocks.values() {
            let mut succ_set = HashSet::new();
            let mut pred_set = HashSet::new();

            for succ in &block.predecessors {
                succ_set.insert(*succ);
            }

            if block.successors.is_empty() {
                pred_set.insert(VIRTUAL_EXIT_ID);
                virtual_exit_successors.insert(block.id);
            } else {
                for pred in &block.successors {
                    pred_set.insert(*pred);
                }
            }

            edges.insert(
                block.id,
                (
                    succ_set.into_iter().collect(),
                    pred_set.into_iter().collect(),
                ),
            );
        }

        edges.insert(
            VIRTUAL_EXIT_ID,
            (virtual_exit_successors.into_iter().collect(), Vec::new()),
        );
        edges
    }

    fn compute_post_dominators(&self, cfg: &StructuredCFG) -> PostDominatorMap {
        let mut dominators = PostDominatorMap::new();
        let edges = Self::get_inverted_edges(cfg);

        let blocks: Vec<GraphNode> = edges
            .iter()
            .map(|(id, (succ, pred))| (id, succ, pred))
            .map(Into::into)
            .collect();

        let immediate_dominators = CHKAlgorithm::execute(&blocks, VIRTUAL_EXIT_ID);

        for block in blocks {
            let mut block_dominators = HashSet::new();
            block_dominators.insert(*block.id);

            let mut curr_id = *block.id;
            while let Some(idom) = immediate_dominators.get(&curr_id) {
                if curr_id == *idom {
                    break;
                }

                block_dominators.insert(*idom);
                curr_id = *idom;
            }

            dominators.insert(*block.id, block_dominators);
        }

        dominators
    }
}
