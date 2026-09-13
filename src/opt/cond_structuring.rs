use std::collections::{HashMap, HashSet};

use crate::{
    common::BlockId,
    opt::{
        ssa::{CHKAlgorithm, GraphNode},
        structured_cfg::{BlockKind, StructuredBlock, StructuredCFG},
    },
};

const VIRTUAL_EXIT_ID: BlockId = usize::MAX;

type ImmediatePostDominators = HashMap<BlockId, BlockId>;
type InvertedEdges = HashMap<BlockId, (Vec<BlockId>, Vec<BlockId>)>;

struct Condition {
    predecessors: Vec<BlockId>,
    header_id: BlockId,
    then_id: BlockId,
    else_id: BlockId,
    merge_id: BlockId,
}

#[derive(Default)]
pub struct ConditionStructuring;

impl ConditionStructuring {
    pub fn execute(&mut self, cfg: &mut StructuredCFG) {
        let ipost_dominators = self.compute_immediate_post_dominators(cfg);
        self.collapse(cfg, &ipost_dominators);

        if cfg!(debug_assertions) {
            println!("======================== COND STRUCTURING ========================");
            print!("{}", cfg);
        }
    }

    fn collapse(&mut self, cfg: &mut StructuredCFG, ipost_dominators: &ImmediatePostDominators) {
        let mut conditions = Vec::new();

        for block in cfg.blocks.values_mut() {
            let [else_target, then_target] = block.successors[..] else {
                continue;
            };

            let header_id = block.id;
            let Some(merge_id) = ipost_dominators.get(&header_id) else {
                continue;
            };

            conditions.push(Condition {
                predecessors: block.predecessors.clone(),
                header_id,
                then_id: then_target,
                else_id: else_target,
                merge_id: *merge_id,
            });
        }

        for condition in conditions {
            let then_body_ids =
                Self::gather_branch_body_ids(condition.then_id, condition.merge_id, cfg);
            let else_body_ids =
                Self::gather_branch_body_ids(condition.else_id, condition.merge_id, cfg);

            let merge_block = cfg.blocks.get_mut(&condition.merge_id).unwrap();
            merge_block.predecessors = vec![condition.header_id];

            let condition_header_block = cfg.blocks.remove(&condition.header_id).unwrap();
            let then_body = Self::extract_blocks(cfg, then_body_ids);
            let else_body = Self::extract_blocks(cfg, else_body_ids);

            let structured_condition_block = StructuredBlock {
                id: condition.header_id,
                successors: vec![condition.merge_id],
                predecessors: condition.predecessors,
                kind: BlockKind::Condition {
                    condition_block: Box::new(condition_header_block),
                    then_body,
                    else_body,
                },
            };

            cfg.blocks
                .insert(condition.header_id, structured_condition_block);
        }
    }

    fn extract_blocks(
        cfg: &mut StructuredCFG,
        block_ids: HashSet<BlockId>,
    ) -> Vec<StructuredBlock> {
        let mut extracted_blocks = Vec::new();

        for block_id in block_ids {
            let block = cfg.blocks.remove(&block_id).unwrap();
            extracted_blocks.push(block);
        }

        extracted_blocks
    }

    fn gather_branch_body_ids(
        branch_start_id: BlockId,
        merge_id: BlockId,
        cfg: &StructuredCFG,
    ) -> HashSet<BlockId> {
        let mut branch_body_ids = HashSet::new();

        let mut current_id = branch_start_id;
        while current_id != merge_id {
            let current_block = cfg.blocks.get(&current_id).unwrap();
            let [successor_id] = current_block.successors[..] else {
                unreachable!()
            };

            branch_body_ids.insert(current_id);
            current_id = successor_id;
        }

        branch_body_ids
    }

    // Creates an inverted adjacency representation of the Structured CFG
    // with a virtual exit node whose ID is `VIRTUAL_EXIT_ID` serving as the root
    // for the CHK Algorithm to calculate post-dominators.
    fn get_inverted_edges(cfg: &StructuredCFG) -> InvertedEdges {
        let mut inverted_edges = InvertedEdges::new();

        let mut virtual_exit_successors = HashSet::new();

        for block in cfg.blocks.values() {
            let mut inverted_successors = HashSet::new();
            let mut inverted_predecessors = HashSet::new();

            for &pred in &block.predecessors {
                inverted_successors.insert(pred);
            }

            if block.successors.is_empty() {
                inverted_predecessors.insert(VIRTUAL_EXIT_ID);
                virtual_exit_successors.insert(block.id);
            } else {
                for &succ in &block.successors {
                    inverted_predecessors.insert(succ);
                }
            }

            inverted_edges.insert(
                block.id,
                (
                    inverted_successors.into_iter().collect(),
                    inverted_predecessors.into_iter().collect(),
                ),
            );
        }

        inverted_edges.insert(
            VIRTUAL_EXIT_ID,
            (virtual_exit_successors.into_iter().collect(), Vec::new()),
        );

        inverted_edges
    }

    fn compute_immediate_post_dominators(&self, cfg: &StructuredCFG) -> ImmediatePostDominators {
        let inverted_edges = Self::get_inverted_edges(cfg);

        let graph_nodes: Vec<GraphNode> = inverted_edges
            .iter()
            .map(|(id, (succs, preds))| (id, succs, preds))
            .map(Into::into)
            .collect();

        CHKAlgorithm::execute(&graph_nodes, VIRTUAL_EXIT_ID)
    }
}
