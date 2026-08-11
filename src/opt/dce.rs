use crate::{
    common::{BlockId, Ir51, Operand, Phi, SSAVar, TempSSAVar},
    lir::{format_addr, is_iram_addr},
    opt::cfg::{BasicBlock, ControlFlowGraph},
};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
enum UseLocation {
    Instruction(BlockId, usize),
    Phi(BlockId, usize),
}

// TODO: Maybe unify them? With a enum or smth
// These represent how many times a variable is used
type VariableUses = HashMap<SSAVar, usize>;
type TemporaryUses = HashMap<TempSSAVar, usize>;

// These represent where a variable is defined so we can remove that
// instruction if the uses of that variable decrease all the way to 0
type VariableDefs = HashMap<SSAVar, UseLocation>;
type TemporaryDefs = HashMap<TempSSAVar, UseLocation>;

#[derive(Default)]
pub struct DeadCodeElimination {
    variable_uses: VariableUses,
    temporary_uses: TemporaryUses,
    variable_defs: VariableDefs,
    temporary_defs: TemporaryDefs,
    dead_instructions: Vec<UseLocation>,
}

impl DeadCodeElimination {
    pub fn execute(&mut self, cfg: &mut ControlFlowGraph) {
        self.use_and_def_count(cfg); // Phase 1: Calculates how many times a variable/temp is used
        self.scan_irs(cfg); // Phase 2: Look at all the destinations of the ir instructions
        // and if they aren't used mark them as dead

        if cfg!(debug_assertions) {
            println!("======================== DEAD CODE ELIMINATION ========================");
            println!("Uses for SSA:");
            let mut sorted_vars: Vec<(&SSAVar, &usize)> = self.variable_uses.iter().collect();
            sorted_vars.sort_by_key(|(var, _)| format!("{}_{}", format_addr(var.0), var.1));
            for (var, uses) in sorted_vars {
                println!("  {}_{}: {}", format_addr(var.0), var.1, uses);
            }

            println!("Uses for TempSSA:");
            let mut sorted_temps: Vec<(&TempSSAVar, &usize)> = self.temporary_uses.iter().collect();
            sorted_temps.sort_by_key(|t| (t.0.0, t.0.1));
            for (temp, uses) in sorted_temps {
                println!("  t{}_{}: {}", temp.0, temp.1, uses);
            }

            println!("Dead Instructions indices: {:?}", self.dead_instructions);
        }

        self.cleanup(cfg); // Phase 3: Actual cleanup of instructions

        if cfg!(debug_assertions) {
            println!("======================== IR CODE CLEANED ========================");
            let mut sorted_blocks: Vec<&BasicBlock> = cfg.blocks.values().collect();
            sorted_blocks.sort_by_key(|b| b.id);
            for block in sorted_blocks {
                print!("{}", block);
            }
        }
    }

    fn cleanup(&mut self, cfg: &mut ControlFlowGraph) {
        // Cleanup all the dead instructions
        while let Some(dead_instr) = self.dead_instructions.pop() {
            match dead_instr {
                UseLocation::Instruction(block_id, index) => {
                    self.delete_instruction(cfg, block_id, index)
                }
                UseLocation::Phi(block, index) => {
                    self.delete_phi(cfg.blocks.get_mut(&block).unwrap(), index)
                }
            }
        }

        // Cleanup all the nops
        cfg.remove_useless_irs();
    }

    fn delete_instruction(&mut self, cfg: &mut ControlFlowGraph, block_id: BlockId, index: usize) {
        let block = cfg.blocks.get_mut(&block_id).unwrap();
        match &block.irs[index] {
            Ir51::Move { src, .. } => self.use_count_subtract(src),
            Ir51::Load { src_ptr, .. } => self.use_count_subtract(src_ptr),
            Ir51::BranchIf { cond, .. } => self.use_count_subtract(cond),

            Ir51::BinOp { lhs, rhs, .. } => {
                self.use_count_subtract(lhs);
                self.use_count_subtract(rhs);
            }

            Ir51::Store {
                dest_ptr, value, ..
            } => {
                self.use_count_subtract(dest_ptr);
                self.use_count_subtract(value);
            }

            Ir51::Nop | Ir51::Jump(_) | Ir51::Call(_) | Ir51::Return => {}
        }

        block.irs[index] = Ir51::Nop;
    }

    fn use_count_subtract(&mut self, operand: &Operand) {
        match operand {
            Operand::SSAVar { addr, version } => {
                let key = (*addr, *version);

                self.variable_uses.entry(key).and_modify(|counter| {
                    *counter -= 1;
                    if let Some(def_loc) = self.variable_defs.get(&key)
                        && *counter == 0
                        && !is_iram_addr(*addr)
                    {
                        self.dead_instructions.push(*def_loc);
                    }
                });
            }

            Operand::TempSSAVar {
                index: tmp_index,
                version,
            } => {
                let key = (*tmp_index, *version);

                self.temporary_uses.entry(key).and_modify(|counter| {
                    *counter -= 1;
                    if let Some(def_loc) = self.temporary_defs.get(&key)
                        && *counter == 0
                    {
                        self.dead_instructions.push(*def_loc);
                    }
                });
            }

            Operand::Constant(_) => {}
            Operand::Register(_) | Operand::Temp(_) | Operand::Var { .. } | Operand::Local(_) => {
                unreachable!()
            }
        }
    }

    fn delete_phi(&mut self, block: &mut BasicBlock, index: usize) {
        let phi = &mut block.phis[index];

        let Operand::SSAVar { addr, .. } = phi.operand else {
            unreachable!();
        };

        for incoming in phi.incoming.values() {
            if matches!(incoming, Operand::Constant(_)) {
                continue;
            }

            let operand = Operand::SSAVar {
                addr,
                version: incoming.extract_ssa_version(),
            };

            self.use_count_subtract(&operand);
        }

        // TODO: Change this to non valid operand
        phi.operand = Operand::Constant(0);
    }

    // ========================= Phase 2 =========================
    fn scan_irs(&mut self, cfg: &ControlFlowGraph) {
        for block in cfg.blocks.values() {
            for (i, phi) in block.phis.iter().enumerate() {
                self.scan_phi(phi, i, block.id);
            }

            for (i, ir) in block.irs.iter().enumerate() {
                self.scan_ir(ir, i, block.id);
            }
        }
    }

    fn scan_phi(&mut self, phi: &Phi, index: usize, blockid: BlockId) {
        self.check_usage(phi.operand, UseLocation::Phi(blockid, index));
    }

    fn scan_ir(&mut self, ir: &Ir51, index: usize, blockid: BlockId) {
        match ir {
            Ir51::Move { dest, .. } => {
                self.check_usage(*dest, UseLocation::Instruction(blockid, index));
            }
            Ir51::Load { dest, .. } => {
                self.check_usage(*dest, UseLocation::Instruction(blockid, index));
            }
            Ir51::BranchIf { cond, .. } => {
                self.check_usage(*cond, UseLocation::Instruction(blockid, index));
            }
            Ir51::BinOp { dest, .. } => {
                self.check_usage(*dest, UseLocation::Instruction(blockid, index));
            }

            Ir51::Store { .. } => {}
            Ir51::Nop | Ir51::Jump(_) | Ir51::Call(_) | Ir51::Return => {}
        }
    }

    fn check_usage(&mut self, dest: Operand, loc: UseLocation) {
        match dest {
            Operand::SSAVar { addr, version } => {
                if !is_iram_addr(addr) && !self.variable_uses.contains_key(&(addr, version)) {
                    self.dead_instructions.push(loc);
                }
            }

            Operand::TempSSAVar { index, version } => {
                if !self.temporary_uses.contains_key(&(index, version)) {
                    self.dead_instructions.push(loc);
                }
            }

            Operand::Var { .. }
            | Operand::Constant(_)
            | Operand::Temp(_)
            | Operand::Register(_)
            | Operand::Local(_) => {
                unreachable!("{dest:?}")
            }
        };
    }

    // ========================= Phase 1 =========================
    fn use_and_def_count(&mut self, cfg: &ControlFlowGraph) {
        // TODO: Since scanning this way happens a lot (even in other passes) we probably
        // need to factor it out
        for block in cfg.blocks.values() {
            for (i, phi) in block.phis.iter().enumerate() {
                self.use_and_def_count_on_phi(phi, i, block.id);
            }

            for (i, ir) in block.irs.iter().enumerate() {
                self.use_and_def_count_on_ir(ir, i, block.id);
            }
        }
    }

    fn use_and_def_count_on_phi(&mut self, phi: &Phi, phi_index: usize, blockid: BlockId) {
        let Operand::SSAVar { addr, .. } = phi.operand else {
            unreachable!();
        };

        self.save_def(&phi.operand, UseLocation::Phi(blockid, phi_index));

        for incoming in phi.incoming.values() {
            if matches!(incoming, Operand::Constant(_)) {
                continue;
            }

            let operand = Operand::SSAVar {
                addr,
                version: incoming.extract_ssa_version(),
            };

            self.use_count_on_operand(&operand);
        }
    }

    fn use_and_def_count_on_ir(&mut self, ir: &Ir51, ir_index: usize, blockid: BlockId) {
        match ir {
            Ir51::Move { dest, src } => {
                self.save_def(dest, UseLocation::Instruction(blockid, ir_index));
                self.use_count_on_operand(src);
            }
            Ir51::BinOp { dest, lhs, rhs, .. } => {
                self.save_def(dest, UseLocation::Instruction(blockid, ir_index));
                self.use_count_on_operand(lhs);
                self.use_count_on_operand(rhs);
            }
            Ir51::Store {
                dest_ptr, value, ..
            } => {
                self.use_count_on_operand(dest_ptr);
                self.use_count_on_operand(value);
            }

            // TODO: We probably have to check the space, because if it isnt' iData there
            // could be side-effect, in that chase we don't wanna remove this ir
            Ir51::Load { dest, src_ptr, .. } => {
                self.save_def(dest, UseLocation::Instruction(blockid, ir_index));
                self.use_count_on_operand(src_ptr);
            }

            Ir51::BranchIf { cond, .. } => self.use_count_on_operand(cond),

            Ir51::Nop => {}
            Ir51::Jump(_) | Ir51::Call(_) | Ir51::Return => {}
        }
    }

    fn save_def(&mut self, operand: &Operand, loc: UseLocation) {
        match operand {
            Operand::SSAVar { addr, version } => {
                self.variable_defs.insert((*addr, *version), loc);
            }

            Operand::TempSSAVar { index, version } => {
                self.temporary_defs.insert((*index, *version), loc);
            }

            Operand::Constant(_) => {}
            Operand::Var { .. } | Operand::Register(_) | Operand::Temp(_) | Operand::Local(_) => {
                unreachable!()
            }
        }
    }

    fn use_count_on_operand(&mut self, operand: &Operand) {
        match operand {
            Operand::SSAVar { addr, version } => {
                self.variable_uses
                    .entry((*addr, *version))
                    .and_modify(|counter| *counter += 1)
                    .or_insert(1);
            }

            Operand::TempSSAVar { index, version } => {
                self.temporary_uses
                    .entry((*index, *version))
                    .and_modify(|counter| *counter += 1)
                    .or_insert(1);
            }

            Operand::Constant(_) => {}
            Operand::Register(_) | Operand::Temp(_) | Operand::Var { .. } | Operand::Local(_) => {
                unreachable!()
            }
        }
    }
}
