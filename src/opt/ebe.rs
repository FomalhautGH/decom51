use std::collections::HashMap;

use crate::{
    common::{BlockId, Ir51},
    opt::cfg_restorer::{BasicBlock, ControlFlowGraph},
};

// This phase does edge contration for blocks with just one jump
pub struct EmptyBlockElimination;

impl EmptyBlockElimination {
    pub fn execute(cfg: &mut ControlFlowGraph) {
        let substitution_map = Self::find_substitutes(cfg);
        Self::substitute(&substitution_map, cfg);
    }

    fn substitute(map: &HashMap<BlockId, BlockId>, cfg: &mut ControlFlowGraph) {
        for block in cfg.blocks.values_mut() {
            for suc in block.successors.iter_mut() {
                Self::map_jump(suc, map);
            }

            for ir in block.irs.iter_mut() {
                Self::substitute_refs(ir, map);
            }
        }
    }

    fn substitute_refs(ir: &mut Ir51, map: &HashMap<BlockId, BlockId>) {
        match ir {
            Ir51::Jump(block) => Self::map_jump(block, map),
            Ir51::BranchIf {
                true_jmp: true_block,
                false_jmp: false_block,
                ..
            } => {
                Self::map_jump(true_block, map);
                Self::map_jump(false_block, map);
            }

            Ir51::Move { .. } => {}
            Ir51::BinOp { .. } => {}
            Ir51::Store { .. } => {}
            Ir51::Load { .. } => {}
            Ir51::Call(_) => {}
            Ir51::Return => {}
            Ir51::Nop => {}
        }
    }

    fn map_jump(jump: &mut BlockId, map: &HashMap<BlockId, BlockId>) {
        if let Some(substitute) = map.get(jump) {
            *jump = *substitute;
        }
    }

    fn find_substitutes(cfg: &ControlFlowGraph) -> HashMap<BlockId, BlockId> {
        let mut substitution_map = HashMap::new();

        for block in cfg.blocks.values() {
            if let Some(sub) = Self::find_substitute(block) {
                substitution_map.insert(block.id, sub);
            }
        }

        substitution_map
    }

    fn find_substitute(block: &BasicBlock) -> Option<BlockId> {
        if let [Ir51::Jump(sub)] = block.irs[..] {
            Some(sub)
        } else {
            None
        }
    }
}
