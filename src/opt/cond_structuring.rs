use std::collections::{HashMap, HashSet};

use crate::{
    common::BlockId,
    opt::{
        ssa::{CHKAlgorithm, GraphNode},
        structured_cfg::{BlockKind, StructuredBlock, StructuredCFG},
    },
};

const VIRTUAL_EXIT_ID: BlockId = usize::MAX;

type ImmediateDominators = HashMap<BlockId, BlockId>;
type InvertedEdges = HashMap<BlockId, (Vec<BlockId>, Vec<BlockId>)>;

struct Condition {
    predecessors: Vec<BlockId>,
    head_id: BlockId,
    true_id: BlockId,
    false_id: BlockId,
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

    fn collapse(&mut self, cfg: &mut StructuredCFG, pdom: &ImmediateDominators) {
        let mut conditions = Vec::new();

        for block in cfg.blocks.values_mut() {
            let [false_jmp, true_jmp] = block.successors[..] else {
                continue;
            };

            let head_block_id = block.id;
            let Some(merge_block_id) = pdom.get(&block.id) else {
                continue;
            };

            conditions.push(Condition {
                predecessors: block.predecessors.clone(),
                head_id: head_block_id,
                true_id: true_jmp,
                false_id: false_jmp,
                merge_id: *merge_block_id,
            });
        }

        for condition in conditions {
            let then_body =
                Self::gather_branch_body_ids(condition.true_id, condition.merge_id, cfg);
            let else_body =
                Self::gather_branch_body_ids(condition.false_id, condition.merge_id, cfg);

            let merge_block = cfg.blocks.get_mut(&condition.merge_id).unwrap();
            merge_block.predecessors = vec![condition.head_id];

            let head_block = cfg.blocks.remove(&condition.head_id).unwrap();
            let then_body = Self::extract_blocks(cfg, then_body);
            let else_body = Self::extract_blocks(cfg, else_body);

            let block = StructuredBlock {
                id: condition.head_id,
                successors: vec![condition.merge_id],
                predecessors: condition.predecessors,
                kind: BlockKind::Condition {
                    condition_block: Box::new(head_block),
                    then_body,
                    else_body,
                },
            };

            cfg.blocks.insert(condition.head_id, block);
        }
    }

    fn extract_blocks(cfg: &mut StructuredCFG, blocks: HashSet<BlockId>) -> Vec<StructuredBlock> {
        let mut extracted_blocks = Vec::new();

        for block_id in blocks {
            let block = cfg.blocks.remove(&block_id).unwrap();
            extracted_blocks.push(block);
        }

        extracted_blocks
    }

    fn gather_branch_body_ids(
        branch_id: BlockId,
        merge_id: BlockId,
        cfg: &StructuredCFG,
    ) -> HashSet<BlockId> {
        let mut body = HashSet::new();

        let mut current = branch_id;
        while current != merge_id {
            let current_block = cfg.blocks.get(&current).unwrap();
            let [successor] = current_block.successors[..] else {
                unreachable!()
            };

            body.insert(current);
            current = successor;
        }

        body
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

    fn compute_immediate_post_dominators(&self, cfg: &StructuredCFG) -> ImmediateDominators {
        let edges = Self::get_inverted_edges(cfg);

        let blocks: Vec<GraphNode> = edges
            .iter()
            .map(|(id, (succ, pred))| (id, succ, pred))
            .map(Into::into)
            .collect();

        CHKAlgorithm::execute(&blocks, VIRTUAL_EXIT_ID)
    }
}
