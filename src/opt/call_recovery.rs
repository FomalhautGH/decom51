use crate::{
    lir::Register,
    mir::{Expr, Mir51},
    opt::cfg::{BasicBlock, ControlFlowGraph},
};

pub struct CallRecovery;

// Works only on Middle IRs
impl CallRecovery {
    pub fn execute(&self, cfg: &mut ControlFlowGraph) {
        for block in cfg.blocks.values_mut() {
            self.recover_calls(block);
        }
    }

    // TODO: Fix the bugs in this phase: remove the correct instructions,
    // better tracking of arguments
    fn recover_calls(&self, block: &mut BasicBlock) -> bool {
        let mut call_args = None;
        let mut params: Vec<Expr> = Vec::new();
        let mut remove_idx = Vec::new();

        for (i, ir) in block.mirs.iter_mut().enumerate() {
            match ir {
                Mir51::Assign { .. } if ir.is_sp_increment().is_some() => {
                    remove_idx.push(i);
                }

                Mir51::Assign { .. } if ir.is_sp_decrement().is_some() => {
                    remove_idx.push(i);
                }

                Mir51::Assign {
                    src: Expr::Deref(src_ptr),
                    ..
                } if matches!(**src_ptr, Expr::Register(Register::SP)) => {
                    remove_idx.push(i);
                }

                Mir51::Store {
                    dest_ptr: Expr::Register(Register::SP),
                    value,
                } => {
                    remove_idx.push(i);
                    params.push(value.clone());
                }

                Mir51::Call { args, .. } => {
                    call_args = Some(args);
                }

                Mir51::Store { .. }
                | Mir51::Assign { .. }
                | Mir51::If { .. }
                | Mir51::Goto(_)
                | Mir51::Nop
                | Mir51::Return => {}
            }
        }

        if let Some(args_ptr) = call_args {
            *args_ptr = params;

            let mut index = 0;
            block.mirs.retain(|_| {
                let keep = !remove_idx.contains(&index);
                index += 1;
                keep
            });

            true
        } else {
            false
        }
    }
}
