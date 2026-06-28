use std::collections::{HashMap, HashSet};

use crate::{
    cfg_restorer::{BasicBlock, BlockId, ControlFlowGraph, HashBlocks, Phi},
    common::{Addr, BinOp, Constant, Ir51, Operand, TempIndex, Version},
};

// TODO: Might move these in common.rs or something
type SSAVar = (Addr, Version);
type TempSSAVar = (TempIndex, Version);

enum Either {
    SSAVar(SSAVar),
    TempSSAVar(TempSSAVar),
}

// States in which a SSAVar can be: Unknown (self explainatory), Constant (a constant) and
// Dynamic which means we cannot infer the constant value. We can only go down from Unknown to
// Dynamic but not up, they're also called Top, Constant and Bottom.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
enum SSAState {
    Bottom,
    Constant(Constant),
    Top,
}

// This type represent for every variable and version its SSAState
type VariableValues = HashMap<SSAVar, SSAState>;

// This represent the same thing but for Temporary variables
type TemporaryValues = HashMap<TempSSAVar, SSAState>;

// Represents the blocks we have to visit by the predecessor and the successor
type BlockQueue = Vec<(BlockId, BlockId)>;

// Represents the variables that changed SSAState
type SSAQueue = Vec<Either>;

// TODO: try to use as mush fields in the main stucture also for the other
// passes

// I will use the Sparse conditional constant propagation technique:
// https://en.wikipedia.org/wiki/Sparse_conditional_constant_propagation
#[derive(Default)]
pub struct ConstPropagation {
    known_values: VariableValues,
    temporaries: TemporaryValues,
    block_queue: BlockQueue,
    ssa_queue: SSAQueue,
    reachable_blocks: HashSet<BlockId>,
}

impl ConstPropagation {
    pub fn execute(&mut self, cfg: &mut ControlFlowGraph) {
        self.init_to_unknown(cfg);

        self.block_queue.push((cfg.start_block, cfg.start_block));
        while !self.block_queue.is_empty() || !self.ssa_queue.is_empty() {
            self.propagate_block(cfg);
        }

        if cfg!(debug_assertions) {
            println!("======================== CONST PROP ========================");
            println!("Variables: {:?}", self.known_values);
            println!("Temporaries: {:?}", self.temporaries);
        }
    }

    #[rustfmt::skip]
    fn propagate_block(
        &mut self,
        cfg: &mut ControlFlowGraph
    ) {
        let Some((src, dest)) = self.block_queue.pop() else { return; };
        let (src, dest) = (&cfg.blocks[&src], &cfg.blocks[&dest]);

        self.propagate_phi(&src.phis, src.id);
        if !self.reachable_blocks.insert(dest.id) { return; }
        self.propagate_ir(&dest, &cfg.ir_code, src.id);
    }

    fn propagate_ir(&mut self, dest: &BasicBlock, ir_code: &[Ir51], src_id: BlockId) {
        let ir_range = dest.ir_range.clone();
        let ir_code = &ir_code[ir_range];

        // The variable we assign to beign that this pass its executed just after the SSA
        // the destination variable is always Top (or Unknown)
        for ir in ir_code {
            match ir {
                Ir51::Move { dest, src } => self.mov(dest, src),
                Ir51::BinOp { op, dest, lhs, rhs } => self.binop(*op, dest, lhs, rhs),

                Ir51::Store { space, ptr, value } => todo!(),
                Ir51::Load { space, dest_ptr } => todo!(),

                Ir51::BranchIf {
                    cond,
                    true_jmp,
                    false_jmp,
                } => self.branchif(cond, *true_jmp, *false_jmp, src_id),

                Ir51::Jump(target) => self.block_queue.push((src_id, *target)),

                Ir51::Call(_) | Ir51::Return => {}
                Ir51::InvalidIr => unreachable!("Invalid IR Touched"),
            }
        }
    }

    fn branchif(&mut self, cond: &Operand, true_jmp: BlockId, false_jmp: BlockId, from: BlockId) {
        let cond_state = self.get_operand_state(cond);
        match cond_state {
            SSAState::Top => {}
            SSAState::Constant(0) => self.block_queue.push((from, false_jmp)),
            SSAState::Constant(_) => self.block_queue.push((from, true_jmp)),
            SSAState::Bottom => {
                self.block_queue.push((from, true_jmp));
                self.block_queue.push((from, false_jmp));
            }
        }
    }

    fn mov(&mut self, dest: &Operand, src: &Operand) {
        match (dest, src) {
            (dest, Operand::Costant(v)) => self.dest_to_state(dest, SSAState::Constant(*v)),
            (dest, src) => {
                let src_state = self.get_operand_state(src);
                if let SSAState::Top = src_state {
                    return;
                }
                self.dest_to_state(dest, src_state);
            }
            _ => unreachable!(),
        };
    }

    fn binop(&mut self, op: BinOp, dest: &Operand, lhs: &Operand, rhs: &Operand) {
        match (lhs, rhs) {
            (Operand::Costant(x), Operand::Costant(y)) => {
                self.dest_to_binop_result(op, dest, *x, *y)
            }

            (Operand::Costant(x), rhs) => match self.get_operand_state(rhs) {
                SSAState::Bottom => self.dest_to_state(dest, SSAState::Bottom),
                SSAState::Constant(y) => self.dest_to_binop_result(op, dest, *x, y),
                SSAState::Top => {}
            },

            (lhs, Operand::Costant(y)) => match self.get_operand_state(lhs) {
                SSAState::Bottom => self.dest_to_state(dest, SSAState::Bottom),
                SSAState::Constant(x) => self.dest_to_binop_result(op, dest, x, *y),
                SSAState::Top => {}
            },

            (lhs, rhs) => self.propagate_binop_state(op, dest, lhs, rhs),
            _ => unreachable!(),
        }
    }

    fn propagate_binop_state(&mut self, op: BinOp, dest: &Operand, lhs: &Operand, rhs: &Operand) {
        let lhs_state = self.get_operand_state(lhs);
        let rhs_state = self.get_operand_state(rhs);

        match (lhs_state, rhs_state) {
            (SSAState::Top, _) | (_, SSAState::Top) => {}
            (SSAState::Bottom, _) | (_, SSAState::Bottom) => {
                self.dest_to_state(dest, SSAState::Bottom)
            }
            (SSAState::Constant(x), SSAState::Constant(y)) => {
                self.dest_to_binop_result(op, dest, x, y)
            }
        }
    }

    fn dest_to_binop_result(&mut self, op: BinOp, dest: &Operand, x: Constant, y: Constant) {
        let result = match op {
            BinOp::BorrowOut => todo!(),
            BinOp::Add => x.overflowing_add(y),
            BinOp::Sub => x.overflowing_sub(y),
            BinOp::Eq => todo!(),
            BinOp::Lt => todo!(),
            BinOp::Gt => todo!(),
            BinOp::Le => todo!(),
            BinOp::Ge => todo!(),
            BinOp::Ne => todo!(),
        }
        .0;

        self.dest_to_state(dest, SSAState::Constant(result));
    }

    fn dest_to_state(&mut self, dest: &Operand, state: SSAState) {
        let dest = Self::operand_to_either(dest);

        let dest_state = match dest {
            Either::SSAVar(key) => self.known_values.get_mut(&key).unwrap(),
            Either::TempSSAVar(key) => self.temporaries.get_mut(&key).unwrap(),
        };

        *dest_state = state;
        self.ssa_queue.push(dest);
    }

    fn get_operand_state(&mut self, operand: &Operand) -> SSAState {
        let operand = Self::operand_to_either(operand);
        *match operand {
            Either::SSAVar(key) => self.known_values.get(&key).unwrap(),
            Either::TempSSAVar(key) => self.temporaries.get(&key).unwrap(),
        }
    }

    // TODO: Maybe to this mapping with an .into()
    fn operand_to_either(op: &Operand) -> Either {
        match op {
            Operand::TempSSAVar { index, version } => Either::TempSSAVar((*index, *version)),
            Operand::SSAVar { addr, version } => Either::SSAVar((*addr, *version)),
            _ => unreachable!(),
        }
    }

    // ===================================== INIT Functions =====================================
    #[rustfmt::skip]
    fn propagate_phi(
        &mut self,
        phis: &[Phi],
        src_id: BlockId,
    ) {
        for phi in phis {
            let Operand::SSAVar {
                addr,
                version: actual_version,
            } = phi.operand
            else { unreachable!() };

            let Some(incoming_version) = phi.incoming.get(&src_id) else { continue; };

            let val = self.known_values[&(addr, actual_version)];
            let arg = self.known_values[&(addr, *incoming_version)];

            let result_state = match (val, arg) {
                (SSAState::Constant(x), SSAState::Constant(y)) => {
                    if x == y {
                        SSAState::Constant(x)
                    } else {
                        SSAState::Bottom
                    }
                }

                (SSAState::Top, SSAState::Constant(v)) => SSAState::Constant(v),
                (SSAState::Constant(v), SSAState::Top) => SSAState::Constant(v),

                (SSAState::Top, SSAState::Bottom)
                | (SSAState::Bottom, SSAState::Top)
                | (SSAState::Bottom, SSAState::Bottom)
                | (SSAState::Bottom, SSAState::Constant(_))
                | (SSAState::Constant(_), SSAState::Bottom) => SSAState::Bottom,

                (SSAState::Top, SSAState::Top) => SSAState::Top,
            };

            if result_state < val {
                self.ssa_queue.push(Either::SSAVar((addr, actual_version)));
            }
        }
    }

    fn init_to_unknown(&mut self, cfg: &ControlFlowGraph) {
        for ir in &cfg.ir_code {
            self.set_ir_unknown(ir);
        }
    }

    fn set_ir_unknown(&mut self, ir: &Ir51) {
        match ir {
            Ir51::Move { dest, src } => {
                self.operand_to_unknown(dest);
                self.operand_to_unknown(src);
            }

            Ir51::BinOp { dest, lhs, rhs, .. } => {
                self.operand_to_unknown(dest);
                self.operand_to_unknown(lhs);
                self.operand_to_unknown(rhs);
            }

            Ir51::Store { ptr, value, .. } => {
                self.operand_to_unknown(ptr);
                self.operand_to_unknown(value);
            }

            Ir51::Load { dest_ptr, .. } => {
                self.operand_to_unknown(dest_ptr);
            }

            Ir51::BranchIf { cond, .. } => {
                self.operand_to_unknown(cond);
            }

            Ir51::Call(_) | Ir51::Return | Ir51::Jump(_) => {}
            Ir51::InvalidIr => unreachable!("Invalid IR touched"),
        }
    }

    fn operand_to_unknown(&mut self, operand: &Operand) {
        match operand {
            Operand::SSAVar { addr, version } => {
                self.known_values.insert((*addr, *version), SSAState::Top)
            }
            Operand::TempSSAVar { index, version } => {
                self.temporaries.insert((*index, *version), SSAState::Top)
            }

            // TODO: Make these unreachable since we shouldn't get here, we have to implement a DCE pass just after the CFG creation
            Operand::Address(_, _) | Operand::Temporary(_) => None,

            // We work on constant later
            Operand::Costant(_) => None,
            op @ Operand::Register(_) => unreachable!("{op:?}"),
        };
    }
}
