use std::collections::HashMap;

use crate::{
    lir::VarId,
    mir::Mir51,
    opt::cfg::{BasicBlock, ControlFlowGraph},
};

#[derive(Default)]
pub struct DeadStoreElimination {
    last_stores: HashMap<VarId, usize>,
}

impl DeadStoreElimination {
    pub fn execute(&mut self, cfg: &mut ControlFlowGraph) {
        for block in cfg.blocks.values_mut() {
            self.execute_on_block(block);
        }

        if cfg!(debug_assertions) {
            println!("======================== DEAD STORE ELIMINATION ========================");
            let mut sorted_blocks: Vec<&BasicBlock> = cfg.blocks.values().collect();
            sorted_blocks.sort_by_key(|block| block.id);
            for block in sorted_blocks {
                print!("{}", block);
            }
        }

        cfg.remove_useless_irs();
    }

    fn execute_on_block(&mut self, block: &mut BasicBlock) {
        self.last_stores.clear();
        let mut dead_store_indices = Vec::new();

        for (mir_idx, mir) in block.mirs.iter().enumerate() {
            if let Mir51::Assign { dest, src } = mir {
                // If a variable is read, its previous assignment cannot be eliminated,
                // so remove it from the tracked stores
                if let Some(src_var_id) = src.var_id() {
                    self.last_stores.remove(&src_var_id);
                    continue;
                }

                // If a previous assignment to this variable exists without an intervening read,
                // mark it as a dead store to be eliminated
                if let Some(dest_var_id) = dest.var_id() {
                    if let Some(prev_idx) = self.last_stores.remove(&dest_var_id) {
                        dead_store_indices.push(prev_idx);
                    }

                    self.last_stores.insert(dest_var_id, mir_idx);
                }
            }
        }

        for &dead_idx in &dead_store_indices {
            block.mirs[dead_idx] = Mir51::Nop;
        }
    }
}
