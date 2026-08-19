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

    // Phase 1: Multi-Byte Variable Aggregation (Arithmetic & Initializations)
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

    // --- Phase 1A: Intra-Block Arithmetic & Bitwise Fusion ---
    fn aggregate_intra_block(&mut self, block: &mut BasicBlock) -> bool {
        if let Some(fused_op) = Self::search_nbit_operation(&block.irs) {
            let replacement_irs = vec![Ir51::BinOp {
                op: fused_op.op,
                dest: fused_op.dest,
                lhs: fused_op.lhs,
                rhs: fused_op.rhs,
            }];

            self.expand_fused_addresses(&fused_op);

            // TODO: Avoid range splice if unrelated instructions are interleaved.
            block
                .irs
                .splice(fused_op.start_idx..=fused_op.end_idx, replacement_irs);
            true
        } else {
            false
        }
    }

    fn search_nbit_operation(irs: &[Ir51]) -> Option<FusedOperation> {
        for (ir_idx, ir) in irs.iter().enumerate() {
            if let Ir51::BinOp { op, .. } = ir {
                let fused_op = match op {
                    // TODO: Handle multi-byte SUBB chains beyond 2-byte pairs.
                    BinOp::Addc => Self::try_fusing_addc(irs, ir_idx),
                    BinOp::Subb => Self::try_fusing_subb(irs, ir_idx),

                    BinOp::And => Self::try_fusing_bitwise(irs, ir_idx, BinOp::And),
                    BinOp::Xor => Self::try_fusing_bitwise(irs, ir_idx, BinOp::Xor),
                    BinOp::Or => Self::try_fusing_bitwise(irs, ir_idx, BinOp::Or)
                        .or_else(|| Self::try_fusing_or(irs, ir_idx)),
                    _ => None,
                };

                if fused_op.is_some() {
                    return fused_op;
                }
            }
        }

        None
    }

    fn try_fusing_addc(irs: &[Ir51], addc_idx: usize) -> Option<FusedOperation> {
        let Ir51::BinOp {
            op: BinOp::Addc,
            dest: addc_dest,
            lhs: addc_lhs,
            rhs: addc_rhs,
        } = irs[addc_idx]
        else {
            unreachable!()
        };

        let _ = addc_rhs.to_var_addr()?;
        let (lhs_high_op, _) = Self::find_last_initialization(&addc_lhs, irs, addc_idx)?;
        let (dest_high_op, end_idx) = Self::find_next_usage(&addc_dest, irs, addc_idx)?;
        let (_, _) = (lhs_high_op.to_var_addr()?, dest_high_op.to_var_addr()?);

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
        let (lhs_low_op, start_idx) = Self::find_last_initialization(&add_lhs, irs, add_idx)?;
        let (dest_low_op, _) = Self::find_next_usage(&add_dest, irs, add_idx)?;
        let (lhs_low_addr, dest_low_addr) = (lhs_low_op.to_var_addr()?, dest_low_op.to_var_addr()?);

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
            dest: first_dest,
            lhs: first_lhs,
            rhs: first_rhs,
        } = irs[subb_idx]
        else {
            unreachable!()
        };

        let rhs_low_addr = first_rhs.to_var_addr()?;
        let (lhs_low_op, start_idx) = Self::find_last_initialization(&first_lhs, irs, subb_idx)?;
        let (dest_low_op, _) = Self::find_next_usage(&first_dest, irs, subb_idx)?;
        let (lhs_low_addr, dest_low_addr) = (lhs_low_op.to_var_addr()?, dest_low_op.to_var_addr()?);

        let next_subb_idx = Self::find_adjacent_subb(irs, subb_idx)?;

        let _ = first_rhs.to_var_addr()?;
        let (lhs_high_op, _) = Self::find_last_initialization(&first_lhs, irs, next_subb_idx)?;
        let (dest_high_op, end_idx) = Self::find_next_usage(&first_dest, irs, next_subb_idx)?;
        let (_, _) = (lhs_high_op.to_var_addr()?, dest_high_op.to_var_addr()?);

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

    fn try_fusing_bitwise(irs: &[Ir51], op_idx: usize, target_op: BinOp) -> Option<FusedOperation> {
        let Ir51::BinOp { op, dest, lhs, rhs } = irs[op_idx] else {
            unreachable!()
        };

        if op != target_op {
            return None;
        }

        let rhs_low_addr = rhs.to_var_addr()?;
        let (lhs_low_op, start_idx) = Self::find_last_initialization(&lhs, irs, op_idx)?;
        let (dest_low_op, _) = Self::find_next_usage(&dest, irs, op_idx)?;
        let (lhs_low_addr, dest_low_addr) = (lhs_low_op.to_var_addr()?, dest_low_op.to_var_addr()?);

        let next_op_idx = Self::find_adjacent_bitwise(irs, op_idx, target_op)?;

        let Ir51::BinOp {
            dest: dest_high,
            lhs: lhs_high,
            rhs: rhs_high,
            ..
        } = irs[next_op_idx]
        else {
            return None;
        };

        let rhs_high_addr = rhs_high.to_var_addr()?;
        let (lhs_high_op, _) = Self::find_last_initialization(&lhs_high, irs, next_op_idx)?;
        let (dest_high_op, end_idx) = Self::find_next_usage(&dest_high, irs, next_op_idx)?;
        let (lhs_high_addr, dest_high_addr) =
            (lhs_high_op.to_var_addr()?, dest_high_op.to_var_addr()?);

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

    fn try_fusing_or(irs: &[Ir51], or_idx: usize) -> Option<FusedOperation> {
        let Ir51::BinOp {
            op: BinOp::Or,
            dest: or_dest,
            lhs: or_lhs,
            rhs: lhs_high,
        } = irs[or_idx]
        else {
            unreachable!()
        };

        let lhs_high_addr = lhs_high.to_var_addr()?;
        let (lhs_low_op, start_idx) = Self::find_last_initialization(&or_lhs, irs, or_idx)?;
        let binop_usage = Self::find_next_binop_usage(&or_dest, irs, or_idx)?;
        let lhs_low_addr = lhs_low_op.to_var_addr()?;

        (lhs_low_addr + 1 == lhs_high_addr).then_some(bool::default())?;

        let op = binop_usage.op;
        let rhs = binop_usage.rhs;
        let dest = binop_usage.dest;
        let end_idx = binop_usage.index;
        let lhs = Operand::Var {
            addr: lhs_low_addr,
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

    // --- Phase 1B: Inter-Block Constant Initialization Fusion ---
    fn aggregate_inter_block(&self, block: &mut BasicBlock) -> bool {
        if let Some(fused_init) = self.search_nbit_initialization(&block.irs) {
            let replacement_irs = vec![Ir51::Move {
                dest: fused_init.dest,
                src: fused_init.src,
            }];

            // TODO: Avoid range splice if unrelated instructions are interleaved.
            block
                .irs
                .splice(fused_init.start_idx..=fused_init.end_idx, replacement_irs);
            true
        } else {
            false
        }
    }

    fn search_nbit_initialization(&self, irs: &[Ir51]) -> Option<FusedInitialization> {
        for (ir_idx, ir) in irs.iter().enumerate() {
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
                    Self::find_next_initialization(irs, *low_addr, ir_idx)
            {
                let start_idx = ir_idx;
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

    fn find_next_initialization(
        irs: &[Ir51],
        low_addr: Addr,
        from_idx: usize,
    ) -> Option<(Constant, usize)> {
        let target_addr = low_addr + 1;
        for (ir_idx, ir) in irs.iter().enumerate().skip(from_idx + 1) {
            if let Ir51::Move {
                dest: Operand::Var {
                    addr: high_addr, ..
                },
                src: Operand::Constant(high_value),
            } = ir
                && *high_addr == target_addr
            {
                return Some((*high_value, ir_idx));
            }
        }

        None
    }

    // Phase 2: Relational Condition / Operation Aggregation
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
            let replacement_irs = vec![Ir51::BinOp {
                op: fused_op.op,
                dest: fused_op.dest,
                lhs: fused_op.lhs,
                rhs: fused_op.rhs,
            }];

            block
                .irs
                .splice(fused_op.start_idx..=fused_op.end_idx, replacement_irs);
            true
        } else {
            false
        }
    }

    fn search_aggr_operation(irs: &[Ir51]) -> Option<FusedOperation> {
        let window_size = 3;

        for (start_idx, window) in irs.windows(window_size).enumerate() {
            let (first_ir, second_ir, third_ir) = (window[0], window[1], window[2]);

            // Finds instruction sequences like this:
            // 1: A = #0xF6
            // 2: A = A + iData[0x08]
            // 3: t0 = PSW != #0x00

            // TODO: dont hardcode register addresses
            if let Ir51::Move {
                dest: Operand::Var { addr: 0xe0, .. },
                src: Operand::Constant(constant_val),
            } = first_ir
                && let Ir51::BinOp {
                    op: BinOp::Add,
                    dest: Operand::Var { addr: 0xe0, .. },
                    lhs: Operand::Var { addr: 0xe0, .. },
                    rhs,
                } = second_ir
                && let Ir51::BinOp {
                    op: BinOp::Ne,
                    dest: Operand::Temp(0),
                    lhs: Operand::Var { addr: 0xd0, .. },
                    rhs: Operand::Constant(0),
                } = third_ir
            {
                return Some(FusedOperation {
                    start_idx,
                    end_idx: start_idx + window_size - 1,
                    op: BinOp::Ge,
                    dest: Operand::Temp(0),
                    lhs: rhs,
                    rhs: Operand::Constant(0u8.wrapping_sub(constant_val as u8).into()),
                });
            };
        }

        None
    }

    // Phase 3: Instruction Search Helpers
    fn find_last_initialization(
        target_op: &Operand,
        irs: &[Ir51],
        from_idx: usize,
    ) -> Option<(Operand, usize)> {
        for (ir_idx, ir) in irs.iter().enumerate().take(from_idx).rev() {
            match ir {
                Ir51::Move { dest, src } if dest == target_op => return Some((*src, ir_idx)),
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
        for (ir_idx, ir) in irs.iter().enumerate().skip(from_idx) {
            match ir {
                Ir51::Move { dest, src } if src == target_op => return Some((*dest, ir_idx)),
                _ => {}
            }
        }

        None
    }

    fn find_next_binop_usage(
        target_op: &Operand,
        irs: &[Ir51],
        from_idx: usize,
    ) -> Option<BinOpUsage> {
        for (ir_idx, ir) in irs.iter().enumerate().skip(from_idx + 1) {
            match ir {
                Ir51::BinOp { op, dest, lhs, rhs } if lhs == target_op => {
                    return Some(BinOpUsage {
                        op: *op,
                        dest: *dest,
                        rhs: *rhs,
                        index: ir_idx,
                    });
                }
                _ => {}
            }
        }

        None
    }

    fn find_last_add(irs: &[Ir51], from_idx: usize) -> Option<usize> {
        for (ir_idx, ir) in irs.iter().enumerate().take(from_idx).rev() {
            if let Ir51::BinOp { op: BinOp::Add, .. } = ir {
                return Some(ir_idx);
            }
        }

        None
    }

    fn find_adjacent_subb(irs: &[Ir51], subb_idx: usize) -> Option<usize> {
        for (ir_idx, ir) in irs.iter().enumerate().skip(subb_idx + 1) {
            match ir {
                // If carry is cleared before the next SUBB, they are not part
                // of the same 16-bit subtraction.
                Ir51::Move {
                    dest: Operand::Register(Register::PSW),
                    src: Operand::Constant(0),
                } => return None,

                Ir51::BinOp {
                    op: BinOp::Subb, ..
                } => return Some(ir_idx),

                _ => {}
            }
        }

        None
    }

    fn find_adjacent_bitwise(irs: &[Ir51], from_idx: usize, target_op: BinOp) -> Option<usize> {
        for (ir_idx, ir) in irs.iter().enumerate().skip(from_idx + 1) {
            if let Ir51::BinOp { op, .. } = ir
                && *op == target_op
            {
                return Some(ir_idx);
            }
        }

        None
    }
}
