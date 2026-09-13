use std::collections::{HashMap, HashSet};

use crate::{
    common::{BlockId, Phi},
    lir::{Ir51, Operand, VarId},
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

    pub fn merge_with(&mut self, mut rhs: BasicBlock) {
        self.irs.append(&mut rhs.irs);
        self.mirs.append(&mut rhs.mirs);
        self.phis.append(&mut rhs.phis);
        self.successors = rhs.successors;
    }

    pub fn remove_useless_initializations(&mut self) {
        let mut initialized_vars = HashSet::new();

        for ir in self.irs.iter() {
            if let Ir51::Move { dest, .. } = ir
                && let Operand::Local { id, .. } = *dest
            {
                initialized_vars.insert(id);
            }
        }

        let mut useless_idxs = HashSet::new();
        for var in initialized_vars {
            let mut prev_init = None;
            let mut read = false;

            for (i, ir) in self.irs.iter().enumerate() {
                if let Some((_, id)) = prev_init {
                    read = read || Self::was_it_used_here(ir, id);
                }

                if let Ir51::Move { dest, .. } = ir
                    && let Operand::Local { id, .. } = *dest
                    && var == id
                {
                    if !read && let Some((idx, _)) = prev_init {
                        useless_idxs.insert(idx);
                    }

                    prev_init = Some((i, id));
                    read = false;
                    continue;
                }
            }
        }

        for (i, ir) in self.irs.iter_mut().enumerate() {
            if useless_idxs.contains(&i) {
                *ir = Ir51::Nop;
            }
        }
    }

    fn was_it_used_here(ir: &Ir51, varid: VarId) -> bool {
        let used = |op: &Operand| -> bool {
            if let Operand::Local { id, .. } = op {
                return *id == varid;
            }

            false
        };

        match ir {
            Ir51::Store {
                dest_ptr, value, ..
            } => used(dest_ptr) || used(value),

            Ir51::BinOp { lhs, rhs, .. } => used(lhs) || used(rhs),
            Ir51::Move { src, .. } => used(src),
            Ir51::Load { src_ptr, .. } => used(src_ptr),
            Ir51::BranchIf { cond, .. } => used(cond),
            Ir51::Nop | Ir51::Jump(_) | Ir51::Call(_) | Ir51::Return => false,
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
            block
                .phis
                .retain(|phi| !matches!(phi.operand, Operand::Constant(0)));

            block.remove_useless_initializations();
            block.irs.retain(|ir| !matches!(ir, Ir51::Nop));
            block.mirs.retain(|ir| !matches!(ir, Mir51::Nop));
        }
    }

    pub fn merge_blocks(&mut self) {
        let mut to_merge_edges = Vec::new();

        for block in self.blocks.values() {
            let [successor_id] = block.successors[..] else {
                continue;
            };

            let successor = self.blocks.get(&successor_id).unwrap();

            let [predecessor_id] = successor.predecessors[..] else {
                continue;
            };

            if block.id == predecessor_id {
                to_merge_edges.push((block.id, successor_id));
            }
        }

        for (b1_id, b2_id) in to_merge_edges {
            let b2 = self.blocks.remove(&b2_id).unwrap();
            let b1 = self.blocks.get_mut(&b1_id).unwrap();

            if let Some(Mir51::Goto(block_id)) = b1.mirs.last()
                && *block_id == b2_id
            {
                b1.mirs.pop();
            }

            b1.merge_with(b2);
        }
    }
}
