use crate::{
    common::{BlockId, Ir51, Operand, Phi, SSAVar},
    lir::format_addr,
    opt::cfg::{BasicBlock, ControlFlowGraph},
};
use std::collections::{HashMap, HashSet};

// Local set for every block
pub type LocalSet = HashMap<BlockId, HashSet<SSAVar>>;

#[derive(Default)]
pub struct Liveness {
    pub in_sets: LocalSet,
    pub out_sets: LocalSet,
    use_sets: LocalSet,
    def_sets: LocalSet,
}

impl Liveness {
    pub fn new(cfg: &ControlFlowGraph) -> Self {
        let mut liveness = Liveness::default();

        for block in cfg.blocks.values() {
            liveness.in_sets.insert(block.id, HashSet::new());
            liveness.out_sets.insert(block.id, HashSet::new());
            liveness.use_sets.insert(block.id, HashSet::new());
            liveness.def_sets.insert(block.id, HashSet::new());
        }

        liveness
    }

    pub fn analysis(&mut self, cfg: &ControlFlowGraph) {
        self.use_def_analysis(cfg); // Phase 1
        self.in_out_analysis(cfg); // Phase 2

        if cfg!(debug_assertions) {
            println!("======================== LIVENESS ANALYSIS ========================");
            let mut sorted_blocks: Vec<BlockId> = self.use_sets.keys().copied().collect();
            sorted_blocks.sort();
            for &block_id in &sorted_blocks {
                let format_set = |set: &HashSet<SSAVar>| -> String {
                    let mut ssa_strs: Vec<String> = set
                        .iter()
                        .map(|s| format!("{}_{}", format_addr(s.0), s.1))
                        .collect();
                    ssa_strs.sort();
                    format!("{{{}}}", ssa_strs.join(", "))
                };
                println!("Block {}:", block_id);
                println!("  UseSet: {}", format_set(&self.use_sets[&block_id]));
                println!("  DefSet: {}", format_set(&self.def_sets[&block_id]));
                println!("  InSet:  {}", format_set(&self.in_sets[&block_id]));
                println!("  OutSet: {}", format_set(&self.out_sets[&block_id]));
            }
        }
    }

    fn in_out_analysis(&mut self, cfg: &ControlFlowGraph) {
        let mut changed = true;
        while changed {
            changed = false;
            for block in cfg.blocks.values() {
                self.in_out_block(cfg, block, &mut changed);
            }
        }
    }

    fn in_out_block(&mut self, cfg: &ControlFlowGraph, block: &BasicBlock, changed: &mut bool) {
        // Getting in and out set from this block
        let (Some(in_set), Some(out_set), Some(use_set), Some(def_set)) = (
            self.in_sets.get(&block.id),
            self.out_sets.get(&block.id),
            self.use_sets.get(&block.id),
            self.def_sets.get(&block.id),
        ) else {
            unreachable!()
        };

        // We start with a fresh out set to check later if it changed something compared
        // to the old one
        let mut new_out_set: HashSet<SSAVar> = HashSet::new();

        for suc_idx in &block.successors {
            // All the in sets of the successors are inserted in the out set of this block
            let in_suc_set = &self.in_sets[suc_idx];
            new_out_set.extend(in_suc_set);

            let suc = &cfg.blocks[suc_idx];

            // If there's a phi in the successor that inherits a ssavar from this block
            // we also add it to out
            for phi in &suc.phis {
                let Some(Operand::SSAVar { addr, version }) = phi.incoming.get(&block.id) else {
                    continue;
                };

                let key = (*addr, *version);
                new_out_set.insert(key);
            }
        }

        let mut new_in_set: HashSet<SSAVar> = HashSet::new();
        new_in_set.extend(use_set);
        new_in_set.extend(new_out_set.difference(def_set));

        // Check if a set is changed in that case we update the sets and set
        // changed to true
        if new_in_set != *in_set || new_out_set != *out_set {
            let in_set = self.in_sets.get_mut(&block.id).unwrap();
            let out_set = self.out_sets.get_mut(&block.id).unwrap();
            *in_set = new_in_set;
            *out_set = new_out_set;

            *changed = true;
        }
    }

    // =================== Phase 1 ===================
    fn use_def_analysis(&mut self, cfg: &ControlFlowGraph) {
        for block in cfg.blocks.values() {
            for phi in &block.phis {
                self.use_def_on_phi(phi, block.id);
            }

            for ir in &block.irs {
                self.use_def_on_ir(ir, block.id);
            }
        }
    }

    fn use_def_on_phi(&mut self, phi: &Phi, blockid: BlockId) {
        self.save_def(&phi.operand, blockid);
    }

    // TODO: maybe add a method on BasicBlock that counts the definitions and usages
    // of a variable in a hashmap and returns it, i need that here, in expr aggregaton
    // and probably other places i don't remember right now
    fn use_def_on_ir(&mut self, ir: &Ir51, blockid: BlockId) {
        match ir {
            Ir51::Move { dest, src } => {
                self.save_use(src, blockid);
                self.save_def(dest, blockid);
            }

            Ir51::BinOp { dest, lhs, rhs, .. } => {
                self.save_use(lhs, blockid);
                self.save_use(rhs, blockid);
                self.save_def(dest, blockid);
            }

            Ir51::Store {
                dest_ptr, value, ..
            } => {
                self.save_use(dest_ptr, blockid);
                self.save_use(value, blockid);
            }

            Ir51::Load { dest, src_ptr, .. } => {
                self.save_use(src_ptr, blockid);
                self.save_def(dest, blockid);
            }

            Ir51::BranchIf { cond, .. } => self.save_use(cond, blockid),

            Ir51::Nop | Ir51::Jump(_) | Ir51::Call(_) | Ir51::Return => {}
        }
    }

    fn save_def(&mut self, operand: &Operand, blockid: BlockId) {
        // TODO: We could provably add a method to extract the key from a valid operand
        match operand {
            Operand::SSAVar { addr, version } => {
                let key = (*addr, *version);
                self.def_sets.entry(blockid).or_default().insert(key);
            }

            // We ignore Temps in this pass
            Operand::TempSSAVar { .. } | Operand::Constant(_) => {}
            Operand::Var(_, _)
            | Operand::Register(_)
            | Operand::Temp(_)
            | Operand::Local(_)
            | Operand::Concat(_, _) => unreachable!(),
        }
    }

    fn save_use(&mut self, operand: &Operand, blockid: BlockId) {
        match operand {
            Operand::SSAVar { addr, version } => {
                let key = (*addr, *version);

                let def = self.def_sets.entry(blockid).or_default().contains(&key);
                if !def {
                    self.use_sets.entry(blockid).or_default().insert(key);
                }
            }

            // We ignore Temps in this pass
            Operand::TempSSAVar { .. } | Operand::Constant(_) => {}
            Operand::Var(_, _)
            | Operand::Register(_)
            | Operand::Temp(_)
            | Operand::Local(_)
            | Operand::Concat(_, _) => unreachable!(),
        }
    }
}
