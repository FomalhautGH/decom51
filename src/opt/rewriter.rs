use crate::{
    common::{Constant, Ir51, Operand, Phi},
    opt::{
        cfg_restorer::{BasicBlock, ControlFlowGraph},
        const_propagation::{SSAState, TemporaryStates, VariableStates},
    },
};

pub struct Rewriter<'a> {
    known_values: &'a VariableStates,
    known_temps: &'a TemporaryStates,
}

impl<'a> Rewriter<'a> {
    pub fn new(known_values: &'a VariableStates, known_temps: &'a TemporaryStates) -> Self {
        Self {
            known_values,
            known_temps,
        }
    }

    pub fn rewrite(&self, cfg: &mut ControlFlowGraph) {
        for block in cfg.blocks.values_mut() {
            self.rewrite_block(block, &mut cfg.ir_code);
        }

        if cfg!(debug_assertions) {
            println!("======================== IR CODE AFTER REWRITE ========================");
            println!("Irs: ");
            for (i, ir) in cfg.ir_code.iter().enumerate() {
                println!("{i}: {ir:?}")
            }

            println!("Phis per block: ");
            for block in cfg.blocks.values() {
                println!(
                    "----------------------- Block number {} -----------------------",
                    block.id
                );

                for (i, phi) in block.phis.iter().enumerate() {
                    println!("{i}: {phi:?}")
                }

                println!(
                    "----------------------- End block number {} -----------------------",
                    block.id
                );
            }
        }
    }

    fn rewrite_block(&self, block: &mut BasicBlock, ir_code: &mut [Ir51]) -> Option<()> {
        block.phis.iter_mut().for_each(|phi| self.rewrite_phi(phi));

        // TODO: Maybe clean up these 3 operations i do often?
        let ir_range = block.ir_range.clone();
        let ir_code = ir_code.get_mut(ir_range)?;
        ir_code.iter_mut().for_each(|ir| self.rewrite_ir(ir));

        Some(())
    }

    fn rewrite_phi(&self, phi: &mut Phi) {
        for (_, incoming) in phi.incoming.iter_mut() {
            self.rewrite_operand(incoming)
        }

        // TODO: Also rewrite if all the incomings became constants with the same value
    }

    fn rewrite_ir(&self, ir: &mut Ir51) {
        match ir {
            Ir51::Move { src, .. } => self.rewrite_operand(src),
            Ir51::BinOp { dest, lhs, rhs, .. } => {
                self.rewrite_operand(lhs);
                self.rewrite_operand(rhs);
                if let Some(v) = self.get_constant(dest) {
                    *ir = Ir51::Move {
                        dest: *dest,
                        src: Operand::Costant(v),
                    };
                }
            }
            Ir51::Store {
                dest_ptr, value, ..
            } => {
                self.rewrite_operand(dest_ptr);
                self.rewrite_operand(value);
            }

            Ir51::Load { dest, src_ptr, .. } => {
                self.rewrite_operand(dest);
                self.rewrite_operand(src_ptr);
            }

            Ir51::BranchIf {
                cond,
                true_jmp,
                false_jmp,
            } => {
                if let Some(v) = self.get_constant(cond) {
                    match v {
                        0 => *ir = Ir51::Jump(*false_jmp),
                        _ => *ir = Ir51::Jump(*true_jmp),
                    }
                }
            }

            Ir51::Nop => {}
            Ir51::Jump(_) | Ir51::Call(_) | Ir51::Return => {}
            Ir51::InvalidIr => unreachable!("Invalid IR Touched"),
        }
    }

    fn rewrite_operand(&self, operand: &mut Operand) {
        match operand {
            Operand::SSAVar { addr, version } => {
                let state = self.known_values[&(*addr, *version)];
                if let SSAState::Constant(v) = state {
                    *operand = Operand::Costant(v);
                }
            }
            Operand::TempSSAVar { index, version } => {
                let state = self.known_temps[&(*index, *version)];
                if let SSAState::Constant(v) = state {
                    *operand = Operand::Costant(v);
                }
            }

            Operand::Costant(_) => {}
            Operand::Register(_) | Operand::Temporary(_) | Operand::Address(_, _) => unreachable!(),
        }
    }

    // TODO: unite these two functions somehow

    fn get_constant(&self, operand: &Operand) -> Option<Constant> {
        match operand {
            Operand::SSAVar { addr, version } => {
                let state = self.known_values[&(*addr, *version)];
                if let SSAState::Constant(v) = state {
                    Some(v)
                } else {
                    None
                }
            }
            Operand::TempSSAVar { index, version } => {
                let state = self.known_temps[&(*index, *version)];
                if let SSAState::Constant(v) = state {
                    Some(v)
                } else {
                    None
                }
            }

            Operand::Costant(v) => Some(*v),
            Operand::Register(_) | Operand::Temporary(_) | Operand::Address(_, _) => unreachable!(),
        }
    }
}
