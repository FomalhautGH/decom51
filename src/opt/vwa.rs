use std::collections::HashSet;

use crate::{
    common::{Addr, AddrSpace, BinOp, Constant, Ir51, Operand, TempIndex},
    lir::Register,
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

// TODO: Generalize multi-byte aggregation beyond 16-bit patterns.
#[derive(Default)]
pub struct VariableWidthAggregation {
    fused_addresses: HashSet<Addr>,
}

impl VariableWidthAggregation {
    // =========================================================================
    // Core Phase Execution
    // =========================================================================

    pub fn execute(&mut self, cfg: &mut ControlFlowGraph) {
        self.aggregate_variables(cfg);
        self.aggregate_operations(cfg);

        if cfg!(debug_assertions) {
            println!("======================== VAR WIDTH AGGREGATION ========================");
            let mut sorted_blocks: Vec<&BasicBlock> = cfg.blocks.values().collect();
            sorted_blocks.sort_by_key(|b| b.id);
            for block in sorted_blocks {
                print!("{}", block);
            }
        }
    }

    fn aggregate_operations(&mut self, cfg: &mut ControlFlowGraph) {
        let mut modified = true;
        while modified {
            modified = false;

            for block in cfg.blocks.values_mut() {
                modified |= self.aggregate_ops(block);
            }
        }
    }

    fn aggregate_ops(&mut self, block: &mut BasicBlock) -> bool {
        if let Some(fused_op) = Self::search_aggr_operation(&block.irs) {
            let with = vec![Ir51::BinOp {
                op: fused_op.op,
                dest: fused_op.dest,
                lhs: fused_op.lhs,
                rhs: fused_op.rhs,
            }];

            block
                .irs
                .splice(fused_op.start_idx..=fused_op.end_idx, with);
            true
        } else {
            false
        }
    }

    fn search_aggr_operation(irs: &[Ir51]) -> Option<FusedOperation> {
        let window_size = 3;

        for (start_idx, ir_window) in irs.windows(window_size).enumerate() {
            let (first, second, third) = (ir_window[0], ir_window[1], ir_window[2]);

            // Finds instruction sequences like this:
            // 1: A = #0xF6
            // 2: A = A + iData[0x08]
            // 3: t0 = PSW != #0x00

            // TODO: dont hardcode register addresses
            if let Ir51::Move {
                dest: Operand::Var { addr: 0xe0, .. },
                src: Operand::Constant(val),
            } = first
                && let Ir51::BinOp {
                    op: BinOp::Add,
                    dest: Operand::Var { addr: 0xe0, .. },
                    lhs: Operand::Var { addr: 0xe0, .. },
                    rhs,
                } = second
                && let Ir51::BinOp {
                    op: BinOp::Ne,
                    dest: Operand::Temp(0),
                    lhs: Operand::Var { addr: 0xd0, .. },
                    rhs: Operand::Constant(0),
                } = third
            {
                return Some(FusedOperation {
                    start_idx,
                    end_idx: start_idx + window_size - 1,
                    op: BinOp::Ge,
                    dest: Operand::Temp(0),
                    lhs: rhs,
                    rhs: Operand::Constant(0u8.wrapping_sub(val as u8).into()),
                });
            };
        }

        None
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

    // =========================================================================
    // Intra-Block Arithmetic & Logical Fusion
    // =========================================================================

    fn aggregate_intra_block(&mut self, block: &mut BasicBlock) -> bool {
        if let Some(fused_op) = Self::search_nbit_operation(&block.irs) {
            let with = vec![Ir51::BinOp {
                op: fused_op.op,
                dest: fused_op.dest,
                lhs: fused_op.lhs,
                rhs: fused_op.rhs,
            }];

            self.expand_fused_addresses(&fused_op);

            // TODO: Avoid range splice if unrelated instructions are interleaved.
            block
                .irs
                .splice(fused_op.start_idx..=fused_op.end_idx, with);
            true
        } else {
            false
        }
    }

    fn search_nbit_operation(irs: &[Ir51]) -> Option<FusedOperation> {
        for (idx, ir) in irs.iter().enumerate() {
            if let Ir51::BinOp { op, .. } = ir {
                let fuse_result = match op {
                    // TODO: Handle multi-byte SUBB chains beyond 2-byte pairs.
                    BinOp::Subb => Self::try_fusing_subb(irs, idx),
                    BinOp::Addc => Self::try_fusing_addc(irs, idx),

                    BinOp::And => Self::try_fusing_bitwise(irs, idx, BinOp::And),
                    BinOp::Xor => Self::try_fusing_bitwise(irs, idx, BinOp::Xor),
                    BinOp::Or => Self::try_fusing_bitwise(irs, idx, BinOp::Or)
                        .or_else(|| Self::try_fusing_or(irs, idx)),
                    _ => None,
                };

                if fuse_result.is_some() {
                    return fuse_result;
                }
            }
        }

        None
    }

    fn try_fusing_bitwise(irs: &[Ir51], op_idx: usize, target_op: BinOp) -> Option<FusedOperation> {
        let Ir51::BinOp { op, dest, lhs, rhs } = irs[op_idx] else {
            unreachable!()
        };

        if op != target_op {
            return None;
        }

        let rhs_low_addr = rhs.to_var_addr()?;
        let (lhs_low, start_idx) = Self::find_last_initialization(&lhs, irs, op_idx)?;
        let (dest_low, _) = Self::find_next_usage(&dest, irs, op_idx)?;
        let (lhs_low_addr, dest_low_addr) = (lhs_low.to_var_addr()?, dest_low.to_var_addr()?);

        let next_op_idx = Self::find_adjacent_bitwise(irs, op_idx, target_op)?;

        let Ir51::BinOp {
            dest: high_dest,
            lhs: high_lhs,
            rhs: high_rhs,
            ..
        } = irs[next_op_idx]
        else {
            return None;
        };

        let rhs_high_addr = high_rhs.to_var_addr()?;
        let (lhs_high, _) = Self::find_last_initialization(&high_lhs, irs, next_op_idx)?;
        let (dest_high, end_idx) = Self::find_next_usage(&high_dest, irs, next_op_idx)?;
        let (lhs_high_addr, dest_high_addr) = (lhs_high.to_var_addr()?, dest_high.to_var_addr()?);

        if lhs_low_addr + 1 != lhs_high_addr
            || rhs_low_addr + 1 != rhs_high_addr
            || dest_low_addr + 1 != dest_high_addr
        {
            return None;
        }

        let op = target_op;
        let lhs = Operand::Var {
            addr: lhs_low_addr,
            space: AddrSpace::iData,
            size: 2,
        };
        let rhs = Operand::Var {
            addr: rhs_low_addr,
            space: AddrSpace::iData,
            size: 2,
        };
        let dest = Operand::Var {
            addr: dest_low_addr,
            space: AddrSpace::iData,
            size: 2,
        };

        Some(FusedOperation {
            start_idx,
            end_idx,
            op,
            dest,
            lhs,
            rhs,
        })
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
        let lhs = Operand::Var {
            addr: lhs_low_addr,
            space: AddrSpace::iData,
            size: 2,
        };
        let rhs = Operand::Var {
            addr: rhs_low_addr,
            space: AddrSpace::iData,
            size: 2,
        };
        let dest = Operand::Var {
            addr: dest_low_addr,
            space: AddrSpace::iData,
            size: 2,
        };

        Some(FusedOperation {
            start_idx,
            end_idx,
            op,
            dest,
            lhs,
            rhs,
        })
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
        let lhs = Operand::Var {
            addr: lhs_low_addr,
            space: AddrSpace::iData,
            size: 2,
        };
        let rhs = Operand::Var {
            addr: rhs_low_addr,
            space: AddrSpace::iData,
            size: 2,
        };
        let dest = Operand::Var {
            addr: dest_low_addr,
            space: AddrSpace::iData,
            size: 2,
        };

        Some(FusedOperation {
            start_idx,
            end_idx,
            op,
            dest,
            lhs,
            rhs,
        })
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
        let lhs = Operand::Var {
            addr: lhs_low_addr,
            space: AddrSpace::iData,
            size: 2,
        };
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

    fn expand_fused_addresses(&mut self, fused_op: &FusedOperation) {
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

    // =========================================================================
    // Inter-Block / Constant Initialization Fusion
    // =========================================================================

    fn aggregate_inter_block(&self, block: &mut BasicBlock) -> bool {
        if let Some(fused_op) = self.search_nbit_inizialization(&block.irs) {
            let with = vec![Ir51::Move {
                dest: fused_op.dest,
                src: fused_op.src,
            }];

            // TODO: Avoid range splice if unrelated instructions are interleaved.
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
                dest:
                    Operand::Var {
                        addr: low_addr,
                        space: addr_space,
                        size,
                    },
                src: Operand::Constant(low_value),
            } = ir
                && self.fused_addresses.contains(low_addr)
                && let Some((high_value, end_idx)) =
                    Self::find_next_inizialization(irs, *low_addr, i)
            {
                let start_idx = i;
                let dest = Operand::Var {
                    addr: *low_addr,
                    space: *addr_space,
                    size: *size,
                };
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
                dest: Operand::Var {
                    addr: high_addr, ..
                },
                src: Operand::Constant(high_value),
            } = ir
                && *high_addr == target_addr
            {
                return Some((*high_value, i));
            }
        }

        None
    }

    // =========================================================================
    // Instruction Search Helpers
    // =========================================================================

    fn find_last_add(irs: &[Ir51], from_idx: usize) -> Option<usize> {
        for (idx, ir) in irs.iter().enumerate().take(from_idx).rev() {
            if let Ir51::BinOp { op: BinOp::Add, .. } = ir {
                return Some(idx);
            }
        }

        None
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

    fn find_adjacent_bitwise(irs: &[Ir51], from_idx: usize, target_op: BinOp) -> Option<usize> {
        for (idx, ir) in irs.iter().enumerate().skip(from_idx + 1) {
            if let Ir51::BinOp { op, .. } = ir
                && *op == target_op
            {
                return Some(idx);
            }
        }

        None
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
