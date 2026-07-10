use std::collections::HashMap;

use crate::{
    common::{Ir51, Operand, SSAVar, VarId},
    opt::{
        cfg_restorer::ControlFlowGraph,
        dessa::interference::{InterferenceGraph, do_they_interfere},
    },
};

// Addr mapping tells if for an address there's already a mapping in
// ssa_mappings and for which SSAVar specifically
pub struct SSAMapping {
    counter: usize,
    ssa_mappings: HashMap<SSAVar, VarId>,
    interference_graph: InterferenceGraph,
}

impl SSAMapping {
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

        let addr = ssavar.0;
        let mut interfere = false;

        // check for mapping for the same address as the ssavar we are trying to map
        let mut original_id: VarId = 0;
        for (ssavar_same_addr, varid) in self.ssa_mappings.iter().filter(|m| m.0.0 == addr) {
            interfere =
                interfere || do_they_interfere(&self.interference_graph, ssavar, *ssavar_same_addr);
            original_id = *varid;
        }

        if interfere || original_id == 0 {
            self.ssa_mappings.insert(ssavar, self.counter);
            self.counter += 1;
        } else {
            self.ssa_mappings.insert(ssavar, original_id);
        }
    }

    fn map_ir(&self, ir: &mut Ir51) {
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
