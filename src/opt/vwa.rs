use std::collections::HashSet;

use crate::{
    common::{AddrSpace, BinOp, BlockId, Constant, Ir51, Operand, Register, TempIndex, VarId},
    opt::{
        cfg_restorer::{BasicBlock, ControlFlowGraph},
        ssa::{CHKAlgorithm, Dominators},
    },
};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TrackedValue {
    Classic(VarId),
    Fused(VarId, VarId),
    Temporary(TempIndex),
    Constant(Constant),
}

const BITS: usize = 8;

impl TrackedValue {
    pub fn from_operand(op: &Operand) -> Self {
        match op {
            Operand::Local(id) => Self::Classic(*id),
            Operand::Temp(id) => Self::Temporary(*id),
            Operand::Constant(value) => Self::Constant(*value),
            Operand::Concat(v1, v2) => Self::Fused(*v1, *v2),

            Operand::Register(_)
            | Operand::TempSSAVar { .. }
            | Operand::SSAVar { .. }
            | Operand::Var(_, _) => unreachable!(),
        }
    }

    pub fn into_operand(self) -> Operand {
        match self {
            Self::Classic(id) => Operand::Local(id),
            Self::Temporary(idx) => Operand::Temp(idx),
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
        let mut modified = true;
        while modified {
            modified = false;

            for block in cfg.blocks.values_mut() {
                modified |= self.aggregate_intra_block(block);
            }

            let shared_blocks: Vec<&BasicBlock> = cfg.blocks.values().collect();
            let immediate_dominators = CHKAlgorithm::execute(&shared_blocks);
            modified |= self.fuse_loop_increments(cfg, &immediate_dominators);
        }

        if cfg!(debug_assertions) {
            println!("======================== VAR WIDTH AGGREGATION ========================");
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

        let mut stack_mode = None;
        let pop = Ir51::BinOp {
            op: BinOp::Sub,
            dest: Operand::Register(Register::SP),
            lhs: Operand::Register(Register::SP),
            rhs: Operand::Constant(1),
        };

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

                    _ if *ir == pop => {
                        stack_mode = Some(i);
                        break;
                    }

                    _ => {}
                }
            }
        }

        if let Some(idx) = stack_mode {
            return Self::trace_stack(&mut block.irs, idx);
        } else if tracker.is_fully_resolved() && defs.is_empty() {
            Self::rewrite_with_fused_op(block, &tracker);
            return true;
        }

        false
    }

    fn trace_stack(ir: &mut Vec<Ir51>, from: usize) -> bool {
        let mut popped_vars = Vec::new();
        let mut curr = from;

        // Trace backward to find the sequence of POP instructions
        while curr > 0 {
            if let Some((var, next_i)) = Self::parse_pop_step(ir, curr) {
                popped_vars.push(var);
                if next_i == 0 {
                    curr = 0;
                    break;
                }
                curr = next_i - 1;
            } else {
                break;
            }
        }

        let num_vars = popped_vars.len();
        if num_vars == 0 {
            return false;
        }

        // Search for a matching PUSH sequence earlier in the block
        let mut pushed_vars = Vec::new();
        let mut push_start = None;
        let mut j = 0;

        while j < curr {
            if let Some((var, next_j)) = Self::parse_push_step(ir, j) {
                if push_start.is_none() {
                    push_start = Some(j);
                }
                pushed_vars.push(var);
                j = next_j;
                if pushed_vars.len() == num_vars {
                    break;
                }
            } else {
                if !pushed_vars.is_empty() {
                    pushed_vars.clear();
                    push_start = None;
                }
                j += 1;
            }
        }

        if pushed_vars.len() == num_vars
            && pushed_vars == popped_vars
            && let Some(start) = push_start
            && let [high_part, low_part] = pushed_vars.as_slice()
            && let Some(low_var) = low_part.to_local_id()
            && let Some(high_var) = high_part.to_local_id()
        {
            let src = Operand::Concat(low_var, high_var);
            ir.splice(curr + 1..=from, Ir51::generate_pop(src, 2));
            ir.splice(start..j, Ir51::generate_push(src, 2));
            return true;
        }

        false
    }

    // Parse POP instruction
    fn parse_pop_step(irs: &[Ir51], from_where: usize) -> Option<(Operand, usize)> {
        if from_where < 1 {
            return None;
        }

        let dec = &irs[from_where];
        let load = &irs[from_where - 1];

        if dec.is_sp_decrement()
            && let Some(dest) = load.as_stack_load()
        {
            Some((*dest, from_where - 1))
        } else {
            None
        }
    }

    // Parse PUSH instruction
    fn parse_push_step(irs: &[Ir51], from_where: usize) -> Option<(Operand, usize)> {
        if from_where + 2 >= irs.len() {
            return None;
        }

        let inc = &irs[from_where];
        let load = &irs[from_where + 1];
        let store = &irs[from_where + 2];

        if inc.is_sp_increment()
            && let Ir51::Load {
                dest: temp_load,
                src_ptr,
                space: AddrSpace::iData,
            } = load
            && let Some(temp_store) = store.as_stack_store()
            && temp_load == temp_store
        {
            Some((*src_ptr, from_where + 3))
        } else {
            None
        }
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

        let head_block = cfg.blocks.get_mut(&head).unwrap();

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
        head_block.irs.push(Ir51::Jump(tail));
        head_block.successors.retain(|s| *s != branch);

        let tail_block = cfg.blocks.get_mut(&tail).unwrap();
        tail_block.predecessors.retain(|p| *p != branch);

        cfg.blocks.remove(&branch);
        Some(())
    }

    fn find_triangle(
        cfg: &ControlFlowGraph,
        idoms: &Dominators,
    ) -> Option<(BlockId, BlockId, BlockId)> {
        for block in cfg.blocks.values() {
            let [suc1, suc2] = block.successors.as_slice() else {
                continue;
            };

            let Some(branch_blockid) = [suc1, suc2]
                .into_iter()
                .find(|s| matches!(idoms.get(s), Some(idom) if *idom == block.id))
            else {
                continue;
            };

            let branch_block = &cfg.blocks[branch_blockid];
            let [branch_suc] = branch_block.successors.as_slice() else {
                continue;
            };

            if branch_suc != branch_blockid && (branch_suc == suc1 || branch_suc == suc2) {
                return Some((block.id, *branch_blockid, *branch_suc));
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
