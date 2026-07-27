use std::collections::HashMap;

use crate::{
    common::{BlockId, Phi},
    lir::{Ir51, Operand},
    mir::Mir51,
};

#[derive(Debug, Clone, Default)]
pub struct BasicBlock {
    pub id: BlockId,
    pub irs: Vec<Ir51>,
    pub mirs: Vec<Mir51>,
    pub successors: Vec<BlockId>,
    pub predecessors: Vec<BlockId>,
    pub phis: Vec<Phi>,
}

impl BasicBlock {
    // TODO: Check if there are places where i can use this new function
    pub fn new(id: BlockId) -> Self {
        Self {
            id,
            ..Default::default()
        }
    }
}

pub type HashBlocks = HashMap<BlockId, BasicBlock>;

#[derive(Debug)]
pub struct ControlFlowGraph {
    pub blocks: HashBlocks,
    pub start_block: BlockId,
}

// TODO !important: all the phases that we do on the controlflow graph should be
// their respective method
impl ControlFlowGraph {
    pub fn remove_useless_irs(&mut self) {
        // TODO: Use a better 'null' operand for the phis
        for block in self.blocks.values_mut() {
            block
                .irs
                .retain(|ir| !matches!(ir, Ir51::Move { dest, src } if dest == src));
            block.irs.retain(|ir| !matches!(ir, Ir51::Nop));
            block
                .phis
                .retain(|phi| !matches!(phi.operand, Operand::Constant(0)));
        }
    }
}
