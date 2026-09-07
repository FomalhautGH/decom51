use std::collections::HashSet;

use crate::{
    common::{Addr, AddrSpace, BinOp, Constant, Ir51, Operand},
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
        cfg.remove_useless_irs();

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
        let mut has_modified = true;
        while has_modified {
            has_modified = false;

            for block in cfg.blocks.values_mut() {
                has_modified |= self.aggregate_intra_block(block);
                has_modified |= self.aggregate_inter_block(block);
            }
        }
    }

    // --- Phase 1A: Intra-Block Arithmetic & Bitwise Fusion ---
    fn aggregate_intra_block(&mut self, block: &mut BasicBlock) -> bool {
        if let Some(fused_op) = Self::search_nbit_operation(&block.irs) {
            self.expand_fused_addresses(&fused_op);
            Self::replace_fused_op(block, fused_op);
            true
        } else {
            false
        }
    }

    fn replace_fused_init(block: &mut BasicBlock, fused_init: FusedInitialization) {
        let replacement_ir = Ir51::Move {
            dest: fused_init.dest,
            src: fused_init.src,
        };

        // TODO: Avoid range splice if unrelated instructions are interleaved.
        block
            .irs
            .get_mut(fused_init.start_idx..=fused_init.end_idx)
            .unwrap()
            .fill(Ir51::Nop);

        block.irs[fused_init.start_idx] = replacement_ir;
    }

    fn replace_fused_op(block: &mut BasicBlock, fused_op: FusedOperation) {
        let replacement_ir = Ir51::BinOp {
            op: fused_op.op,
            dest: fused_op.dest,
            lhs: fused_op.lhs,
            rhs: fused_op.rhs,
        };
        // TODO: Avoid range splice if unrelated instructions are interleaved.
        block
            .irs
            .get_mut(fused_op.start_idx..=fused_op.end_idx)
            .unwrap()
            .fill(Ir51::Nop);

        block.irs[fused_op.start_idx] = replacement_ir;
    }

    fn search_nbit_operation(irs: &[Ir51]) -> Option<FusedOperation> {
        for (ir_idx, instr) in irs.iter().enumerate() {
            if let Ir51::BinOp { op, .. } = instr {
                let fused_op = match op {
                    // TODO: Handle multi-byte SUBB chains beyond 2-byte pairs.
                    BinOp::Subb => Self::try_fusing_subb(irs, ir_idx),
                    BinOp::Addc => Self::try_fusing_addc(irs, ir_idx),

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

    fn try_fusing_addc_constant(
        irs: &[Ir51],
        high_addc_idx: usize,
        high_lhs_init: &Operand,
    ) -> Option<FusedOperation> {
        let accumulator = Operand::Var {
            addr: 0xE0,
            space: AddrSpace::iData,
            size: 1,
        };

        let Ir51::BinOp {
            op: BinOp::Addc,
            dest: high_dest,
            lhs: _high_lhs,
            rhs: _high_rhs,
        } = irs[high_addc_idx]
        else {
            unreachable!()
        };

        let high_lhs_const = high_lhs_init.to_const_value()?;
        let (high_dest_op, end_idx) = Self::find_next_usage(&high_dest, irs, high_addc_idx)?;
        let _high_dest_addr = high_dest_op.to_var_addr()?;

        let low_add_idx = Self::find_last_add(irs, high_addc_idx)?;
        let Ir51::BinOp {
            op: BinOp::Add,
            lhs: low_lhs,
            rhs: low_rhs,
            ..
        } = irs[low_add_idx]
        else {
            unreachable!()
        };

        let (low_lhs_init_op, _) = Self::find_last_initialization(&low_lhs, irs, low_add_idx)?;
        let low_lhs_const = low_lhs_init_op.to_const_value()?;

        let (low_rhs_init_op, start_idx) =
            Self::find_last_initialization(&low_rhs, irs, low_add_idx)?;
        let low_rhs_addr = low_rhs_init_op.to_var_addr()?;

        let (low_dest_op, _) = Self::find_next_usage(&accumulator, irs, low_add_idx)?;
        let low_dest_addr = low_dest_op.to_var_addr()?;

        let op = BinOp::Add;
        let lhs = Operand::Constant((high_lhs_const << 8) | low_lhs_const);

        let rhs = Operand::Var {
            addr: low_rhs_addr,
            space: AddrSpace::iData,
            size: 2,
        };

        let dest = Operand::Var {
            addr: low_dest_addr,
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

    fn try_fusing_addc_variables(
        irs: &[Ir51],
        high_addc_idx: usize,
        high_lhs_init: &Operand,
    ) -> Option<FusedOperation> {
        // TODO: remove this and in fusing subbc, here we use the normalized version of the A
        // register to find it in `find_next_usage`, find a bettere solution for recognizing patterns
        let accumulator = Operand::Var {
            addr: 0xE0,
            space: AddrSpace::iData,
            size: 1,
        };

        let Ir51::BinOp {
            op: BinOp::Addc,
            dest: high_dest,
            lhs: _high_lhs,
            rhs: _high_rhs,
        } = irs[high_addc_idx]
        else {
            unreachable!()
        };

        let _high_lhs_addr = high_lhs_init.to_var_addr()?;
        let (high_dest_op, end_idx) = Self::find_next_usage(&high_dest, irs, high_addc_idx)?;
        let _high_dest_addr = high_dest_op.to_var_addr()?;

        let low_add_idx = Self::find_last_add(irs, high_addc_idx)?;
        let Ir51::BinOp {
            op: BinOp::Add,
            lhs: low_lhs,
            rhs: low_rhs,
            ..
        } = irs[low_add_idx]
        else {
            unreachable!()
        };

        let low_rhs_addr = low_rhs.to_var_addr()?;
        let (low_lhs_op, start_idx) = Self::find_last_initialization(&low_lhs, irs, low_add_idx)?;
        let (low_dest_op, _) = Self::find_next_usage(&accumulator, irs, low_add_idx)?;
        let low_lhs_addr = low_lhs_op.to_var_addr()?;
        let low_dest_addr = low_dest_op.to_var_addr()?;

        let op = BinOp::Add;
        let lhs = Operand::Var {
            addr: low_lhs_addr,
            space: AddrSpace::iData,
            size: 2,
        };

        let rhs = Operand::Var {
            addr: low_rhs_addr,
            space: AddrSpace::iData,
            size: 2,
        };

        let dest = Operand::Var {
            addr: low_dest_addr,
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
            lhs: addc_lhs,
            rhs: _addc_rhs,
            ..
        } = irs[addc_idx]
        else {
            unreachable!()
        };

        // Searches what value the accumulator (A) is.
        let (high_lhs_init, _) = Self::find_last_initialization(&addc_lhs, irs, addc_idx)?;
        match high_lhs_init {
            Operand::Constant(_) => Self::try_fusing_addc_constant(irs, addc_idx, &high_lhs_init),
            Operand::Var { .. } => Self::try_fusing_addc_variables(irs, addc_idx, &high_lhs_init),
            _ => unreachable!(),
        }
    }

    // Fuses subb used for condition
    fn try_fusing_subb_constant(
        irs: &[Ir51],
        low_subb_idx: usize,
        low_rhs_const: Constant,
    ) -> Option<FusedOperation> {
        let carry_flag = Operand::Var {
            addr: 0xD7,
            space: AddrSpace::iData,
            size: 1,
        };

        let Ir51::BinOp {
            op: BinOp::Subb,
            lhs: low_lhs,
            ..
        } = irs[low_subb_idx]
        else {
            unreachable!()
        };

        let (low_lhs_op, start_idx) = Self::find_last_initialization(&low_lhs, irs, low_subb_idx)?;
        let low_lhs_addr = low_lhs_op.to_var_addr()?;

        let high_subb_idx = Self::find_adjacent_subb(irs, low_subb_idx)?;
        let Ir51::BinOp {
            op: BinOp::Subb,
            rhs: high_rhs,
            ..
        } = irs[high_subb_idx]
        else {
            unreachable!()
        };

        let high_rhs_const = high_rhs.to_const_value()?;
        let (high_lhs_init_op, _) = Self::find_last_initialization(&low_lhs, irs, high_subb_idx)?;
        let _high_lhs_addr = high_lhs_init_op.to_var_addr()?;

        let op = BinOp::Lt;
        let lhs = Operand::Var {
            addr: low_lhs_addr,
            space: AddrSpace::iData,
            size: 2,
        };

        let rhs = Operand::Constant((high_rhs_const << 8) | low_rhs_const);
        let end_idx = high_subb_idx;

        Some(FusedOperation {
            start_idx,
            end_idx,
            op,
            dest: carry_flag,
            lhs,
            rhs,
        })
    }

    fn try_fusing_subb_variable(
        irs: &[Ir51],
        low_subb_idx: usize,
        low_rhs_var_addr: Addr,
    ) -> Option<FusedOperation> {
        let Ir51::BinOp {
            op: BinOp::Subb,
            dest: low_dest,
            lhs: low_lhs,
            rhs: _low_rhs,
        } = irs[low_subb_idx]
        else {
            unreachable!()
        };

        let low_rhs_addr = low_rhs_var_addr;

        let (low_lhs_op, start_idx) = Self::find_last_initialization(&low_lhs, irs, low_subb_idx)?;
        let (low_dest_op, _) = Self::find_next_usage(&low_dest, irs, low_subb_idx)?;
        let low_lhs_addr = low_lhs_op.to_var_addr()?;
        let low_dest_addr = low_dest_op.to_var_addr()?;

        let high_subb_idx = Self::find_adjacent_subb(irs, low_subb_idx)?;

        let (high_lhs_op, _) = Self::find_last_initialization(&low_lhs, irs, high_subb_idx)?;
        let (high_dest_op, end_idx) = Self::find_next_usage(&low_dest, irs, high_subb_idx)?;
        let _high_lhs_addr = high_lhs_op.to_var_addr()?;
        let _high_dest_addr = high_dest_op.to_var_addr()?;

        let op = BinOp::Sub;
        let lhs = Operand::Var {
            addr: low_lhs_addr,
            space: AddrSpace::iData,
            size: 2,
        };

        let rhs = Operand::Var {
            addr: low_rhs_addr,
            space: AddrSpace::iData,
            size: 2,
        };

        let dest = Operand::Var {
            addr: low_dest_addr,
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
            rhs: subb_rhs,
            ..
        } = irs[subb_idx]
        else {
            unreachable!()
        };

        match subb_rhs {
            Operand::Constant(const_val) => {
                Self::try_fusing_subb_constant(irs, subb_idx, const_val)
            }
            Operand::Var { addr: var_addr, .. } => {
                Self::try_fusing_subb_variable(irs, subb_idx, var_addr)
            }
            _ => unreachable!(),
        }
    }

    fn try_fusing_bitwise(
        irs: &[Ir51],
        low_op_idx: usize,
        target_op: BinOp,
    ) -> Option<FusedOperation> {
        let Ir51::BinOp {
            op,
            dest: low_dest,
            lhs: low_lhs,
            rhs: low_rhs,
        } = irs[low_op_idx]
        else {
            unreachable!()
        };

        if op != target_op {
            return None;
        }

        let low_rhs_addr = low_rhs.to_var_addr()?;
        let (low_lhs_op, start_idx) = Self::find_last_initialization(&low_lhs, irs, low_op_idx)?;
        let (low_dest_op, _) = Self::find_next_usage(&low_dest, irs, low_op_idx)?;
        let low_lhs_addr = low_lhs_op.to_var_addr()?;
        let low_dest_addr = low_dest_op.to_var_addr()?;

        let high_op_idx = Self::find_adjacent_bitwise(irs, low_op_idx, target_op)?;

        let Ir51::BinOp {
            dest: high_dest,
            lhs: high_lhs,
            rhs: high_rhs,
            ..
        } = irs[high_op_idx]
        else {
            return None;
        };

        let high_rhs_addr = high_rhs.to_var_addr()?;
        let (high_lhs_op, _) = Self::find_last_initialization(&high_lhs, irs, high_op_idx)?;
        let (high_dest_op, end_idx) = Self::find_next_usage(&high_dest, irs, high_op_idx)?;
        let high_lhs_addr = high_lhs_op.to_var_addr()?;
        let high_dest_addr = high_dest_op.to_var_addr()?;

        if low_lhs_addr + 1 != high_lhs_addr
            || low_rhs_addr + 1 != high_rhs_addr
            || low_dest_addr + 1 != high_dest_addr
        {
            return None;
        }

        let op = target_op;
        let lhs = Operand::Var {
            addr: low_lhs_addr,
            space: AddrSpace::iData,
            size: 2,
        };

        let rhs = Operand::Var {
            addr: low_rhs_addr,
            space: AddrSpace::iData,
            size: 2,
        };

        let dest = Operand::Var {
            addr: low_dest_addr,
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
            lhs: low_lhs,
            rhs: high_lhs,
        } = irs[or_idx]
        else {
            unreachable!()
        };

        let high_lhs_addr = high_lhs.to_var_addr()?;
        let (low_lhs_op, start_idx) = Self::find_last_initialization(&low_lhs, irs, or_idx)?;
        let consumer_op = Self::find_next_binop_usage(&or_dest, irs, or_idx)?;
        let low_lhs_addr = low_lhs_op.to_var_addr()?;

        (low_lhs_addr + 1 == high_lhs_addr).then_some(bool::default())?;

        let op = consumer_op.op;
        let rhs = consumer_op.rhs;
        let dest = consumer_op.dest;
        let end_idx = consumer_op.index;
        let lhs = Operand::Var {
            addr: low_lhs_addr,
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
        if let Some(var_addr) = fused_op.dest.to_var_addr() {
            self.fused_addresses.insert(var_addr);
        }
        if let Some(var_addr) = fused_op.lhs.to_var_addr() {
            self.fused_addresses.insert(var_addr);
        }
        if let Some(var_addr) = fused_op.rhs.to_var_addr() {
            self.fused_addresses.insert(var_addr);
        }
    }

    // --- Phase 1B: Inter-Block Constant Initialization Fusion ---
    fn aggregate_inter_block(&self, block: &mut BasicBlock) -> bool {
        if let Some(fused_init) = self.search_nbit_initialization(&block.irs) {
            Self::replace_fused_init(block, fused_init);
            true
        } else {
            false
        }
    }

    fn as_constant(operand: &Operand, irs: &[Ir51], search_from_idx: usize) -> Option<Constant> {
        if let Operand::Constant(_) = *operand {
            operand.to_const_value()
        } else {
            let (src_op, _) = Self::find_last_initialization(operand, irs, search_from_idx)?;
            src_op.to_const_value()
        }
    }

    fn search_nbit_initialization(&self, irs: &[Ir51]) -> Option<FusedInitialization> {
        for (ir_idx, ir) in irs.iter().enumerate() {
            if let Ir51::Move {
                dest:
                    Operand::Var {
                        addr: low_var_addr,
                        space: addr_space,
                        ..
                    },
                src,
            } = ir
                && self.fused_addresses.contains(low_var_addr)
            {
                let low_const = Self::as_constant(src, irs, ir_idx)?;

                if let Some((high_const, end_idx)) =
                    Self::find_next_initialization_of_addr(low_var_addr + 1, irs, ir_idx)
                {
                    let start_idx = ir_idx;
                    let dest = Operand::Var {
                        addr: *low_var_addr,
                        space: *addr_space,
                        size: 2,
                    };

                    let src = Operand::Constant((high_const << 8) | low_const);
                    return Some(FusedInitialization {
                        start_idx,
                        end_idx,
                        dest,
                        src,
                    });
                }
            }
        }

        None
    }

    fn find_next_initialization_of_addr(
        target_addr: Addr,
        irs: &[Ir51],
        search_after_idx: usize,
    ) -> Option<(Constant, usize)> {
        for (ir_idx, ir) in irs.iter().enumerate().skip(search_after_idx + 1) {
            if let Ir51::Move {
                dest:
                    Operand::Var {
                        addr: candidate_high_addr,
                        ..
                    },
                src,
            } = ir
                && *candidate_high_addr == target_addr
            {
                let high_const = Self::as_constant(src, irs, ir_idx)?;
                return Some((high_const, ir_idx));
            }
        }

        None
    }

    // Phase 2: Relational Condition / Operation Aggregation
    fn aggregate_operations(&mut self, cfg: &mut ControlFlowGraph) {
        let mut has_modified = true;
        while has_modified {
            has_modified = false;

            for block in cfg.blocks.values_mut() {
                has_modified |= self.aggregate_ops(block);
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
            let (load_const_ir, add_var_ir, check_flag_ir) = (window[0], window[1], window[2]);

            // Finds instruction sequences like this:
            // 1: A = #0xF6
            // 2: A = A + iData[0x08]
            // 3: t0 = PSW != #0x00

            // TODO: dont hardcode register addresses
            if let Ir51::Move {
                dest: Operand::Var { addr: 0xe0, .. },
                src: Operand::Constant(threshold_const_val),
            } = load_const_ir
                && let Ir51::BinOp {
                    op: BinOp::Add,
                    dest: Operand::Var { addr: 0xe0, .. },
                    lhs: Operand::Var { addr: 0xe0, .. },
                    rhs: counter_var,
                } = add_var_ir
                && let Ir51::BinOp {
                    op: BinOp::Ne,
                    dest: Operand::Temp(0),
                    lhs: Operand::Var { addr: 0xd0, .. },
                    rhs: Operand::Constant(0),
                } = check_flag_ir
            {
                return Some(FusedOperation {
                    start_idx,
                    end_idx: start_idx + window_size - 1,
                    op: BinOp::Ge,
                    dest: Operand::Temp(0),
                    lhs: counter_var,
                    rhs: Operand::Constant(0u8.wrapping_sub(threshold_const_val as u8).into()),
                });
            };
        }

        None
    }

    // Phase 3: Instruction Search Helpers
    fn find_last_initialization(
        target_op: &Operand,
        irs: &[Ir51],
        search_before_idx: usize,
    ) -> Option<(Operand, usize)> {
        for (ir_idx, ir) in irs.iter().enumerate().take(search_before_idx).rev() {
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
        search_start_idx: usize,
    ) -> Option<(Operand, usize)> {
        for (ir_idx, ir) in irs.iter().enumerate().skip(search_start_idx) {
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
        search_start_idx: usize,
    ) -> Option<BinOpUsage> {
        for (ir_idx, ir) in irs.iter().enumerate().skip(search_start_idx + 1) {
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

    fn find_last_add(irs: &[Ir51], search_before_idx: usize) -> Option<usize> {
        for (ir_idx, ir) in irs.iter().enumerate().take(search_before_idx).rev() {
            if let Ir51::BinOp { op: BinOp::Add, .. } = ir {
                return Some(ir_idx);
            }
        }

        None
    }

    fn find_adjacent_subb(irs: &[Ir51], low_subb_idx: usize) -> Option<usize> {
        for (ir_idx, ir) in irs.iter().enumerate().skip(low_subb_idx + 1) {
            match ir {
                // If carry is cleared before the next SUBB, they are not part
                // of the same 16-bit subtraction.
                Ir51::Move {
                    dest: Operand::Register(Register::C),
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

    fn find_adjacent_bitwise(irs: &[Ir51], low_op_idx: usize, target_op: BinOp) -> Option<usize> {
        for (ir_idx, ir) in irs.iter().enumerate().skip(low_op_idx + 1) {
            if let Ir51::BinOp { op, .. } = ir
                && *op == target_op
            {
                return Some(ir_idx);
            }
        }

        None
    }
}
