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

        if cfg!(debug_assertions) {
            println!("======================== LOOP STRUCTURING ========================");
            print!("{}", cfg);
        }
    }

    fn rename_edges(&self, edges: &mut Vec<BlockId>, inside_block_id: BlockId) {
        for edge in &mut *edges {
            *edge = self.resolve_alias(*edge);
        }

        edges.retain(|succ| *succ != inside_block_id);
        edges.sort();
        edges.dedup();
    }

    fn collapse(&mut self, loop_tree: LoopTree, cfg: &mut StructuredCFG) {
        for loop_tree in loop_tree.children {
            self.collapse(loop_tree, cfg);
        }

        let header_block = cfg.blocks.get(&loop_tree.header).unwrap();

        let body_block_ids = self.gather_ids(&loop_tree.body);
        let (predecessors, successors) = self.compute_edges(header_block, &body_block_ids, cfg);
        let new_block_id = self.new_id();
        let nested_blocks = self.transpose_blocks(&body_block_ids, cfg, new_block_id);

        // Insert block in the Graph
        let macro_block = StructuredBlock {
            id: new_block_id,
            successors: successors.into_iter().collect(),
            predecessors: predecessors.into_iter().collect(),
            kind: BlockKind::Macro(nested_blocks),
        };

        cfg.blocks.insert(macro_block.id, macro_block);
    }

    // Removing the ids from the main cfg
    fn transpose_blocks(
        &mut self,
        body_block_ids: &HashSet<BlockId>,
        cfg: &mut StructuredCFG,
        into_block_id: BlockId,
    ) -> Vec<StructuredBlock> {
        let mut nested_blocks = Vec::new();

        for block_id in body_block_ids {
            let block = cfg.blocks.remove(block_id).unwrap();
            nested_blocks.push(block);
            self.block_aliases.insert(*block_id, into_block_id);
        }

        nested_blocks.sort_by_key(|b| b.id);
        nested_blocks
    }

    // Calculating the predecessors and successors of the macro block, successors are
    // determined by exiting to an external block
    fn compute_edges(
        &self,
        header_block: &StructuredBlock,
        body_block_ids: &HashSet<BlockId>,
        cfg: &StructuredCFG,
    ) -> (HashSet<BlockId>, HashSet<BlockId>) {
        let mut predecessors = HashSet::new();
        let mut successors = HashSet::new();

        for predecessor in &header_block.predecessors {
            let predecessor = self.resolve_alias(*predecessor);
            if !body_block_ids.contains(&predecessor) {
                predecessors.insert(predecessor);
            }
        }

        for block_id in body_block_ids {
            let body_block = cfg.blocks.get(block_id).unwrap();
            for successor in &body_block.successors {
                let successor = self.resolve_alias(*successor);
                if !body_block_ids.contains(&successor) {
                    successors.insert(successor);
                }
            }
        }

        (predecessors, successors)
    }

    // Find all the ids we need, even the ones we moved to a macro block
    fn gather_ids(&self, body: &HashSet<BlockId>) -> HashSet<BlockId> {
        let mut body_block_ids = HashSet::new();

        for block_id in body {
            let alias_id = self.resolve_alias(*block_id);
            body_block_ids.insert(alias_id);
        }

        body_block_ids
    }

    fn resolve_alias(&self, id: BlockId) -> BlockId {
        let mut current_id = id;

        while let Some(&alias_id) = self.block_aliases.get(&current_id) {
            current_id = alias_id;
        }

        current_id
    }

    // Increment the id for the new macro block
    fn new_id(&mut self) -> BlockId {
        self.next_block_id += 1;
        self.next_block_id - 1
    }
}
