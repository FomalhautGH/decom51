use std::collections::HashMap;

use crate::{
    common::{Ir51, Operand},
    opt::{
        cfg_restorer::{BasicBlock, ControlFlowGraph},
        vwa::TrackedValue,
    },
};

type UseCounter = HashMap<TrackedValue, usize>;

#[derive(Default)]
pub struct ExprAggregation {
    defs: HashMap<TrackedValue, TrackedValue>,
}

// Phase that aggregates the expressions, we aggregate temporary veriables always
// and normal one if they are used just once
impl ExprAggregation {
    pub fn execute(&mut self, cfg: &mut ControlFlowGraph) {
        for block in cfg.blocks.values_mut() {}
    }

    fn aggregate_on(&mut self, block: &mut BasicBlock) {
        let mut use_count: UseCounter = HashMap::new();
        for ir in block.irs.iter() {
            Self::count_usages(ir, &mut use_count);
        }

        // TODO: implement aggregation logic
    }

    fn count_usages(ir: &Ir51, use_count: &mut UseCounter) {
        let mut save_use = |op: &Operand| {
            use_count
                .entry(TrackedValue::from_operand(op))
                .and_modify(|counter| *counter += 1)
                .or_insert(1);
        };

        match ir {
            Ir51::Move { dest, src } => {
                save_use(src);
            }

            Ir51::BinOp { dest, lhs, rhs, .. } => {
                save_use(lhs);
                save_use(rhs);
            }

            Ir51::Store {
                dest_ptr, value, ..
            } => {
                save_use(dest_ptr);
                save_use(value);
            }

            Ir51::Load { dest, src_ptr, .. } => {
                save_use(src_ptr);
            }

            Ir51::BranchIf { cond, .. } => save_use(cond),
            Ir51::Nop | Ir51::Jump(_) | Ir51::Call(_) | Ir51::Return => {}
        }
    }
}
