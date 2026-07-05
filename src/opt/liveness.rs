use crate::{
    common::{BlockId, Ir51, Operand, Phi, SSAVar},
    opt::cfg_restorer::{BasicBlock, ControlFlowGraph},
};
use std::collections::{HashMap, HashSet};

// Local set for every block
type LocalSet = HashMap<BlockId, HashSet<SSAVar>>;

#[derive(Default)]
pub struct Liveness {
    use_sets: LocalSet,
    def_sets: LocalSet,
    in_sets: LocalSet,
    out_sets: LocalSet,
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
            println!("Use Set: {:?}; Def Set: {:?}", self.use_sets, self.def_sets);
            println!("In Set: {:?}; Out Set: {:?}", self.in_sets, self.out_sets);
        }
    }

    fn in_out_analysis(&mut self, cfg: &ControlFlowGraph) {
        let mut changed = true;
        while changed {
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
        } else {
            *changed = false;
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

    fn use_def_on_ir(&mut self, ir: &Ir51, blockid: BlockId) {
        match ir {
            Ir51::Move { dest, src } => {
                self.save_def(dest, blockid);
                self.save_use(src, blockid);
            }

            Ir51::BinOp { dest, lhs, rhs, .. } => {
                self.save_def(dest, blockid);
                self.save_use(lhs, blockid);
                self.save_use(rhs, blockid);
            }

            Ir51::Store {
                dest_ptr, value, ..
            } => {
                self.save_use(dest_ptr, blockid);
                self.save_use(value, blockid);
            }

            Ir51::Load { dest, src_ptr, .. } => {
                self.save_def(dest, blockid);
                self.save_use(src_ptr, blockid);
            }

            Ir51::BranchIf { cond, .. } => self.save_use(cond, blockid),

            Ir51::Nop | Ir51::Jump(_) | Ir51::Call(_) | Ir51::Return => {}
            Ir51::InvalidIr => unreachable!("Invalid IR Touched"),
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
            Operand::Address(_, _) | Operand::Register(_) | Operand::Temporary(_) => unreachable!(),
        }
    }

    fn save_use(&mut self, operand: &Operand, blockid: BlockId) {
        match operand {
            Operand::SSAVar { addr, version } => {
                let key = (*addr, *version);

                let def = self
                    .def_sets
                    .get(&blockid)
                    .is_some_and(|def_set| def_set.contains(&key));

                if !def {
                    self.use_sets.entry(blockid).or_default().insert(key);
                }
            }

            // We ignore Temps in this pass
            Operand::TempSSAVar { .. } | Operand::Constant(_) => {}
            Operand::Address(_, _) | Operand::Register(_) | Operand::Temporary(_) => unreachable!(),
        }
    }
}
