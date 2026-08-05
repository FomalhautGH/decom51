use std::collections::{HashMap, HashSet};

use crate::{
    common::{BlockId, Ir51, Operand},
    lir::{AddrSpace, BinOp},
    mir::{Expr, Mir51},
    opt::{
        cfg::{BasicBlock, ControlFlowGraph},
        vwa::TrackedValue,
    },
};

type UseCounter = HashMap<TrackedValue, usize>;

#[derive(Default)]
pub struct ExprAggregation {
    definitions: HashMap<TrackedValue, Expr>,
    use_count: UseCounter,
    mir_instructions: Vec<Mir51>,
}

// Phase that aggregates the expressions, we aggregate temporary variables always
// and normal one if they are used just once
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
            self.record_usages(ir);
        }

        for ir in block.irs.iter() {
            self.lift_instruction(ir)
                .expect("Error while mapping ir to mirs");
        }

        self.finalize_block(block);
    }

    fn lift_instruction(&mut self, ir: &Ir51) -> Option<()> {
        match ir {
            Ir51::Move { dest, src } => self.mov(dest, src),
            Ir51::BinOp { op, dest, lhs, rhs } => self.binop(op, dest, lhs, rhs),
            Ir51::Store {
                space,
                dest_ptr,
                value,
            } => self.store(space, dest_ptr, value),
            Ir51::Load {
                space,
                dest,
                src_ptr,
            } => self.load(space, dest, src_ptr),
            Ir51::BranchIf {
                cond,
                true_jmp,
                false_jmp,
            } => self.branchif(cond, true_jmp, false_jmp),
            Ir51::Jump(bid) => self.jump(*bid),
            Ir51::Call(bid) => self.call(*bid),
            Ir51::Return => self.ret(),
            Ir51::Nop => Some(()),
        }
    }

    fn ret(&mut self) -> Option<()> {
        self.mir_instructions.push(Mir51::Return);
        Some(())
    }

    fn call(&mut self, bid: BlockId) -> Option<()> {
        self.definitions.clear();
        self.mir_instructions.push(Mir51::Call {
            jmp: bid,
            args: Vec::new(),
        });
        Some(())
    }

    fn store(&mut self, _space: &AddrSpace, dest_ptr: &Operand, value: &Operand) -> Option<()> {
        let value = self.resolve_operand(value)?;
        let dest_ptr = self.resolve_operand(dest_ptr)?;

        self.definitions.clear();
        self.mir_instructions.push(Mir51::Store { dest_ptr, value });
        Some(())
    }

    fn load(&mut self, _space: &AddrSpace, dest: &Operand, src_ptr: &Operand) -> Option<()> {
        let dest_var: Expr = (*dest).try_into().ok()?;
        let src_ptr = self.resolve_operand(src_ptr)?;

        let src = Expr::Deref(Box::new(src_ptr.clone()));
        let assignment = Mir51::Assign {
            dest: dest_var,
            src,
        };

        self.aggregate_or_emit(dest, src_ptr, assignment);
        Some(())
    }

    fn jump(&mut self, bid: BlockId) -> Option<()> {
        self.mir_instructions.push(Mir51::Goto(bid));
        Some(())
    }

    fn branchif(&mut self, cond: &Operand, true_jmp: &BlockId, false_jmp: &BlockId) -> Option<()> {
        let cond = self.resolve_operand(cond)?;
        self.mir_instructions.push(Mir51::If {
            cond,
            true_jmp: *true_jmp,
            false_jmp: *false_jmp,
        });
        Some(())
    }

    fn mov(&mut self, dest: &Operand, src: &Operand) -> Option<()> {
        let dest_var: Expr = (*dest).try_into().ok()?;
        let src = self.resolve_operand(src)?;

        let assignment = Mir51::Assign {
            dest: dest_var,
            src: src.clone(),
        };

        self.aggregate_or_emit(dest, src, assignment);
        Some(())
    }

    fn binop(&mut self, op: &BinOp, dest: &Operand, lhs: &Operand, rhs: &Operand) -> Option<()> {
        let dest_var: Expr = (*dest).try_into().ok()?;

        let lhs = Box::new(self.resolve_operand(lhs)?);
        let rhs = Box::new(self.resolve_operand(rhs)?);
        let src = match op {
            BinOp::Add => Expr::Add(lhs, rhs),
            BinOp::Sub => Expr::Sub(lhs, rhs),
            BinOp::Mul => Expr::Mul(lhs, rhs),
            BinOp::Div => Expr::Div(lhs, rhs),
            BinOp::Mod => Expr::Mod(lhs, rhs),
            BinOp::And => Expr::And(lhs, rhs),
            BinOp::Or => Expr::Or(lhs, rhs),
            BinOp::Xor => Expr::Xor(lhs, rhs),
            BinOp::Eq => Expr::Eq(lhs, rhs),
            BinOp::Lt => Expr::Lt(lhs, rhs),
            BinOp::Gt => Expr::Gt(lhs, rhs),
            BinOp::Le => Expr::Le(lhs, rhs),
            BinOp::Ge => Expr::Ge(lhs, rhs),
            BinOp::Ne => Expr::Ne(lhs, rhs),
        };

        let assignment = Mir51::Assign {
            dest: dest_var,
            src: src.clone(),
        };

        self.aggregate_or_emit(dest, src, assignment);
        Some(())
    }

    fn record_usages(&mut self, ir: &Ir51) {
        let mut record_use = |op: &Operand| {
            if let Some(tv) = TrackedValue::from_operand(op) {
                self.use_count
                    .entry(tv)
                    .and_modify(|counter| *counter += 1)
                    .or_insert(1);
            }
        };

        match ir {
            Ir51::Move { src, .. } => {
                record_use(src);
            }

            Ir51::BinOp { lhs, rhs, .. } => {
                record_use(lhs);
                record_use(rhs);
            }

            Ir51::Store {
                dest_ptr, value, ..
            } => {
                record_use(dest_ptr);
                record_use(value);
            }

            Ir51::Load { src_ptr, .. } => {
                record_use(src_ptr);
            }

            Ir51::BranchIf { cond, .. } => record_use(cond),
            Ir51::Nop | Ir51::Jump(_) | Ir51::Call(_) | Ir51::Return => {}
        }
    }

    fn finalize_block(&mut self, block: &mut BasicBlock) {
        block.irs.clear();
        block.mirs.extend(self.mir_instructions.clone());

        self.definitions.clear();
        self.use_count.clear();
        self.mir_instructions.clear();
    }

    fn resolve_operand(&self, op: &Operand) -> Option<Expr> {
        let def = TrackedValue::from_operand(op)
            .and_then(|tv| self.definitions.get(&tv).cloned())
            .unwrap_or((*op).try_into().ok()?);
        Some(def)
    }

    fn aggregate_or_emit(&mut self, dest: &Operand, def: Expr, assignment: Mir51) {
        if let Some(tv) = TrackedValue::from_operand(dest)
            && let Some(count) = self.use_count.get(&tv)
            && *count == 1
        {
            self.definitions.insert(tv, def);
        } else {
            self.mir_instructions.push(assignment);
            self.definitions.retain(|_, def| {
                let mut dep = HashSet::new();
                def.collect_dependencies(&mut dep);
                !dep.iter().any(|d| d.does_overlap(dest))
            });
        }
    }
}
