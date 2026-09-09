use std::collections::{HashMap, HashSet};

use crate::{
    common::BlockId,
    opt::{
        loop_recovery::LoopTree,
        structured_cfg::{BlockKind, StructuredBlock, StructuredCFG},
    },
};

pub struct LoopStructuring {
    next_block_id: usize,
    block_aliases: HashMap<BlockId, BlockId>,
}

impl LoopStructuring {
    pub fn new(cfg: &StructuredCFG) -> Self {
        Self {
            next_block_id: cfg.blocks.keys().max().copied().unwrap_or(0) + 1,
            block_aliases: HashMap::new(),
        }
    }

    pub fn execute(&mut self, cfg: &mut StructuredCFG, loop_trees: Vec<LoopTree>) {
        for loop_tree in loop_trees {
            self.collapse(loop_tree, cfg);
        }

        for block in cfg.blocks.values_mut() {
            self.rename_edges(&mut block.predecessors, block.id);
            self.rename_edges(&mut block.successors, block.id);
        }

        cfg.start_block = self.resolve_alias(cfg.start_block);

        if cfg!(debug_assertions) {
            println!("======================== LOOP STRUCTURING ========================");
            print!("{}", cfg);
        }
    }

    // Phase 1: Macro Block Collapsing & Transposition
    fn collapse(&mut self, loop_tree: LoopTree, cfg: &mut StructuredCFG) {
        for child_tree in loop_tree.children {
            self.collapse(child_tree, cfg);
        }

        let header_block = cfg.blocks.get(&loop_tree.header).unwrap();

        let resolved_body_ids = self.gather_resolved_body_ids(&loop_tree.body);
        let (predecessors, successors) = self.compute_edges(header_block, &resolved_body_ids, cfg);
        let macro_block_id = self.allocate_block_id();
        let (header, nested_blocks) =
            self.transpose_blocks(cfg, loop_tree.header, &resolved_body_ids, macro_block_id);

        // Insert macro block into the Structured CFG
        let macro_block = StructuredBlock {
            id: macro_block_id,
            successors: successors.into_iter().collect(),
            predecessors: predecessors.into_iter().collect(),
            kind: BlockKind::Loop {
                header: Box::new(header),
                body: nested_blocks,
            },
        };

        cfg.blocks.insert(macro_block.id, macro_block);
    }

    // Resolves aliases for all block IDs in the loop body
    fn gather_resolved_body_ids(&self, loop_body: &HashSet<BlockId>) -> HashSet<BlockId> {
        let mut resolved_body_ids = HashSet::new();

        for &block_id in loop_body {
            let resolved_id = self.resolve_alias(block_id);
            resolved_body_ids.insert(resolved_id);
        }

        resolved_body_ids
    }

    // Calculates the external predecessors (incoming) and successors (loop exits) of the macro block
    fn compute_edges(
        &self,
        header_block: &StructuredBlock,
        body_block_ids: &HashSet<BlockId>,
        cfg: &StructuredCFG,
    ) -> (HashSet<BlockId>, HashSet<BlockId>) {
        let mut predecessors = HashSet::new();
        let mut successors = HashSet::new();

        for &pred_id in &header_block.predecessors {
            let resolved_pred = self.resolve_alias(pred_id);
            if !body_block_ids.contains(&resolved_pred) {
                predecessors.insert(resolved_pred);
            }
        }

        for &suc_id in &header_block.successors {
            let resolved_suc = self.resolve_alias(suc_id);
            if !body_block_ids.contains(&resolved_suc) {
                successors.insert(resolved_suc);
            }
        }

        for &block_id in body_block_ids {
            let body_block = cfg.blocks.get(&block_id).unwrap_or_else(|| {
                panic!("Block with id: {block_id} wasn't found");
            });

            for &succ_id in &body_block.successors {
                let resolved_succ = self.resolve_alias(succ_id);
                if !body_block_ids.contains(&resolved_succ) {
                    successors.insert(resolved_succ);
                }
            }
        }

        (predecessors, successors)
    }

    // Removes the loop body blocks from the main CFG and packages them into the macro block
    fn transpose_blocks(
        &mut self,
        cfg: &mut StructuredCFG,
        header_id: BlockId,
        body_block_ids: &HashSet<BlockId>,
        macro_block_id: BlockId,
    ) -> (StructuredBlock, Vec<StructuredBlock>) {
        let mut gathered_blocks = HashMap::new();

        let resolved_header_id = self.resolve_alias(header_id);
        let header_block = cfg.blocks.remove(&resolved_header_id).unwrap();
        for &block_id in body_block_ids {
            if let Some(block) = cfg.blocks.remove(&block_id) {
                gathered_blocks.insert(block.id, block);
            }
        }

        let ordered_nested_blocks =
            self.topological_sort(gathered_blocks, &header_block, body_block_ids);

        self.block_aliases.insert(header_id, macro_block_id);
        self.block_aliases
            .insert(resolved_header_id, macro_block_id);

        for &block_id in body_block_ids {
            self.block_aliases.insert(block_id, macro_block_id);
        }

        (header_block, ordered_nested_blocks)
    }

    fn topological_sort(
        &self,
        mut gathered_blocks: HashMap<BlockId, StructuredBlock>,
        header_block: &StructuredBlock,
        body_block_ids: &HashSet<BlockId>,
    ) -> Vec<StructuredBlock> {
        let Some(start_block_id) = header_block
            .successors
            .iter()
            .map(|s| self.resolve_alias(*s))
            .find(|s| body_block_ids.contains(s))
        else {
            return gathered_blocks.into_values().collect();
        };

        let mut visited_blocks = HashSet::new();
        let mut visit_stack = vec![(start_block_id, false)];
        let mut topologic_order = Vec::new();

        while let Some((block_id, is_black)) = visit_stack.pop() {
            let Some(actual_block) = gathered_blocks.get(&block_id) else {
                continue;
            };

            if is_black {
                topologic_order.push(block_id);
            } else {
                if !visited_blocks.insert(block_id) {
                    continue;
                }

                visit_stack.push((block_id, true));
                for suc in &actual_block.successors {
                    let suc = self.resolve_alias(*suc);
                    if gathered_blocks.contains_key(&suc) {
                        visit_stack.push((suc, false));
                    }
                }
            }
        }

        let mut ordered_blocks = Vec::new();
        for block_id in topologic_order.iter().rev() {
            if let Some(block) = gathered_blocks.remove(block_id) {
                ordered_blocks.push(block);
            }
        }

        ordered_blocks
    }

    // Allocates a unique ID for the new macro block
    fn allocate_block_id(&mut self) -> BlockId {
        let id = self.next_block_id;
        self.next_block_id += 1;
        id
    }

    // Phase 2: Global Edge Renaming & Alias Resolution
    fn rename_edges(&self, edges: &mut Vec<BlockId>, current_block_id: BlockId) {
        for target_id in &mut *edges {
            *target_id = self.resolve_alias(*target_id);
        }

        edges.retain(|&target_id| target_id != current_block_id);
        edges.sort();
        edges.dedup();
    }

    fn resolve_alias(&self, block_id: BlockId) -> BlockId {
        let mut current_id = block_id;

        while let Some(&aliased_id) = self.block_aliases.get(&current_id) {
            current_id = aliased_id;
        }

        current_id
    }
}
