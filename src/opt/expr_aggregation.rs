use std::collections::{HashMap, HashSet};

use crate::{
    common::{BlockId, Ir51, Operand},
    lir::{Addr, AddrSpace, BinOp, Constant, TempIndex},
    mir::{Expr, Mir51},
    opt::cfg::{BasicBlock, ControlFlowGraph},
};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TrackedValue {
    Variable(Addr),
    Temporary(TempIndex),
    Constant(Constant),
}

impl TrackedValue {
    pub fn from_operand(operand: &Operand) -> Option<Self> {
        match operand {
            Operand::Var { addr, .. } => Some(Self::Variable(*addr)),
            Operand::Local { id, .. } => Some(Self::Variable(*id as Addr)),
            Operand::Temp(id) => Some(Self::Temporary(*id)),
            Operand::Constant(value) => Some(Self::Constant(*value)),
            Operand::Register(_) | Operand::TempSSAVar { .. } | Operand::SSAVar { .. } => None,
        }
    }
}

type UseCounts = HashMap<TrackedValue, usize>;

#[derive(Default)]
pub struct ExprAggregation {
    definitions: HashMap<TrackedValue, Expr>,
    use_counts: UseCounts,
    emitted_mirs: Vec<Mir51>,
}

// Optimization pass that aggregates expressions into trees (forward substitution).
// Temporary variables are always aggregated, while normal variables are aggregated if used only once.
impl ExprAggregation {
    pub fn execute(&mut self, cfg: &mut ControlFlowGraph) {
        for block in cfg.blocks.values_mut() {
            self.aggregate_block(block);
        }

        if cfg!(debug_assertions) {
            println!("======================== EXPR AGGREGATION ========================");
            let mut sorted_blocks: Vec<&BasicBlock> = cfg.blocks.values().collect();
            sorted_blocks.sort_by_key(|b| b.id);
            for block in sorted_blocks {
                print!("{}", block);
            }
        }
    }

    fn aggregate_block(&mut self, block: &mut BasicBlock) {
        for ir in block.irs.iter() {
            self.record_usages_in_ir(ir);
        }

        for ir in block.irs.iter() {
            self.lift_instruction(ir)
                .expect("Error while translating IR51 instruction to MIR51");
        }

        self.finalize_block(block);
    }

    // Phase 1: Usages Pre-Pass
    fn record_usages_in_ir(&mut self, ir: &Ir51) {
        match ir {
            Ir51::Move { src, .. } => {
                self.increment_use_count(src);
            }

            Ir51::BinOp { lhs, rhs, .. } => {
                self.increment_use_count(lhs);
                self.increment_use_count(rhs);
            }

            Ir51::Store {
                dest_ptr, value, ..
            } => {
                self.increment_use_count(dest_ptr);
                self.increment_use_count(value);
            }

            Ir51::Load { src_ptr, .. } => {
                self.increment_use_count(src_ptr);
            }

            Ir51::BranchIf { cond, .. } => self.increment_use_count(cond),
            Ir51::Nop | Ir51::Jump(_) | Ir51::Call(_) | Ir51::Return => {}
        }
    }

    fn increment_use_count(&mut self, operand: &Operand) {
        if let Some(tracked_val) = TrackedValue::from_operand(operand) {
            self.use_counts
                .entry(tracked_val)
                .and_modify(|counter| *counter += 1)
                .or_insert(1);
        }
    }

    fn decrement_use_count(&mut self, operand: &Operand) {
        if let Some(tracked_val) = TrackedValue::from_operand(operand) {
            self.use_counts
                .entry(tracked_val)
                .and_modify(|counter| *counter = counter.saturating_sub(1));
        }
    }

    // Phase 2: Instruction Lifting & Expression Inlining
    fn lift_instruction(&mut self, ir: &Ir51) -> Option<()> {
        match ir {
            Ir51::Move { dest, src } => self.lift_move(dest, src),
            Ir51::BinOp { op, dest, lhs, rhs } => self.lift_binop(op, dest, lhs, rhs),
            Ir51::Store {
                space,
                dest_ptr,
                value,
            } => self.lift_store(space, dest_ptr, value),
            Ir51::Load {
                space,
                dest,
                src_ptr,
            } => self.lift_load(space, dest, src_ptr),
            Ir51::BranchIf {
                cond,
                true_jmp,
                false_jmp,
            } => self.lift_branch_if(cond, true_jmp, false_jmp),
            Ir51::Jump(target_block_id) => self.lift_jump(*target_block_id),
            Ir51::Call(target_block_id) => self.lift_call(*target_block_id),
            Ir51::Return => self.lift_return(),
            Ir51::Nop => Some(()),
        }
    }

    fn lift_move(&mut self, dest: &Operand, src: &Operand) -> Option<()> {
        let dest_expr: Expr = (*dest).try_into().ok()?;
        let resolved_src = self.resolve_operand(src)?;

        self.decrement_use_count(src);

        let assignment = Mir51::Assign {
            dest: dest_expr,
            src: resolved_src.clone(),
        };

        self.aggregate_or_emit(dest, resolved_src, assignment);
        Some(())
    }

    fn lift_binop(
        &mut self,
        op: &BinOp,
        dest: &Operand,
        lhs: &Operand,
        rhs: &Operand,
    ) -> Option<()> {
        let dest_expr: Expr = (*dest).try_into().ok()?;
        let resolved_lhs = Box::new(self.resolve_operand(lhs)?);
        let resolved_rhs = Box::new(self.resolve_operand(rhs)?);

        self.decrement_use_count(lhs);
        self.decrement_use_count(rhs);

        let binop_expr = match op {
            BinOp::Add | BinOp::Addc => Expr::Add(resolved_lhs, resolved_rhs),
            BinOp::Sub | BinOp::Subb => Expr::Sub(resolved_lhs, resolved_rhs),
            BinOp::Mul => Expr::Mul(resolved_lhs, resolved_rhs),
            BinOp::Div => Expr::Div(resolved_lhs, resolved_rhs),
            BinOp::Mod => Expr::Mod(resolved_lhs, resolved_rhs),
            BinOp::And => Expr::And(resolved_lhs, resolved_rhs),
            BinOp::Xor => Expr::Xor(resolved_lhs, resolved_rhs),
            BinOp::Or => Expr::Or(resolved_lhs, resolved_rhs),
            BinOp::Eq => Expr::Eq(resolved_lhs, resolved_rhs),
            BinOp::Lt => Expr::Lt(resolved_lhs, resolved_rhs),
            BinOp::Gt => Expr::Gt(resolved_lhs, resolved_rhs),
            BinOp::Le => Expr::Le(resolved_lhs, resolved_rhs),
            BinOp::Ge => Expr::Ge(resolved_lhs, resolved_rhs),
            BinOp::Ne => Expr::Ne(resolved_lhs, resolved_rhs),
        };

        let assignment = Mir51::Assign {
            dest: dest_expr,
            src: binop_expr.clone(),
        };

        self.aggregate_or_emit(dest, binop_expr, assignment);
        Some(())
    }

    fn lift_load(&mut self, _space: &AddrSpace, dest: &Operand, src_ptr: &Operand) -> Option<()> {
        let dest_expr: Expr = (*dest).try_into().ok()?;
        let resolved_src_ptr = self.resolve_operand(src_ptr)?;

        self.decrement_use_count(src_ptr);

        let deref_expr = Expr::Deref(Box::new(resolved_src_ptr.clone()));
        let assignment = Mir51::Assign {
            dest: dest_expr,
            src: deref_expr,
        };

        self.aggregate_or_emit(dest, resolved_src_ptr, assignment);
        Some(())
    }

    fn lift_store(
        &mut self,
        _space: &AddrSpace,
        dest_ptr: &Operand,
        value: &Operand,
    ) -> Option<()> {
        let resolved_value = self.resolve_operand(value)?;
        let resolved_dest_ptr = self.resolve_operand(dest_ptr)?;

        self.decrement_use_count(dest_ptr);
        self.decrement_use_count(value);

        self.definitions.clear();
        self.emitted_mirs.push(Mir51::Store {
            dest_ptr: resolved_dest_ptr,
            value: resolved_value,
        });
        Some(())
    }

    fn lift_branch_if(
        &mut self,
        cond: &Operand,
        true_jmp: &BlockId,
        false_jmp: &BlockId,
    ) -> Option<()> {
        let resolved_cond = self.resolve_operand(cond)?;

        self.decrement_use_count(cond);

        self.emitted_mirs.push(Mir51::If {
            cond: resolved_cond,
            true_jmp: *true_jmp,
            false_jmp: *false_jmp,
        });
        Some(())
    }

    fn lift_jump(&mut self, target_block_id: BlockId) -> Option<()> {
        self.emitted_mirs.push(Mir51::Goto(target_block_id));
        Some(())
    }

    fn lift_call(&mut self, target_block_id: BlockId) -> Option<()> {
        self.definitions.clear();
        self.emitted_mirs.push(Mir51::Call {
            jmp: target_block_id,
            args: Vec::new(),
        });
        Some(())
    }

    fn lift_return(&mut self) -> Option<()> {
        self.emitted_mirs.push(Mir51::Return);
        Some(())
    }

    fn resolve_operand(&self, operand: &Operand) -> Option<Expr> {
        let expr = TrackedValue::from_operand(operand)
            .and_then(|tracked_val| self.definitions.get(&tracked_val).cloned())
            .unwrap_or((*operand).try_into().ok()?);
        Some(expr)
    }

    fn aggregate_or_emit(&mut self, dest: &Operand, expr: Expr, assignment: Mir51) {
        if let Some(tracked_val) = TrackedValue::from_operand(dest)
            && let Some(1) = self.use_counts.get(&tracked_val)
        {
            self.definitions.insert(tracked_val, expr);
        } else {
            self.emitted_mirs.push(assignment);
            self.definitions.retain(|_, defined_expr| {
                let mut dependencies = HashSet::new();
                defined_expr.collect_dependencies(&mut dependencies);
                !dependencies.iter().any(|dep| dep.does_overlap(dest))
            });
        }
    }

    // Phase 3: Block Finalization
    fn finalize_block(&mut self, block: &mut BasicBlock) {
        block.irs.clear();
        block.mirs.extend(self.emitted_mirs.clone());

        self.definitions.clear();
        self.use_counts.clear();
        self.emitted_mirs.clear();
    }
}
