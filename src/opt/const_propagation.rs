use std::{
    collections::{HashMap, HashSet},
    fmt::{Display, Formatter, Result},
};

use crate::{
    common::{Addr, BinOp, BlockId, Constant, Ir51, Operand, Phi, SSAVar, TempSSAVar},
    lir::format_addr,
    opt::cfg::{BasicBlock, ControlFlowGraph},
};

#[derive(Debug, PartialEq, Eq, Hash)]
enum Either {
    SSAVar(SSAVar),
    TempSSAVar(TempSSAVar),
}

impl Display for Either {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Either::SSAVar((addr, version, ..)) => write!(f, "{}_{}", format_addr(*addr), version),
            Either::TempSSAVar((idx, version)) => write!(f, "t{}_{}", idx, version),
        }
    }
}

// States in which a SSAVar can be: Unknown (self explainatory), Constant (a constant) and
// Dynamic which means we cannot infer the constant value. We can only go down from Unknown to
// Dynamic but not up, they're also called Top, Constant and Bottom.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub enum SSAState {
    Bottom,
    Constant(Constant),
    Top,
}

// This type represent for every variable and version its SSAState
pub type VariableStates = HashMap<SSAVar, SSAState>;

// This represent the same thing but for Temporary variables
pub type TemporaryStates = HashMap<TempSSAVar, SSAState>;

// Represents the blocks we have to visit by the predecessor and the successor
type BlockQueue = Vec<(BlockId, BlockId)>;

// Represents the variables that changed SSAState
type SSAQueue = Vec<Either>;

// This is the def-use chain, represents for every SSAVar or TempSSAVar every place that
// it is used (thanks to the blockid and the specific ir index) in so that we can update
// it when the state for one of them changes
type DefUseChain = HashMap<Either, Vec<(BlockId, UseLocation)>>;

// We need this to differentiate when a use is referred to the phis or the classic instructions
#[derive(Debug, Clone, Copy)]
enum UseLocation {
    Instruction(usize),
    Phi(usize),
}

// TODO: Since intel8051 is a small architecture we could make this better by also tracking
// the RAMs states by avoiding to use Bottom when Loading from memory (in particular __idata)

// TODO: try to use as mush fields in the main stucture also for the other
// passes

// I will use the Sparse conditional constant propagation technique:
// https://en.wikipedia.org/wiki/Sparse_conditional_constant_propagation
#[derive(Default)]
pub struct ConstPropagation {
    pub known_values: VariableStates,
    pub known_temps: TemporaryStates,
    block_queue: BlockQueue,
    ssa_queue: SSAQueue,
    defuse_chain: DefUseChain,
    reachable_blocks: HashSet<BlockId>,
}

impl ConstPropagation {
    pub fn execute(&mut self, cfg: &ControlFlowGraph) {
        self.init_to_top(cfg);
        self.init_defusechain(cfg);

        if cfg!(debug_assertions) {
            println!("======================== CONST PROP ========================");
            println!("Def-Use Chain:");
            let mut sorted_chain: Vec<(&Either, &Vec<(BlockId, UseLocation)>)> =
                self.defuse_chain.iter().collect();
            sorted_chain.sort_by_key(|(k, _)| k.to_string());
            for (key, uses) in sorted_chain {
                println!("  {} is used in: {:?}", key, uses);
            }
        }

        // Start by inserting the first acr to analyze
        self.block_queue.push((cfg.start_block, cfg.start_block));

        while !self.block_queue.is_empty() || !self.ssa_queue.is_empty() {
            self.propagate_block(cfg); // Phase 1
            self.elaborate_ssavar(cfg); // Phase 2
        }

        if cfg!(debug_assertions) {
            println!("Variables:");
            let mut sorted_vars: Vec<(&SSAVar, &SSAState)> = self.known_values.iter().collect();
            sorted_vars.sort_by_key(|(var, _)| format!("{}_{}", format_addr(var.0), var.1));
            for (var, state) in sorted_vars {
                println!("  {}_{}: {:?}", format_addr(var.0), var.1, state);
            }

            println!("Temporaries:");
            let mut sorted_temps: Vec<(&TempSSAVar, &SSAState)> = self.known_temps.iter().collect();
            sorted_temps.sort_by_key(|t| (t.0.0, t.0.1));
            for (temp, state) in sorted_temps {
                println!("  t{}_{}: {:?}", temp.0, temp.1, state);
            }
        }
    }

    fn init_defusechain(&mut self, cfg: &ControlFlowGraph) {
        for block in cfg.blocks.values() {
            for (i, phi) in block.phis.iter().enumerate() {
                self.insert_use_for_phi(phi, i, block.id);
            }

            for (i, ir) in block.irs.iter().enumerate() {
                self.insert_use_for_ir(ir, i, block.id);
            }
        }
    }

    fn insert_use_for_phi(&mut self, phi: &Phi, phi_index: usize, block_id: BlockId) {
        for incoming in phi.incoming.values() {
            let Operand::SSAVar { addr, size, .. } = phi.operand else {
                unreachable!();
            };

            let operand = Operand::SSAVar {
                addr,
                version: incoming.extract_ssa_version(),
                size,
            };

            let operand = Self::operand_to_either(&operand);
            self.defuse_chain
                .entry(operand)
                .or_default()
                .push((block_id, UseLocation::Phi(phi_index)));
        }
    }

    fn insert_use_for_ir(&mut self, ir: &Ir51, ir_index: usize, block_id: BlockId) {
        let mut insert = |operand: &Operand| {
            if let Operand::Constant(_) = operand {
                return;
            }

            let src = Self::operand_to_either(operand);
            self.defuse_chain
                .entry(src)
                .or_default()
                .push((block_id, UseLocation::Instruction(ir_index)));
        };

        match ir {
            Ir51::Move { src, .. } => insert(src),

            Ir51::BinOp { lhs, rhs, .. } => {
                insert(lhs);
                insert(rhs);
            }

            Ir51::Store {
                dest_ptr, value, ..
            } => {
                insert(dest_ptr);
                insert(value);
            }

            Ir51::Load { src_ptr, .. } => insert(src_ptr),
            Ir51::BranchIf { cond, .. } => insert(cond),

            Ir51::Nop | Ir51::Jump(_) | Ir51::Call(_) | Ir51::Return => {}
        }
    }

    fn elaborate_ssavar(&mut self, cfg: &ControlFlowGraph) -> Option<()> {
        let ssavar = self.ssa_queue.pop()?;
        let var_uses = self.defuse_chain.get(&ssavar)?.clone();

        for (block_id, var_use) in var_uses {
            if !self.reachable_blocks.contains(&block_id) {
                continue;
            }

            match var_use {
                UseLocation::Instruction(index) => {
                    let block = &cfg.blocks[&block_id];
                    self.elaborate_ir(&block.irs, index, block_id)
                }
                UseLocation::Phi(index) => self.elaborate_phi(&cfg.blocks[&block_id], index),
            }
        }

        Some(())
    }

    fn elaborate_phi(&mut self, block: &BasicBlock, index: usize) {
        let phi = &block.phis[index];
        self.phi(phi);
    }

    fn elaborate_ir(&mut self, ir_code: &[Ir51], index: usize, block_id: BlockId) {
        let instruction = &ir_code[index];
        match instruction {
            Ir51::Move { dest, src } => self.mov(dest, src),
            Ir51::BinOp { op, dest, lhs, rhs } => self.binop(*op, dest, lhs, rhs),
            Ir51::Load { dest, .. } => self.dest_to_state(dest, SSAState::Bottom),
            Ir51::BranchIf {
                cond,
                true_jmp,
                false_jmp,
            } => self.branchif(cond, *true_jmp, *false_jmp, block_id),

            Ir51::Store { .. } => {}
            Ir51::Nop | Ir51::Jump(_) | Ir51::Call(_) | Ir51::Return => {}
        }
    }

    // TODO: from isn't actually used anywhere afaik so remove it
    fn propagate_block(&mut self, cfg: &ControlFlowGraph) -> Option<()> {
        let (from, to) = self.block_queue.pop()?;
        let (_, to) = (&cfg.blocks[&from], &cfg.blocks[&to]);

        // We propagate the phis every time we enter a block (not the phis from the block
        // we come from) and if we already seen the 'to' block we don't re-propagate the ir
        self.propagate_phi(&to.phis);
        self.reachable_blocks.insert(to.id).then_some(())?;
        self.propagate_ir(to);

        Some(())
    }

    fn propagate_ir(&mut self, to: &BasicBlock) {
        let mut is_last_jump = false;

        // The variable we assign to beign that this pass its executed just after the SSA
        // the destination variable is always Top (or Unknown)
        for ir in &to.irs {
            match ir {
                Ir51::Move { dest, src } => {
                    self.mov(dest, src);
                    is_last_jump = false;
                }

                Ir51::BinOp { op, dest, lhs, rhs } => {
                    self.binop(*op, dest, lhs, rhs);
                    is_last_jump = false;
                }

                Ir51::Store { .. } => {}
                Ir51::Load { dest, .. } => self.dest_to_state(dest, SSAState::Bottom),

                Ir51::BranchIf {
                    cond,
                    true_jmp,
                    false_jmp,
                } => {
                    self.branchif(cond, *true_jmp, *false_jmp, to.id);
                    is_last_jump = true;
                }

                Ir51::Jump(target) => {
                    self.block_queue.push((to.id, *target));
                    is_last_jump = true;
                }

                Ir51::Nop | Ir51::Call(_) | Ir51::Return => is_last_jump = false,
            }
        }

        // If we didn't found any jump at the end of the block it means
        // we have to push the only successor of this block
        if !is_last_jump {
            let [successor] = to.successors[..] else {
                return;
            };
            self.block_queue.push((to.id, successor));
        }
    }

    fn branchif(&mut self, cond: &Operand, true_jmp: BlockId, false_jmp: BlockId, to_id: BlockId) {
        let cond_state = self.get_operand_state(cond);
        match cond_state {
            SSAState::Top => {}
            SSAState::Constant(0) => self.block_queue.push((to_id, false_jmp)),
            SSAState::Constant(_) => self.block_queue.push((to_id, true_jmp)),
            SSAState::Bottom => {
                self.block_queue.push((to_id, true_jmp));
                self.block_queue.push((to_id, false_jmp));
            }
        }
    }

    fn mov(&mut self, dest: &Operand, src: &Operand) {
        match (dest, src) {
            (dest, Operand::Constant(v)) => self.dest_to_state(dest, SSAState::Constant(*v)),
            (dest, src) => {
                let src_state = self.get_operand_state(src);
                if let SSAState::Top = src_state {
                    return;
                }
                self.dest_to_state(dest, src_state);
            }
        };
    }

    fn binop(&mut self, op: BinOp, dest: &Operand, lhs: &Operand, rhs: &Operand) {
        match (lhs, rhs) {
            (Operand::Constant(x), Operand::Constant(y)) => {
                self.dest_to_binop_result(op, dest, *x, *y)
            }

            (Operand::Constant(x), rhs) => match self.get_operand_state(rhs) {
                SSAState::Bottom => self.dest_to_state(dest, SSAState::Bottom),
                SSAState::Constant(y) => self.dest_to_binop_result(op, dest, *x, y),
                SSAState::Top => {}
            },

            (lhs, Operand::Constant(y)) => match self.get_operand_state(lhs) {
                SSAState::Bottom => self.dest_to_state(dest, SSAState::Bottom),
                SSAState::Constant(x) => self.dest_to_binop_result(op, dest, x, *y),
                SSAState::Top => {}
            },

            (lhs, rhs) => self.propagate_binop_state(op, dest, lhs, rhs),
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

    fn dest_to_binop_result(&mut self, op: BinOp, dest: &Operand, lhs: Constant, rhs: Constant) {
        let (result, shouldnt_set) = match op {
            BinOp::Lt => ((lhs < rhs) as u16, false),
            BinOp::Eq => ((lhs == rhs) as u16, false),
            BinOp::Add | BinOp::Addc => lhs.overflowing_add(rhs),
            BinOp::Sub | BinOp::Subb => lhs.overflowing_sub(rhs),
            BinOp::Mul => lhs.overflowing_mul(rhs),
            BinOp::Div => {
                if let Some(result) = lhs.checked_div(rhs) {
                    (result, false)
                } else {
                    (0, true)
                }
            }
            BinOp::Mod => {
                if let Some(result) = lhs.checked_rem(rhs) {
                    (result, false)
                } else {
                    (0, true)
                }
            }
            BinOp::And => (lhs & rhs, false),
            BinOp::Or => (lhs | rhs, false),
            BinOp::Xor => (lhs ^ rhs, false),
            BinOp::Gt => ((lhs > rhs) as u16, false),
            BinOp::Le => ((lhs <= rhs) as u16, false),
            BinOp::Ge => ((lhs >= rhs) as u16, false),
            BinOp::Ne => ((lhs != rhs) as u16, false),
        };

        if !shouldnt_set {
            self.dest_to_state(dest, SSAState::Constant(result));
        }
    }

    fn dest_to_state(&mut self, dest: &Operand, state: SSAState) {
        let dest = Self::operand_to_either(dest);

        let dest_state = match dest {
            Either::SSAVar(key) => self.known_values.get_mut(&key).unwrap(),
            Either::TempSSAVar(key) => self.known_temps.get_mut(&key).unwrap(),
        };

        if *dest_state != state {
            *dest_state = state;
            self.ssa_queue.push(dest);
        }
    }

    fn get_operand_state(&mut self, operand: &Operand) -> SSAState {
        let operand = Self::operand_to_either(operand);
        *match operand {
            Either::SSAVar(key) => self.known_values.get(&key).unwrap(),
            Either::TempSSAVar(key) => self.known_temps.get(&key).unwrap(),
        }
    }

    // TODO: Maybe to this mapping with an .into()
    fn operand_to_either(op: &Operand) -> Either {
        match op {
            Operand::SSAVar {
                addr,
                version,
                size,
            } => Either::SSAVar((*addr, *version, *size)),
            Operand::TempSSAVar { index, version } => Either::TempSSAVar((*index, *version)),
            _ => unreachable!("{op:?}"),
        }
    }

    fn propagate_phi(&mut self, to_phis: &[Phi]) {
        to_phis.iter().for_each(|phi| self.phi(phi));
    }

    fn phi(&mut self, phi: &Phi) {
        // Phis should always be an SSAVar
        let Operand::SSAVar {
            addr,
            version: actual_version,
            size,
        } = phi.operand
        else {
            unreachable!()
        };

        for (pred_block_id, incoming) in &phi.incoming {
            if self.reachable_blocks.contains(pred_block_id) {
                self.delta_phi(addr, actual_version, incoming.extract_ssa_version(), size);
            }
        }
    }

    fn delta_phi(
        &mut self,
        addr: Addr,
        actual_version: usize,
        incoming_version: usize,
        size: usize,
    ) {
        if actual_version == incoming_version {
            return;
        }

        let actual_key = &(addr, actual_version, size);
        let incoming_key = &(addr, incoming_version, size);

        let [Some(val), Some(arg)] = self
            .known_values
            .get_disjoint_mut([actual_key, incoming_key])
        else {
            unreachable!("");
        };

        let result_state = match (*val, *arg) {
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

        if &result_state != val {
            *val = result_state;
            self.ssa_queue
                .push(Either::SSAVar((addr, actual_version, size)));
        }
    }

    // ===================================== INIT Functions =====================================
    fn init_to_top(&mut self, cfg: &ControlFlowGraph) {
        for block in cfg.blocks.values() {
            block.phis.iter().for_each(|phi| self.set_phi_top(phi));
            block.irs.iter().for_each(|ir| self.set_ir_top(ir));
        }
    }

    fn set_phi_top(&mut self, phi: &Phi) {
        let Operand::SSAVar { addr, size, .. } = phi.operand else {
            unreachable!()
        };

        self.operand_to_top(&phi.operand);

        for incoming in phi.incoming.values() {
            let operand = Operand::SSAVar {
                addr,
                version: incoming.extract_ssa_version(),
                size,
            };
            self.operand_to_top(&operand);
        }
    }

    fn set_ir_top(&mut self, ir: &Ir51) {
        match ir {
            Ir51::Move { dest, src } => {
                self.operand_to_top(dest);
                self.operand_to_top(src);
            }

            Ir51::BinOp { dest, lhs, rhs, .. } => {
                self.operand_to_top(dest);
                self.operand_to_top(lhs);
                self.operand_to_top(rhs);
            }

            Ir51::Store {
                dest_ptr: ptr,
                value,
                ..
            } => {
                self.operand_to_top(ptr);
                self.operand_to_top(value);
            }

            Ir51::Load { dest: dest_ptr, .. } => {
                self.operand_to_top(dest_ptr);
            }

            Ir51::BranchIf { cond, .. } => {
                self.operand_to_top(cond);
            }

            Ir51::Nop | Ir51::Call(_) | Ir51::Return | Ir51::Jump(_) => {}
        }
    }

    fn operand_to_top(&mut self, operand: &Operand) {
        match operand {
            Operand::SSAVar {
                addr,
                version,
                size,
            } => {
                let initial_state = if *version == 0 {
                    SSAState::Bottom
                } else {
                    SSAState::Top
                };
                self.known_values
                    .insert((*addr, *version, *size), initial_state)
            }

            Operand::TempSSAVar { index, version } => {
                let initial_state = if *version == 0 {
                    SSAState::Bottom
                } else {
                    SSAState::Top
                };
                self.known_temps.insert((*index, *version), initial_state)
            }

            // We work on constant later
            Operand::Constant(_) => None,
            Operand::Var { .. }
            | Operand::Temp(_)
            | Operand::Register(_)
            | Operand::Local { .. } => {
                unreachable!("{operand:?}")
            }
        };
    }
}
