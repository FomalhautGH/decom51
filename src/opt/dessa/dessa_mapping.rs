use std::collections::{HashMap, HashSet};

use crate::{
    common::{Ir51, Operand, SSAVar, VarId},
    opt::{
        cfg_restorer::ControlFlowGraph,
        dessa::interference::{InterferenceGraph, do_they_interfere},
    },
};

// Addr mapping tells if for an address there's already a mapping in
// ssa_mappings and for which SSAVar specifically
pub struct DeSSAMapping {
    counter: usize,
    ssa_mappings: HashMap<SSAVar, VarId>,
    interference_graph: InterferenceGraph,
}

impl DeSSAMapping {
    pub fn new(interference_graph: InterferenceGraph) -> Self {
        Self {
            counter: 1,
            ssa_mappings: HashMap::new(),
            interference_graph,
        }
    }

    pub fn execute(&mut self, cfg: &mut ControlFlowGraph) {
        for block in cfg.blocks.values() {
            for ir in &block.irs {
                self.analyze_ir(ir);
            }
        }

        for block in cfg.blocks.values_mut() {
            for ir in &mut block.irs {
                self.map_ir(ir);
            }
        }

        if cfg!(debug_assertions) {
            println!("======================== DE-SSA MAPPING ========================");
            for block in cfg.blocks.values() {
                println!("Block {}:", block.id);
                for (i, ir) in block.irs.iter().enumerate() {
                    println!("    {i}: {ir:?}")
                }
            }
        }
    }

    fn analyze_ir(&mut self, ir: &Ir51) {
        match ir {
            Ir51::Move { dest, src } => {
                self.analyze_operand(src);
                self.analyze_operand(dest);
            }

            Ir51::BinOp { dest, lhs, rhs, .. } => {
                self.analyze_operand(lhs);
                self.analyze_operand(rhs);
                self.analyze_operand(dest);
            }

            Ir51::Store {
                dest_ptr, value, ..
            } => {
                self.analyze_operand(dest_ptr);
                self.analyze_operand(value);
            }

            Ir51::Load { dest, src_ptr, .. } => {
                self.analyze_operand(src_ptr);
                self.analyze_operand(dest);
            }

            Ir51::BranchIf { cond, .. } => self.analyze_operand(cond),

            Ir51::Nop | Ir51::Jump(_) | Ir51::Call(_) | Ir51::Return => {}
            Ir51::InvalidIr => unreachable!("Invalid IR Touched"),
        }
    }

    fn analyze_operand(&mut self, operand: &Operand) {
        let Some(ssavar) = operand.to_ssavar() else {
            return;
        };
        if self.ssa_mappings.contains_key(&ssavar) {
            return;
        }

        // Find all the varid that interefere
        let addr = ssavar.0;
        let interfering_varids: HashSet<VarId> = self
            .ssa_mappings
            .iter()
            .filter(|m| m.0.0 == addr && do_they_interfere(&self.interference_graph, *m.0, ssavar))
            .map(|(_, &varid)| varid)
            .collect();

        // Use the first varid that is not part of the interfering varid as a reuse
        let reused_id = self
            .ssa_mappings
            .iter()
            .filter(|m| m.0.0 == addr)
            .map(|(_, &varid)| varid)
            .find(|varid| !interfering_varids.contains(varid));

        if let Some(reused_id) = reused_id {
            self.ssa_mappings.insert(ssavar, reused_id);
        } else {
            self.ssa_mappings.insert(ssavar, self.counter);
            self.counter += 1;
        }
    }

    fn map_ir(&self, ir: &mut Ir51) {
        // TODO: Also map tempssavar to something else
        let map = |op: &mut Operand| -> Option<()> {
            let ssaop = op.to_ssavar()?;
            let id = *self.ssa_mappings.get(&ssaop)?;

            *op = Operand::Var(id);

            Some(())
        };

        match ir {
            Ir51::Move { dest, src } => {
                map(dest);
                map(src);
            }

            Ir51::BinOp { dest, lhs, rhs, .. } => {
                map(lhs);
                map(rhs);
                map(dest);
            }

            Ir51::Store {
                dest_ptr, value, ..
            } => {
                map(dest_ptr);
                map(value);
            }

            Ir51::Load { dest, src_ptr, .. } => {
                map(src_ptr);
                map(dest);
            }

            Ir51::BranchIf { cond, .. } => {
                map(cond);
            }

            Ir51::Nop | Ir51::Jump(_) | Ir51::Call(_) | Ir51::Return => {}
            Ir51::InvalidIr => unreachable!("Invalid IR Touched"),
        }
    }
}
