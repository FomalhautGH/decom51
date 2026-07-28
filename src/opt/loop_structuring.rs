use std::collections::{HashMap, HashSet};

use crate::{
    common::BlockId,
    opt::{
        cfg::ControlFlowGraph,
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
            for pred in &mut block.predecessors {
                *pred = self.resolve_alias(*pred);
            }

            block.predecessors.retain(|pred| *pred != block.id);
            block.predecessors.sort();
            block.predecessors.dedup();

            for succ in &mut block.successors {
                *succ = self.resolve_alias(*succ);
            }

            block.successors.retain(|succ| *succ != block.id);
            block.successors.sort();
            block.successors.dedup();
        }

        if cfg!(debug_assertions) {
            println!("======================== LOOP STRUCTURING ========================");
            print!("{}", cfg);
        }
    }

    fn collapse(&mut self, loop_tree: LoopTree, cfg: &mut StructuredCFG) {
        for loop_tree in loop_tree.children {
            self.collapse(loop_tree, cfg);
        }

        let loop_header = loop_tree.header;
        let header_block = cfg.blocks.get(&loop_header).unwrap();

        // Find all the ids we need, even the ones we moved to a macro block
        let mut body_block_ids = HashSet::new();
        for block_id in &loop_tree.body {
            let alias_id = self.resolve_alias(*block_id);
            body_block_ids.insert(alias_id);
        }

        // Calculating the predecessors and successors of the macro block, successors are
        // determined by exiting to an external block
        let mut predecessors = HashSet::new();
        let mut successors = HashSet::new();

        for predecessor in &header_block.predecessors {
            let predecessor = self.resolve_alias(*predecessor);
            if !body_block_ids.contains(&predecessor) {
                predecessors.insert(predecessor);
            }
        }

        for block_id in &body_block_ids {
            let body_block = cfg.blocks.get(block_id).unwrap();
            for successor in &body_block.successors {
                let successor = self.resolve_alias(*successor);
                if !body_block_ids.contains(&successor) {
                    successors.insert(successor);
                }
            }
        }

        // Increment the id for the new macro block
        let new_block_id = self.next_block_id;
        self.next_block_id += 1;

        // Removing the ids from the main cfg
        let mut nested_blocks = Vec::new();
        for block_id in &body_block_ids {
            let block = cfg.blocks.remove(block_id).unwrap();
            nested_blocks.push(block);
            self.block_aliases.insert(*block_id, new_block_id);
        }

        nested_blocks.sort_by_key(|b| b.id);
        let macro_block = StructuredBlock {
            id: new_block_id,
            successors: successors.into_iter().collect(),
            predecessors: predecessors.into_iter().collect(),
            kind: BlockKind::Macro(nested_blocks),
        };

        cfg.blocks.insert(macro_block.id, macro_block);
    }

    fn resolve_alias(&self, id: BlockId) -> BlockId {
        let mut current_id = id;

        while let Some(&alias_id) = self.block_aliases.get(&current_id) {
            current_id = alias_id;
        }

        current_id
    }
}
