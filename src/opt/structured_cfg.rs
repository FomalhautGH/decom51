use std::collections::HashMap;

use crate::{
    common::BlockId,
    mir::Mir51,
    opt::cfg::{BasicBlock, ControlFlowGraph},
};

#[derive(Debug, Clone, Default)]
pub struct FlatBlock {
    pub id: BlockId,
    pub mirs: Vec<Mir51>,
    pub successors: Vec<BlockId>,
    pub predecessors: Vec<BlockId>,
}

impl FlatBlock {
    pub fn from(block: BasicBlock) -> Self {
        Self {
            id: block.id,
            mirs: block.mirs,
            successors: block.successors,
            predecessors: block.predecessors,
        }
    }
}

#[derive(Debug)]
pub enum StructuredBlock {
    Flat(FlatBlock),
    Macro {
        header: BlockId,
        body: Vec<StructuredBlock>,
    },
}

pub type StructuredBlockMap = HashMap<BlockId, StructuredBlock>;

#[derive(Debug)]
pub struct StructuredCFG {
    pub blocks: StructuredBlockMap,
    pub start_block: BlockId,
}

impl StructuredCFG {
    pub fn from(cfg: ControlFlowGraph) -> Self {
        let mut flat_blocks: StructuredBlockMap = HashMap::new();

        for block in cfg.blocks.into_values() {
            let flat_block = FlatBlock::from(block);
            flat_blocks.insert(flat_block.id, StructuredBlock::Flat(flat_block));
        }

        Self {
            blocks: flat_blocks,
            start_block: cfg.start_block,
        }
    }
}
