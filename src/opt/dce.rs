use crate::{
    common::{BlockId, Ir51, Operand, Phi},
    opt::{
        cfg_restorer::{BasicBlock, ControlFlowGraph},
        const_propagation::{SSAVar, TempSSAVar},
    },
};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
enum UseLocation {
    Instruction(usize),
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
        self.use_and_def_count(cfg).unwrap(); // Phase 1: Calculates how many times a variable/temp is used
        self.scan_irs(cfg); // Phase 2: Look at all the destinations of the ir instructions
        // and if they aren't used mark them as dead

        if cfg!(debug_assertions) {
            println!("======================== DEAD CODE ELIMINATION ========================");
            println!("Uses for SSA: {:?}", self.variable_uses);
            println!("Uses for TempSSA: {:?}", self.temporary_uses);
            println!("Defs for SSA: {:?}", self.variable_defs);
            println!("Defs for TempSSA: {:?}", self.temporary_defs);
            println!("Dead Instructions indeces: {:?}", self.dead_instructions);
        }

        self.cleanup(cfg); // Phase 3: Actual cleanup of instructions

        if cfg!(debug_assertions) {
            println!("======================== IR CODE CLEANED ========================");
            for (i, ir) in cfg.ir_code.iter().enumerate() {
                if matches!(ir, Ir51::Nop) { continue; }
                println!("{i}: {ir:?}")
            }
        }
    }

    fn cleanup(&mut self, cfg: &mut ControlFlowGraph) {
        while let Some(dead_instr) = self.dead_instructions.pop() {
            match dead_instr {
                UseLocation::Instruction(index) => self.delete_instruction(cfg, index),
                UseLocation::Phi(block, index) => {
                    self.delete_phi(cfg.blocks.get_mut(&block).unwrap(), index)
                }
            }
        }
    }

    fn delete_instruction(&mut self, cfg: &mut ControlFlowGraph, index: usize) {
        match &cfg.ir_code[index] {
            Ir51::Move { src, .. } => self.use_count_subtract(src),
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

            Ir51::Load { src_ptr, .. } => self.use_count_subtract(src_ptr),

            Ir51::BranchIf { cond, .. } => self.use_count_subtract(cond),

            Ir51::Nop | Ir51::Jump(_) | Ir51::Call(_) | Ir51::Return => {}
            Ir51::InvalidIr => unreachable!("Invalid IR Touched"),
        }

        cfg.ir_code[index] = Ir51::Nop;
    }

    fn use_count_subtract(&mut self, operand: &Operand) {
        match operand {
            Operand::SSAVar { addr, version } => {
                let key = (*addr, *version);

                self.variable_uses.entry(key).and_modify(|counter| {
                    *counter -= 1;
                    if let Some(def_loc) = self.variable_defs.get(&key)
                        && *counter == 0
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

            Operand::Costant(_) => {}
            Operand::Register(_) | Operand::Temporary(_) | Operand::Address(_, _) => unreachable!(),
        }
    }

    fn delete_phi(&mut self, block: &mut BasicBlock, index: usize) {
        let phi = &mut block.phis[index];

        let Operand::SSAVar { addr, .. } = phi.operand else {
            unreachable!();
        };

        for incoming in phi.incoming.values() {
            if matches!(incoming, Operand::Costant(_)) {
                continue;
            }

            let operand = Operand::SSAVar {
                addr,
                version: incoming.extract_ssa_version(),
            };

            self.use_count_subtract(&operand);
        }

        // TODO: Change this to non valid operand
        phi.operand = Operand::Costant(0);
    }

    // ========================= Phase 2 =========================
    fn scan_irs(&mut self, cfg: &ControlFlowGraph) {
        for block in cfg.blocks.values() {
            block
                .phis
                .iter()
                .enumerate()
                .for_each(|(i, phi)| self.scan_phi(phi, i, block.id));

            block
                .ir_range
                .clone()
                .for_each(|i| self.scan_ir(&cfg.ir_code[i], i));
        }
    }

    fn scan_phi(&mut self, phi: &Phi, index: usize, blockid: BlockId) {
        self.check_usage(phi.operand, UseLocation::Phi(blockid, index));
    }

    fn scan_ir(&mut self, ir: &Ir51, index: usize) {
        match ir {
            Ir51::Move { dest, .. } => self.check_usage(*dest, UseLocation::Instruction(index)),
            Ir51::Load { dest, .. } => self.check_usage(*dest, UseLocation::Instruction(index)),
            Ir51::BranchIf { cond, .. } => self.check_usage(*cond, UseLocation::Instruction(index)),
            Ir51::Store { .. } => {}
            Ir51::BinOp { dest, .. } => {
                self.check_usage(*dest, UseLocation::Instruction(index));
            }

            Ir51::Nop | Ir51::Jump(_) | Ir51::Call(_) | Ir51::Return => {}
            Ir51::InvalidIr => unreachable!("Invalid IR Touched"),
        }
    }

    fn check_usage(&mut self, dest: Operand, loc: UseLocation) {
        match dest {
            Operand::SSAVar { addr, version } => {
                if !self.variable_uses.contains_key(&(addr, version)) {
                    self.dead_instructions.push(loc);
                }
            }

            Operand::TempSSAVar { index, version } => {
                if !self.temporary_uses.contains_key(&(index, version)) {
                    self.dead_instructions.push(loc);
                }
            }

            Operand::Address(_, _)
            | Operand::Costant(_)
            | Operand::Temporary(_)
            | Operand::Register(_) => {
                unreachable!("{dest:?}")
            }
        };
    }

    // ========================= Phase 1 =========================
    fn use_and_def_count(&mut self, cfg: &ControlFlowGraph) -> Option<()> {
        // TODO: Since scanning this way happens a lot (even in other passes) we probably
        // need to factor it out
        for block in cfg.blocks.values() {
            block
                .phis
                .iter()
                .enumerate()
                .for_each(|(i, phi)| self.use_and_def_count_on_phi(phi, i, block.id));

            let ir_range = block.ir_range.clone();
            ir_range.for_each(|i| self.use_and_def_count_on_ir(&cfg.ir_code[i], i));
        }

        Some(())
    }

    fn use_and_def_count_on_phi(&mut self, phi: &Phi, phi_index: usize, blockid: BlockId) {
        let Operand::SSAVar { addr, .. } = phi.operand else {
            unreachable!();
        };

        self.save_def(&phi.operand, UseLocation::Phi(blockid, phi_index));

        for incoming in phi.incoming.values() {
            if matches!(incoming, Operand::Costant(_)) {
                continue;
            }

            let operand = Operand::SSAVar {
                addr,
                version: incoming.extract_ssa_version(),
            };

            self.use_count_on_operand(&operand);
        }
    }

    fn use_and_def_count_on_ir(&mut self, ir: &Ir51, ir_index: usize) {
        match ir {
            Ir51::Move { dest, src } => {
                self.save_def(dest, UseLocation::Instruction(ir_index));
                self.use_count_on_operand(src);
            }
            Ir51::BinOp { dest, lhs, rhs, .. } => {
                self.save_def(dest, UseLocation::Instruction(ir_index));
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
                self.save_def(dest, UseLocation::Instruction(ir_index));
                self.use_count_on_operand(src_ptr);
            }

            Ir51::BranchIf { cond, .. } => self.use_count_on_operand(cond),

            Ir51::Nop => {}
            Ir51::Jump(_) | Ir51::Call(_) | Ir51::Return => {}
            Ir51::InvalidIr => unreachable!("Invalid IR Touched"),
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

            Operand::Costant(_) => {}
            Operand::Address(_, _) | Operand::Register(_) | Operand::Temporary(_) => unreachable!(),
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

            Operand::Costant(_) => {}
            Operand::Register(_) | Operand::Temporary(_) | Operand::Address(_, _) => unreachable!(),
        }
    }
}
