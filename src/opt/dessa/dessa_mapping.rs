use std::collections::{HashMap, HashSet};

use crate::{
    common::{
        Ir51,
        Operand::{self, Temp},
        Phi, Register, SSAVar, VarId,
    },
    opt::{
        cfg::{BasicBlock, ControlFlowGraph},
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
            for phi in &block.phis {
                self.analyze_phi(phi);
            }
            for ir in &block.irs {
                self.analyze_ir(ir);
            }
        }

        for block in cfg.blocks.values_mut() {
            for phi in &mut block.phis {
                self.map_phi(phi);
            }
            for ir in &mut block.irs {
                self.map_ir(ir);
            }
        }

        if cfg!(debug_assertions) {
            println!("======================== DE-SSA MAPPING ========================");
            let mut sorted_blocks: Vec<&BasicBlock> = cfg.blocks.values().collect();
            sorted_blocks.sort_by_key(|b| b.id);
            for block in sorted_blocks {
                print!("{}", block);
            }
        }
    }

    fn analyze_phi(&mut self, phi: &Phi) {
        self.analyze_operand(&phi.operand);
        for incoming in phi.incoming.values() {
            self.analyze_operand(incoming);
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

    fn map_phi(&self, phi: &mut Phi) {
        self.map(&mut phi.operand);
        for incoming in phi.incoming.values_mut() {
            self.map(incoming);
        }
    }

    fn map_ir(&self, ir: &mut Ir51) {
        match ir {
            Ir51::Move { dest, src } => {
                self.map(dest);
                self.map(src);
            }

            Ir51::BinOp { dest, lhs, rhs, .. } => {
                self.map(lhs);
                self.map(rhs);
                self.map(dest);
            }

            Ir51::Store {
                dest_ptr, value, ..
            } => {
                self.map(dest_ptr);
                self.map(value);
            }

            Ir51::Load { dest, src_ptr, .. } => {
                self.map(src_ptr);
                self.map(dest);
            }

            Ir51::BranchIf { cond, .. } => {
                self.map(cond);
            }

            Ir51::Nop | Ir51::Jump(_) | Ir51::Call(_) | Ir51::Return => {}
        }
    }

    fn map(&self, op: &mut Operand) -> Option<()> {
        match op {
            Operand::SSAVar {
                addr,
                version,
                size,
            } => {
                let ssaop = (*addr, *version);
                let id = *self.ssa_mappings.get(&ssaop)?;

                // TODO: here we are hardcoding the mapping back to the SP register
                // maybe use a specific phase for what we need to map back
                if *addr == 0x81 {
                    *op = Operand::Register(Register::SP)
                } else {
                    *op = Operand::Local { id, size: *size };
                }

                Some(())
            }

            Operand::TempSSAVar { index, .. } => {
                *op = Temp(*index);
                Some(())
            }

            Operand::Register(_)
            | Operand::Constant(_)
            | Operand::Temp(_)
            | Operand::Var { .. }
            | Operand::Local { .. } => None,
        }
    }
}
