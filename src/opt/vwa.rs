use std::collections::{HashMap, HashSet};

use crate::{
    common::{BinOp, Constant, Ir51, Operand, TempIndex, VarId},
    opt::cfg_restorer::{BasicBlock, ControlFlowGraph},
};

// Usual bits for a operand in intel8051
const BITS: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Argument {
    Classic(VarId),
    Temporary(TempIndex),
    Constant(Constant),
}

impl Argument {
    pub fn from(op: &Operand) -> Self {
        match op {
            Operand::Var(id) => Self::Classic(*id),
            Operand::Temporary(id) => Self::Temporary(*id),
            Operand::Constant(value) => Self::Constant(*value),

            // TODO: Concat return its high part for now
            Operand::Concat(high, _) => Self::Classic(*high),

            Operand::Register(_)
            | Operand::TempSSAVar { .. }
            | Operand::Address(_, _)
            | Operand::SSAVar { .. }
            | Operand::Concat(_, _) => unreachable!(),
        }
    }

    pub fn to_operand(self) -> Operand {
        match self {
            Argument::Classic(id) => Operand::Var(id),
            Argument::Temporary(idx) => Operand::Temporary(idx),
            Argument::Constant(val) => Operand::Constant(val),
        }
    }

    pub fn concatenate_with(self, rhs: Argument) -> Operand {
        match (self, rhs) {
            (Argument::Classic(lid), Argument::Classic(rid)) => Operand::Concat(lid, rid),
            (Argument::Constant(lval), Argument::Constant(rval)) => {
                Operand::Constant(lval << BITS | rval)
            }

            _ => todo!(),
        }
    }
}

// This struct represents the high and low part
// of the lhs and rhs of a binary operation
#[derive(Default, Debug)]
struct Aggregation {
    lhs_high: Option<Argument>,
    lhs_low: Option<Argument>,
    rhs_high: Option<Argument>,
    rhs_low: Option<Argument>,

    start_index: usize,
    end_index: usize,

    dest: Option<Argument>,
    binop: Option<BinOp>,
}

impl Aggregation {
    pub fn insert(&mut self, left: Argument, right: Argument) {
        if self.lhs_high.is_none() && self.rhs_high.is_none() {
            self.lhs_high = Some(left);
            self.rhs_high = Some(right);
        } else {
            self.lhs_low = Some(left);
            self.rhs_low = Some(right);
        }
    }

    pub fn substitute(&mut self, from: Argument, to: Argument) {
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

    // TODO: Maybe use this for when we have to iterate te fields of a struct
    pub fn is_inside(&self, var: Argument) -> bool {
        [self.lhs_high, self.lhs_low, self.rhs_high, self.rhs_low].contains(&Some(var))
    }

    pub fn is_complete(&self) -> bool {
        [self.lhs_high, self.lhs_low, self.rhs_high, self.rhs_low]
            .iter()
            .all(|field| field.is_some())
            && self.binop.is_some()
    }
}

#[derive(Default)]
pub struct VariableWidthAggregation {
    defs: HashSet<Argument>,
    aggregate: Aggregation,
}

impl VariableWidthAggregation {
    pub fn execute(&mut self, cfg: &mut ControlFlowGraph) {
        let mut modified = true;

        while modified {
            modified = false;

            for block in cfg.blocks.values_mut() {
                modified |= self.intra_block(block);
                modified |= self.inter_block(block);
            }
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

    fn intra_block(&mut self, block: &mut BasicBlock) -> bool {
        self.clear();

        let mut track_mode = false;
        for (i, ir) in block.irs.iter_mut().enumerate().rev() {
            if track_mode {
                self.track(i, ir);
            } else {
                track_mode = self.analyze(i, ir);
            }
        }

        if self.aggregate.is_complete() && self.defs.is_empty() {
            self.fusion(block);
            true
        } else {
            false
        }
    }

    fn fusion(&self, block: &mut BasicBlock) {
        let aggregate = &self.aggregate;

        let from = aggregate.start_index;
        let to = aggregate.end_index + 1;

        let lhs = aggregate
            .lhs_high
            .unwrap()
            .concatenate_with(aggregate.lhs_low.unwrap());
        let rhs = aggregate
            .rhs_high
            .unwrap()
            .concatenate_with(aggregate.rhs_low.unwrap());
        let dest = aggregate.dest.unwrap().to_operand();

        let with = vec![Ir51::BinOp {
            op: aggregate.binop.unwrap(),
            dest,
            lhs,
            rhs,
        }];

        block.irs.splice(from..to, with);
    }

    fn track(&mut self, idx: usize, ir: &Ir51) {
        let aggregate = &mut self.aggregate;
        let defs = &mut self.defs;

        match ir {
            Ir51::Move { dest, src } => {
                let dest = Argument::from(dest);
                let src = Argument::from(src);

                aggregate.substitute(dest, src);
                aggregate.start_index = idx;
            }

            Ir51::BinOp { op, dest, lhs, rhs } if op.is_glue_op() => {
                let dest = Argument::from(dest);
                let lhs = Argument::from(lhs);
                let rhs = Argument::from(rhs);
                if !defs.remove(&dest) {
                    return;
                }

                aggregate.binop = Some(*op);
                aggregate.start_index = idx;
                if !aggregate.is_inside(lhs) {
                    defs.insert(lhs);
                };
                if !aggregate.is_inside(rhs) {
                    defs.insert(rhs);
                };
            }

            Ir51::BinOp { op, dest, lhs, rhs } if op.is_math_op() => {
                let dest = Argument::from(dest);
                let lhs = Argument::from(lhs);
                let rhs = Argument::from(rhs);

                if !defs.remove(&dest) {
                    return;
                }

                aggregate.start_index = idx;
                aggregate.insert(lhs, rhs);
            }

            _ => {}
        }
    }

    fn analyze(&mut self, idx: usize, ir: &Ir51) -> bool {
        let aggregate = &mut self.aggregate;
        let defs = &mut self.defs;

        let mut track_mode = false;
        match ir {
            Ir51::BinOp { op, dest, lhs, rhs } if op.is_glue_op() => {
                let dest = Argument::from(dest);
                let lhs = Argument::from(lhs);
                let rhs = Argument::from(rhs);

                track_mode = true;

                aggregate.dest = Some(dest);
                aggregate.end_index = idx;

                defs.insert(lhs);
                defs.insert(rhs);
            }

            _ => {}
        }

        track_mode
    }

    fn inter_block(&mut self, block: &mut BasicBlock) -> bool {
        false
    }

    fn clear(&mut self) {
        self.aggregate = Aggregation::default();
        self.defs.clear();
    }
}
