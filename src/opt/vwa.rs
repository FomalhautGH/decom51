use std::collections::{HashMap, HashSet};

use crate::{
    common::{Addr, AddrSpace, BinOp, Constant, Ir51, Operand, TempIndex},
    lir::{Register, Size},
    opt::cfg::{BasicBlock, ControlFlowGraph},
};

#[derive(Debug, Clone)]
struct FusedOperation {
    start_idx: usize,
    end_idx: usize,
    op: BinOp,
    dest: Operand,
    lhs: Operand,
    rhs: Operand,
}

#[derive(Debug, Clone)]
struct FusedInitialization {
    start_idx: usize,
    end_idx: usize,
    dest: Operand,
    src: Operand,
}

#[derive(Debug, Clone, Copy)]
struct BinOpUsage {
    op: BinOp,
    dest: Operand,
    rhs: Operand,
    index: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
// TODO: We probably need to have a size in classic as well
pub enum TrackedValue {
    Classic(Addr),
    Temporary(TempIndex),
    Constant(Constant),
}

impl TrackedValue {
    pub fn from_operand(op: &Operand) -> Option<Self> {
        match op {
            Operand::Var(addr, _, _) => Some(Self::Classic(*addr)),
            Operand::Local(id) => Some(Self::Classic(*id as Addr)),
            Operand::Temp(id) => Some(Self::Temporary(*id)),
            Operand::Constant(value) => Some(Self::Constant(*value)),
            Operand::Register(_) | Operand::TempSSAVar { .. } | Operand::SSAVar { .. } => None,
        }
    }
}

// TODO: this phase probably needs to be more general
#[derive(Default)]
pub struct VariableWidthAggregation {
    fused_addresses: HashSet<Addr>,
}

impl VariableWidthAggregation {
    pub fn execute(&mut self, cfg: &mut ControlFlowGraph) {
        self.aggregate_variables(cfg);

        if cfg!(debug_assertions) {
            println!("======================== VAR WIDTH AGGREGATION ========================");
            let mut sorted_blocks: Vec<&BasicBlock> = cfg.blocks.values().collect();
            sorted_blocks.sort_by_key(|b| b.id);
            for block in sorted_blocks {
                print!("{}", block);
            }
        }
    }

    fn aggregate_variables(&mut self, cfg: &mut ControlFlowGraph) {
        let mut modified = true;
        while modified {
            modified = false;

            for block in cfg.blocks.values_mut() {
                modified |= self.aggregate_intra_block(block);
                modified |= self.aggregate_inter_block(block);
            }
        }
    }

    fn aggregate_inter_block(&self, block: &mut BasicBlock) -> bool {
        if let Some(fused_op) = self.search_nbit_inizialization(&block.irs) {
            let with = vec![Ir51::Move {
                dest: fused_op.dest,
                src: fused_op.src,
            }];

            // TODO: we shouldn't use splice if there are other instruction in between start idx and
            // end idx but its fine for now
            block
                .irs
                .splice(fused_op.start_idx..=fused_op.end_idx, with);
            true
        } else {
            false
        }
    }

    fn search_nbit_inizialization(&self, irs: &[Ir51]) -> Option<FusedInitialization> {
        for (i, ir) in irs.iter().enumerate() {
            if let Ir51::Move {
                dest: Operand::Var(low_addr, addr_space, size),
                src: Operand::Constant(low_value),
            } = ir
                && self.fused_addresses.contains(low_addr)
                && let Some((high_value, end_idx)) =
                    Self::find_next_inizialization(irs, *low_addr, i)
            {
                let start_idx = i;
                let dest = Operand::Var(*low_addr, *addr_space, *size);
                let src = Operand::Constant((high_value << 8) | low_value);

                return Some(FusedInitialization {
                    start_idx,
                    end_idx,
                    dest,
                    src,
                });
            }
        }

        None
    }

    fn find_next_inizialization(
        irs: &[Ir51],
        low_addr: Addr,
        idx: usize,
    ) -> Option<(Constant, usize)> {
        let target_addr = low_addr + 1;
        for (i, ir) in irs.iter().enumerate().skip(idx + 1) {
            if let Ir51::Move {
                dest: Operand::Var(high_addr, _, _),
                src: Operand::Constant(high_value),
            } = ir
                && *high_addr == target_addr
            {
                return Some((*high_value, i));
            }
        }

        None
    }

    fn aggregate_intra_block(&mut self, block: &mut BasicBlock) -> bool {
        if let Some(fused_op) = Self::search_nbit_operation(&block.irs) {
            let with = vec![Ir51::BinOp {
                op: fused_op.op,
                dest: fused_op.dest,
                lhs: fused_op.lhs,
                rhs: fused_op.rhs,
            }];

            self.expand_fused_adderesses(&fused_op);

            // TODO: we shouldn't use splice if there are other instruction in between start idx and
            // end idx but its fine for now
            block
                .irs
                .splice(fused_op.start_idx..=fused_op.end_idx, with);
            true
        } else {
            false
        }
    }

    fn expand_fused_adderesses(&mut self, fused_op: &FusedOperation) {
        if let Some(addr) = fused_op.dest.to_var_addr() {
            self.fused_addresses.insert(addr);
        }
        if let Some(addr) = fused_op.lhs.to_var_addr() {
            self.fused_addresses.insert(addr);
        }
        if let Some(addr) = fused_op.rhs.to_var_addr() {
            self.fused_addresses.insert(addr);
        }
    }

    fn search_nbit_operation(irs: &[Ir51]) -> Option<FusedOperation> {
        for (idx, ir) in irs.iter().enumerate() {
            if let Ir51::BinOp { op, .. } = ir {
                let fuse_result = match op {
                    BinOp::Addc => Self::try_fusing_addc(irs, idx),
                    BinOp::Or => Self::try_fusing_or(irs, idx),

                    // TODO: if there are more than one subb later even if
                    // that subb is adjacent we still analyze here
                    BinOp::Subb => Self::try_fusing_subb(irs, idx),

                    BinOp::And => None,
                    BinOp::Xor => None,
                    _ => None,
                };

                if fuse_result.is_some() {
                    return fuse_result;
                }
            }
        }

        None
    }

    fn try_fusing_or(irs: &[Ir51], or_idx: usize) -> Option<FusedOperation> {
        let Ir51::BinOp {
            op: BinOp::Or,
            dest,
            lhs,
            rhs: lhs_high,
        } = irs[or_idx]
        else {
            unreachable!()
        };

        let lhs_high_addr = lhs_high.to_var_addr()?;
        let (lhs_low, start_idx) = Self::find_last_initialization(&lhs, irs, or_idx)?;
        let binop_usage = Self::find_next_binop_usage(&dest, irs, or_idx)?;
        let lhs_low_addr = lhs_low.to_var_addr()?;

        (lhs_low_addr + 1 == lhs_high_addr).then_some(bool::default())?;

        let op = binop_usage.op;
        let lhs = Operand::Var(lhs_low_addr, AddrSpace::iData, 2);
        let rhs = binop_usage.rhs;
        let dest = binop_usage.dest;
        let end_idx = binop_usage.index;

        Some(FusedOperation {
            start_idx,
            end_idx,
            op,
            dest,
            lhs,
            rhs,
        })
    }

    fn find_next_binop_usage(
        target_op: &Operand,
        irs: &[Ir51],
        from_idx: usize,
    ) -> Option<BinOpUsage> {
        for (idx, ir) in irs.iter().enumerate().skip(from_idx + 1) {
            match ir {
                Ir51::BinOp { op, dest, lhs, rhs } if lhs == target_op => {
                    return Some(BinOpUsage {
                        op: *op,
                        dest: *dest,
                        rhs: *rhs,
                        index: idx,
                    });
                }
                _ => {}
            }
        }

        None
    }

    fn try_fusing_subb(irs: &[Ir51], subb_idx: usize) -> Option<FusedOperation> {
        let Ir51::BinOp {
            op: BinOp::Subb,
            dest,
            lhs,
            rhs,
        } = irs[subb_idx]
        else {
            unreachable!()
        };

        let rhs_low_addr = rhs.to_var_addr()?;
        let (lhs_low, start_idx) = Self::find_last_initialization(&lhs, irs, subb_idx)?;
        let (dest_low, _) = Self::find_next_usage(&dest, irs, subb_idx)?;
        let (lhs_low_addr, dest_low_addr) = (lhs_low.to_var_addr()?, dest_low.to_var_addr()?);

        let next_subb_idx = Self::find_adjacent_subb(irs, subb_idx)?;

        let _ = rhs.to_var_addr()?;
        let (lhs_high, _) = Self::find_last_initialization(&lhs, irs, next_subb_idx)?;
        let (dest_high, end_idx) = Self::find_next_usage(&dest, irs, next_subb_idx)?;
        let (_, _) = (lhs_high.to_var_addr()?, dest_high.to_var_addr()?);

        let op = BinOp::Sub;
        let lhs = Operand::Var(lhs_low_addr, AddrSpace::iData, 2);
        let rhs = Operand::Var(rhs_low_addr, AddrSpace::iData, 2);
        let dest = Operand::Var(dest_low_addr, AddrSpace::iData, 2);

        Some(FusedOperation {
            start_idx,
            end_idx,
            op,
            dest,
            lhs,
            rhs,
        })
    }

    fn find_adjacent_subb(irs: &[Ir51], subb_idx: usize) -> Option<usize> {
        for (idx, ir) in irs.iter().enumerate().skip(subb_idx + 1) {
            match ir {
                // If carry is cleared before the next SUBB, they are not part
                // of the same 16-bit subtraction.
                Ir51::Move {
                    dest: Operand::Register(Register::PSW),
                    src: Operand::Constant(0),
                } => return None,

                Ir51::BinOp {
                    op: BinOp::Subb, ..
                } => return Some(idx),

                _ => {}
            }
        }

        None
    }

    fn try_fusing_addc(irs: &[Ir51], addc_idx: usize) -> Option<FusedOperation> {
        let Ir51::BinOp {
            op: BinOp::Addc,
            dest,
            lhs,
            rhs,
        } = irs[addc_idx]
        else {
            unreachable!()
        };

        let _ = rhs.to_var_addr()?;
        let (lhs_high, _) = Self::find_last_initialization(&lhs, irs, addc_idx)?;
        let (dest_high, end_idx) = Self::find_next_usage(&dest, irs, addc_idx)?;
        let (_, _) = (lhs_high.to_var_addr()?, dest_high.to_var_addr()?);

        let add_idx = Self::find_last_add(irs, addc_idx)?;
        let Ir51::BinOp {
            op: BinOp::Add,
            dest: add_dest,
            lhs: add_lhs,
            rhs: add_rhs,
        } = irs[add_idx]
        else {
            unreachable!()
        };

        let rhs_low_addr = add_rhs.to_var_addr()?;
        let (lhs_low, start_idx) = Self::find_last_initialization(&add_lhs, irs, add_idx)?;
        let (dest_low, _) = Self::find_next_usage(&add_dest, irs, add_idx)?;
        let (lhs_low_addr, dest_low_addr) = (lhs_low.to_var_addr()?, dest_low.to_var_addr()?);

        let op = BinOp::Add;
        let lhs = Operand::Var(lhs_low_addr, AddrSpace::iData, 2);
        let rhs = Operand::Var(rhs_low_addr, AddrSpace::iData, 2);
        let dest = Operand::Var(dest_low_addr, AddrSpace::iData, 2);

        Some(FusedOperation {
            start_idx,
            end_idx,
            op,
            dest,
            lhs,
            rhs,
        })
    }

    fn find_last_add(irs: &[Ir51], from_idx: usize) -> Option<usize> {
        for (idx, ir) in irs.iter().enumerate().take(from_idx).rev() {
            if let Ir51::BinOp { op: BinOp::Add, .. } = ir {
                return Some(idx);
            }
        }

        None
    }

    // Finds the next Move instruction reading `target_op` as its source, returning `(dest, index)`.
    fn find_next_usage(
        target_op: &Operand,
        irs: &[Ir51],
        from_idx: usize,
    ) -> Option<(Operand, usize)> {
        for (idx, ir) in irs.iter().enumerate().skip(from_idx) {
            match ir {
                Ir51::Move { dest, src } if src == target_op => return Some((*dest, idx)),
                _ => {}
            }
        }

        None
    }

    // Finds the last Move instruction writing to `target_op` as its destination, returning `(src, index)`.
    fn find_last_initialization(
        target_op: &Operand,
        irs: &[Ir51],
        from_idx: usize,
    ) -> Option<(Operand, usize)> {
        for (idx, ir) in irs.iter().enumerate().take(from_idx).rev() {
            match ir {
                Ir51::Move { dest, src } if dest == target_op => return Some((*src, idx)),
                _ => {}
            }
        }

        None
    }
}
