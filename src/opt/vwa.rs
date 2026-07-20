use std::collections::HashSet;

use crate::{
    common::{BinOp, BlockId, Constant, Ir51, Operand, TempIndex, VarId},
    opt::{
        cfg_restorer::{BasicBlock, ControlFlowGraph},
        ssa::{CHKAlgorithm, Dominators},
    },
};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum TrackedValue {
    Classic(VarId),
    Fused(VarId, VarId),
    Temporary(TempIndex),
    Constant(Constant),
}

const BITS: usize = 8;

impl TrackedValue {
    pub fn from_operand(op: &Operand) -> Self {
        match op {
            Operand::Var(id) => Self::Classic(*id),
            Operand::Temporary(id) => Self::Temporary(*id),
            Operand::Constant(value) => Self::Constant(*value),
            Operand::Concat(v1, v2) => Self::Fused(*v1, *v2),

            Operand::Register(_)
            | Operand::TempSSAVar { .. }
            | Operand::SSAVar { .. }
            | Operand::Address(_, _) => unreachable!(),
        }
    }

    pub fn into_operand(self) -> Operand {
        match self {
            Self::Classic(id) => Operand::Var(id),
            Self::Temporary(idx) => Operand::Temporary(idx),
            Self::Constant(val) => Operand::Constant(val),
            Self::Fused(v1, v2) => Operand::Concat(v1, v2),
        }
    }

    pub fn fuse_with(self, rhs: Self) -> Operand {
        match (self, rhs) {
            (Self::Classic(low), Self::Classic(high)) => Operand::Concat(low, high),
            (Self::Constant(low), Self::Constant(high)) => Operand::Constant(low | high << BITS),

            _ => todo!(),
        }
    }
}

// This struct represents the high and low part
// of the lhs and rhs of a binary operation
#[derive(Default, Debug)]
struct AluPatternTracker {
    lhs_high: Option<TrackedValue>,
    lhs_low: Option<TrackedValue>,
    rhs_high: Option<TrackedValue>,
    rhs_low: Option<TrackedValue>,

    start_index: usize,
    end_index: usize,

    dest: Option<TrackedValue>,
    binop: Option<BinOp>,
}

impl AluPatternTracker {
    pub fn record_byte_pair(&mut self, left: TrackedValue, right: TrackedValue) {
        if self.lhs_high.is_none() && self.rhs_high.is_none() {
            self.lhs_high = Some(left);
            self.rhs_high = Some(right);
        } else {
            self.lhs_low = Some(left);
            self.rhs_low = Some(right);
        }
    }

    pub fn propagate_alias(&mut self, from: TrackedValue, to: TrackedValue) {
        for field in [
            &mut self.lhs_high,
            &mut self.lhs_low,
            &mut self.rhs_high,
            &mut self.rhs_low,
        ] {
            if *field == Some(from) {
                *field = Some(to);
            }
        }
    }

    pub fn contains(&self, var: TrackedValue) -> bool {
        [self.lhs_high, self.lhs_low, self.rhs_high, self.rhs_low].contains(&Some(var))
    }

    pub fn is_fully_resolved(&self) -> bool {
        [self.lhs_high, self.lhs_low, self.rhs_high, self.rhs_low]
            .iter()
            .all(|field| field.is_some())
            && self.binop.is_some()
    }
}

#[derive(Default)]
pub struct VariableWidthAggregation;

// TODO: this phase probably needs to be more general
impl VariableWidthAggregation {
    pub fn execute(&mut self, cfg: &mut ControlFlowGraph) {
        let shared_blocks: Vec<&BasicBlock> = cfg.blocks.values().collect();
        let immediate_dominators = CHKAlgorithm::execute(&shared_blocks);

        let mut modified = true;
        while modified {
            modified = false;

            for block in cfg.blocks.values_mut() {
                modified |= self.aggregate_intra_block(block);
            }

            modified |= self.fuse_loop_increments(cfg, &immediate_dominators);
        }

        if cfg!(debug_assertions) {
            println!("======================== VAR WIDTH AGGREGATION ========================");
            println!("Immediate Dominators: {immediate_dominators:?}");
            let mut sorted_blocks: Vec<&BasicBlock> = cfg.blocks.values().collect();
            sorted_blocks.sort_by_key(|b| b.id);
            for block in sorted_blocks {
                print!("{}", block);
            }
        }
    }

    fn aggregate_intra_block(&mut self, block: &mut BasicBlock) -> bool {
        let mut defs: HashSet<TrackedValue> = HashSet::new();

        let mut tracker = AluPatternTracker::default();
        let mut track_mode = false;

        for (i, ir) in block.irs.iter_mut().enumerate().rev() {
            if track_mode {
                Self::trace_dependency(ir, i, &mut tracker, &mut defs);
            } else {
                match ir {
                    Ir51::BinOp { op, dest, lhs, rhs } if op.is_glue_op() => {
                        let dest = TrackedValue::from_operand(dest);
                        let lhs = TrackedValue::from_operand(lhs);
                        let rhs = TrackedValue::from_operand(rhs);

                        track_mode = true;

                        tracker.dest = Some(dest);
                        tracker.end_index = i;

                        defs.insert(lhs);
                        defs.insert(rhs);
                    }

                    _ => {}
                }
            }
        }

        if tracker.is_fully_resolved() && defs.is_empty() {
            Self::rewrite_with_fused_op(block, &tracker);
            return true;
        }

        false
    }

    fn rewrite_with_fused_op(block: &mut BasicBlock, tracker: &AluPatternTracker) {
        let from = tracker.start_index;
        let to = tracker.end_index + 1;

        let lhs = tracker
            .lhs_low
            .unwrap()
            .fuse_with(tracker.lhs_high.unwrap());

        let rhs = tracker
            .rhs_low
            .unwrap()
            .fuse_with(tracker.rhs_high.unwrap());

        let dest = tracker.dest.unwrap().into_operand();
        let with = vec![Ir51::BinOp {
            op: tracker.binop.unwrap(),
            dest,
            lhs,
            rhs,
        }];

        block.irs.splice(from..to, with);
    }

    fn trace_dependency(
        ir: &Ir51,
        i: usize,
        tracker: &mut AluPatternTracker,
        defs: &mut HashSet<TrackedValue>,
    ) {
        match ir {
            Ir51::Move { dest, src } => {
                let dest = TrackedValue::from_operand(dest);
                let src = TrackedValue::from_operand(src);

                tracker.propagate_alias(dest, src);
                tracker.start_index = i;
            }

            Ir51::BinOp { op, dest, lhs, rhs } if op.is_glue_op() => {
                let dest = TrackedValue::from_operand(dest);
                let lhs = TrackedValue::from_operand(lhs);
                let rhs = TrackedValue::from_operand(rhs);
                if !defs.remove(&dest) {
                    return;
                }

                tracker.binop = Some(*op);
                tracker.start_index = i;
                if !tracker.contains(lhs) {
                    defs.insert(lhs);
                };
                if !tracker.contains(rhs) {
                    defs.insert(rhs);
                };
            }

            Ir51::BinOp { op, dest, lhs, rhs } if op.is_math_op() => {
                let dest = TrackedValue::from_operand(dest);
                let lhs = TrackedValue::from_operand(lhs);
                let rhs = TrackedValue::from_operand(rhs);

                if !defs.remove(&dest) {
                    return;
                }

                tracker.start_index = i;
                tracker.record_byte_pair(lhs, rhs);
            }

            _ => {}
        }
    }

    // TODO: i just need to analyze triangles (probably yes just for while loops)?
    fn fuse_loop_increments(&mut self, cfg: &mut ControlFlowGraph, idoms: &Dominators) -> bool {
        let Some((head, branch, tail)) = Self::find_triangle(cfg, idoms) else {
            return false;
        };

        Self::analyze_triangle(head, branch, tail, cfg).is_some()
    }

    fn analyze_triangle(
        head: BlockId,
        branch: BlockId,
        tail: BlockId,
        cfg: &mut ControlFlowGraph,
    ) -> Option<()> {
        let (high_part, binop) = Self::analyze_branch(branch, cfg)?;

        let Ir51::BinOp { op, .. } = binop else {
            return None;
        };
        let (low_part, indices) = Self::analyze_head(head, cfg, op)?;

        Self::fuse_triangle(
            cfg, low_part, high_part, binop, &indices, head, branch, tail,
        )
    }

    fn fuse_triangle(
        cfg: &mut ControlFlowGraph,
        low_part: Operand,
        high_part: Operand,
        binop: Ir51,
        indices: &[usize],
        headid: BlockId,
        branchid: BlockId,
        tailid: BlockId,
    ) -> Option<()> {
        let head_block = cfg.blocks.get_mut(&headid).unwrap();

        let mut index = 0;
        head_block.irs.retain(|_| {
            let keep = !indices.contains(&index);
            index += 1;
            keep
        });

        let low = TrackedValue::from_operand(&low_part);
        let high = TrackedValue::from_operand(&high_part);

        let Ir51::BinOp { op, rhs, .. } = binop else {
            return None;
        };

        let var = low.fuse_with(high);
        head_block.irs.push(Ir51::BinOp {
            op,
            dest: var,
            lhs: var,
            rhs,
        });
        head_block.irs.push(Ir51::Jump(tailid));
        head_block.successors.retain(|s| *s != branchid);

        let tail_block = cfg.blocks.get_mut(&tailid).unwrap();
        tail_block.predecessors.retain(|p| *p != branchid);

        cfg.blocks.remove(&branchid);
        Some(())
    }

    fn find_triangle(
        cfg: &ControlFlowGraph,
        idoms: &Dominators,
    ) -> Option<(BlockId, BlockId, BlockId)> {
        // Topologically search where a triangle could be
        for block in cfg.blocks.values() {
            let [suc1, suc2] = block.successors.as_slice() else {
                continue;
            };

            let head = Some(block.id);

            let Some(branch_blockid) = [suc1, suc2]
                .into_iter()
                .find(|s| matches!(idoms.get(s), Some(idom) if *idom == block.id))
            else {
                continue;
            };

            let branch = Some(*branch_blockid);

            let branch_block = &cfg.blocks[branch_blockid];
            let [branch_suc] = branch_block.successors.as_slice() else {
                continue;
            };

            let mut tail = None;
            if branch_suc != branch_blockid && (branch_suc == suc1 || branch_suc == suc2) {
                tail = Some(*branch_suc);
            }

            if let Some(head) = head
                && let Some(branch) = branch
                && let Some(tail) = tail
            {
                return Some((head, branch, tail));
            }
        }

        None
    }

    fn analyze_head(
        headid: BlockId,
        cfg: &ControlFlowGraph,
        binop: BinOp,
    ) -> Option<(Operand, Vec<usize>)> {
        let head = &cfg.blocks[&headid];
        let mut indices = Vec::new();

        let lastir = head.irs.last()?;
        let Ir51::BranchIf { cond, .. } = *lastir else {
            return None;
        };

        indices.push(head.irs.len() - 1);
        let mut low_part: Option<Operand> = None;

        for (i, ir) in head.irs.iter().enumerate().rev().skip(1) {
            match ir {
                Ir51::BinOp { op, dest, lhs, rhs } if *dest == cond => {
                    indices.push(i);
                    low_part = Some(*lhs);
                }

                Ir51::BinOp { op, dest, lhs, rhs }
                    if *op == binop && low_part == Some(*dest) && low_part == Some(*lhs) =>
                {
                    indices.push(i);
                    low_part = Some(*dest);
                }

                _ => {}
            }
        }

        let low_part = low_part?;
        Some((low_part, indices))
    }

    // TODO: maybe use a function that checks whether or not 2 instruction "correspond"
    // to the same calculation but on the high and low part

    // Analyzes the branch block and returns the variable representing the high byte
    fn analyze_branch(branchid: BlockId, cfg: &ControlFlowGraph) -> Option<(Operand, Ir51)> {
        let branch_block = &cfg.blocks[&branchid];

        let [op, jmp] = &branch_block.irs.as_slice() else {
            return None;
        };

        if let Ir51::BinOp {
            op: binop, dest, ..
        } = op
            && binop.is_math_op()
            && matches!(jmp, Ir51::Jump(_))
        {
            Some((*dest, *op))
        } else {
            None
        }
    }
}
