use crate::{
    cfg_restorer::{BasicBlock, BlockId, ControlFlowGraph, Phi},
    common::{Addr, AddrSpace, Ir51, Operand},
    ssa::chk_algorithm::{CHKAlgorithm, Dominators},
};
use std::collections::{HashMap, HashSet};

mod chk_algorithm;

pub struct SSABuilder;

// We need this enum in the fill_phis function to correctly determine the two phases of visiting.
// - When we enter we pass the id of the block we are entering so that we can work with it.
// - When we exit the block we need the vector of the new addesses we pushed onto the stack
//   in that specific block because outside that block they are not valid anymore
enum VisitState {
    Enter(BlockId),
    Exit(Vec<Addr>),
}

// Represents variables by their address and the block they are modified in
type VarsBlocks = HashMap<Addr, HashSet<BlockId>>;
type Frontiers = HashMap<BlockId, HashSet<BlockId>>;
type DominatorsTree = HashMap<BlockId, HashSet<BlockId>>;

// Type for counting the last version used for a variable
type VersionCounter = HashMap<Addr, usize>;

// Type for keeping track of the last valid version of a variable in a block
type VersionStack = HashMap<Addr, Vec<usize>>;

impl SSABuilder {
    pub fn build(cfg: &mut ControlFlowGraph) {
        let shared_blocks: Vec<&BasicBlock> = cfg.blocks.values().collect();
        let immidiate_dominators = CHKAlgorithm::execute(&shared_blocks);
        let dominator_frontiers = Self::calculate_frontiers(&immidiate_dominators, &shared_blocks);
        let variables = Self::find_variables(&shared_blocks, &cfg.ir_code);

        Self::place_phis(&mut cfg.blocks, &dominator_frontiers, &variables);
        Self::ssa(cfg, &immidiate_dominators);

        for (i, ir) in cfg.ir_code.iter().enumerate() {
            println!("{i}: {ir:?}")
        }

        println!("{:#?}", cfg.blocks);
        // println!("{idoms:?}");
        // println!("{variables:?}");
        // println!("{dom_frontiers:?}");
    }

    // We need this function to map addresses to their respective SSAVar with the correct version
    // (thanks to the stack) and increase the version number for that variable when we read from it
    fn ssa(cfg: &mut ControlFlowGraph, idoms: &Dominators) {
        // Counts the versions for each variable
        let mut version_counter: VersionCounter = HashMap::new();
        let dom_tree = Self::get_dominators_tree(idoms);

        // This is a stack for each variable represeting the current last version in this block
        let mut stack: VersionStack = HashMap::new();
        let mut to_visit = vec![VisitState::Enter(cfg.start_block)];

        // DFS Visit of the dominators tree
        while let Some(state) = to_visit.pop() {
            match state {
                VisitState::Enter(block_id) => Self::enter_block(
                    block_id,
                    cfg,
                    &mut stack,
                    &mut version_counter,
                    &mut to_visit,
                    &dom_tree,
                ),
                VisitState::Exit(variables) => Self::rollback_stack(&mut stack, variables),
            }
        }
    }

    fn enter_block(
        block_id: usize,
        cfg: &mut ControlFlowGraph,
        stack: &mut VersionStack,
        version_counter: &mut VersionCounter,
        to_visit: &mut Vec<VisitState>,
        dom_tree: &DominatorsTree,
    ) {
        let mut locals_vars: Vec<Addr> = Vec::new();
        let block = cfg
            .blocks
            .get_mut(&block_id)
            .expect("Non existent block for some reason");
        let ir_range = block.ir_range.clone();

        Self::rewrite_phis_with_versions(block, stack, version_counter, &mut locals_vars);
        Self::rewrite_irs_with_versions(
            &mut cfg.ir_code[ir_range],
            stack,
            version_counter,
            &mut locals_vars,
        );
        Self::fill_incomings_in_successors(block_id, cfg, stack);

        // Push for when we exit this block
        to_visit.push(VisitState::Exit(locals_vars));

        // Visit all the children
        if let Some(dominated_blocks) = dom_tree.get(&block_id) {
            for dominated in dominated_blocks.iter() {
                to_visit.push(VisitState::Enter(*dominated));
            }
        }
    }

    fn fill_incomings_in_successors(
        block_id: usize,
        cfg: &mut ControlFlowGraph,
        stack: &VersionStack,
    ) {
        let block = cfg
            .blocks
            .get_mut(&block_id)
            .expect("Non existent block for some reason");

        for suc in block.successors.clone() {
            let phis = &mut cfg
                .blocks
                .get_mut(&suc)
                .expect("Non existent block for some reason")
                .phis;

            for phi in phis.iter_mut() {
                let (Operand::Address(addr, _) | Operand::SSAVar { addr, .. }) = phi.operand else {
                    unreachable!()
                };
                let current_version = Self::get_stack_top_for(stack, addr);
                phi.incoming.insert(block_id, current_version);
            }
        }
    }

    fn rewrite_irs_with_versions(
        irs: &mut [Ir51],
        stack: &mut VersionStack,
        version_counter: &mut VersionCounter,
        locals_vars: &mut Vec<Addr>,
    ) {
        irs.iter_mut().for_each(|ir| {
            *ir = Self::address_to_ssavar(*ir, stack, version_counter, locals_vars);
        });
    }

    fn rewrite_phis_with_versions(
        block: &mut BasicBlock,
        stack: &mut VersionStack,
        version_counter: &mut VersionCounter,
        locals_vars: &mut Vec<Addr>,
    ) {
        for phi in &mut block.phis {
            let op = Self::write_op(&phi.operand, stack, version_counter, locals_vars);
            phi.operand = op;
        }
    }

    fn rollback_stack(stack: &mut VersionStack, variables: Vec<u16>) {
        for var in variables {
            if let Some(stack) = stack.get_mut(&var) {
                stack.pop();
            }
        }
    }

    fn address_to_ssavar(
        ir: Ir51,
        stack: &mut HashMap<Addr, Vec<usize>>,
        version_counter: &mut HashMap<Addr, usize>,
        pushes_to_rollback: &mut Vec<Addr>,
    ) -> Ir51 {
        match ir {
            Ir51::Move { dest, src } => {
                let src = Self::read_op(&src, stack);
                let dest = Self::write_op(&dest, stack, version_counter, pushes_to_rollback);
                Ir51::Move { dest, src }
            }

            Ir51::BinOp { op, dest, lhs, rhs } => {
                let lhs = Self::read_op(&lhs, stack);
                let rhs = Self::read_op(&rhs, stack);
                let dest = Self::write_op(&dest, stack, version_counter, pushes_to_rollback);
                Ir51::BinOp { op, dest, lhs, rhs }
            }

            Ir51::Store { space, ptr, value } => {
                let ptr = Self::read_op(&ptr, stack);
                let value = Self::read_op(&value, stack);
                Ir51::Store { space, ptr, value }
            }

            Ir51::Load { space, dest_ptr } => {
                let dest_ptr =
                    Self::write_op(&dest_ptr, stack, version_counter, pushes_to_rollback);
                Ir51::Load { space, dest_ptr }
            }

            Ir51::BranchIf {
                cond,
                true_jmp,
                false_jmp,
            } => {
                let cond = Self::read_op(&cond, stack);
                Ir51::BranchIf {
                    cond,
                    true_jmp,
                    false_jmp,
                }
            }

            Ir51::Jump(tgt) => Ir51::Jump(tgt),
            Ir51::Call(tgt) => Ir51::Call(tgt),
            Ir51::Return => Ir51::Return,
            Ir51::InvalidIr => unreachable!("Invalid IR touched"),
        }
    }

    // When reading a variable we just take the top value of the stack and use it as the correct
    // version to read
    fn read_op(op: &Operand, stack: &VersionStack) -> Operand {
        if let Operand::Address(id, AddrSpace::iData) = op {
            let version = Self::get_stack_top_for(stack, *id);
            Operand::SSAVar { addr: *id, version }
        } else {
            *op
        }
    }

    // When writing a variable we increment the counter that keeps track of the new versions and
    // push that version on the version stack
    fn write_op(
        op: &Operand,
        stack: &mut VersionStack,
        version_counter: &mut VersionCounter,
        local_vars: &mut Vec<Addr>,
    ) -> Operand {
        if let Operand::Address(addr, AddrSpace::iData) = op {
            let counter = version_counter
                .entry(*addr)
                .and_modify(|ver| *ver += 1)
                .or_insert(1);
            stack.entry(*addr).or_default().push(*counter);

            local_vars.push(*addr);

            Operand::SSAVar {
                addr: *addr,
                version: *counter,
            }
        } else {
            *op
        }
    }

    // Create the dominators tree from the dominators
    fn get_dominators_tree(idoms: &Dominators) -> DominatorsTree {
        let mut dom_tree: DominatorsTree = HashMap::new();

        for (block, &idom) in idoms {
            // Avoid cyclic in the tree
            if *block == idom {
                continue;
            }
            dom_tree.entry(idom).or_default().insert(*block);
        }

        dom_tree
    }

    fn get_stack_top_for(stack: &VersionStack, addr: u16) -> usize {
        stack
            .get(&addr)
            .and_then(|v| v.last())
            .copied()
            .unwrap_or(0)
    }

    fn place_phis(
        blocks: &mut HashMap<BlockId, BasicBlock>,
        frontiers: &Frontiers,
        variables: &VarsBlocks,
    ) {
        for (var, worklist) in variables {
            let mut worklist: Vec<BlockId> = worklist.iter().copied().collect();
            while let Some(block_id) = worklist.pop() {
                let block_frontiers = &frontiers[&block_id];
                for frontier in block_frontiers {
                    let frontier_block = blocks
                        .get_mut(frontier)
                        .expect("No frontier for this block");

                    // TODO: Dont hardcode iData cuz variables could be from different RAMs
                    if frontier_block.phis.iter().any(|e| {
                        matches!(e, Phi { operand: Operand::Address(v, AddrSpace::iData), .. } if *v == *var)
                    }) {
                        continue;
                    }

                    frontier_block.phis.push(Phi {
                        operand: Operand::Address(*var, AddrSpace::iData),
                        incoming: HashMap::new(),
                    });

                    worklist.push(*frontier);
                }
            }
        }
    }

    // TODO: To get a better version of this is to find where a variabile is read in order to avoid
    // placing a phi if a variable is not read in the frontiers
    fn find_variables(blocks: &[&BasicBlock], ir: &[Ir51]) -> VarsBlocks {
        let mut vars: VarsBlocks = HashMap::new();

        for b in blocks {
            for i in b.ir_range.clone() {
                let ir = ir[i];

                match ir {
                    Ir51::Move {
                        dest: Operand::Address(addr, _),
                        ..
                    } => {
                        vars.entry(addr).or_default().insert(b.id);
                    }

                    Ir51::BinOp {
                        dest: Operand::Address(addr, _),
                        ..
                    } => {
                        vars.entry(addr).or_default().insert(b.id);
                    }

                    Ir51::Load {
                        dest_ptr: Operand::Address(addr, _),
                        ..
                    } => {
                        vars.entry(addr).or_default().insert(b.id);
                    }

                    Ir51::BranchIf {
                        cond: Operand::Address(addr, _),
                        ..
                    } => {
                        vars.entry(addr).or_default().insert(b.id);
                    }

                    Ir51::BranchIf { .. } => {}
                    Ir51::Load { .. } => {}
                    Ir51::BinOp { .. } => {}
                    Ir51::Move { .. } => {}
                    Ir51::Store { .. } => {}
                    Ir51::Jump(_) => {}
                    Ir51::Call(_) => {}
                    Ir51::Return => {}
                    Ir51::InvalidIr => unreachable!("Invalid IR touched"),
                }
            }
        }

        vars
    }

    fn calculate_frontiers(idom: &Dominators, blocks: &[&BasicBlock]) -> Frontiers {
        let mut frontiers: Frontiers = HashMap::new();

        for b in blocks {
            frontiers.insert(b.id, HashSet::new());
        }

        for b in blocks {
            if b.predecessors.len() <= 1 {
                continue;
            }

            let dominator = idom[&b.id];
            for &pred in &b.predecessors {
                let mut it = pred;
                while it != dominator {
                    let affected_blocks = frontiers.get_mut(&pred);
                    if let Some(affected_blocks) = affected_blocks {
                        affected_blocks.insert(b.id);
                    }

                    it = *idom
                        .get(&it)
                        .expect("Block is not dominated by anything for some reason")
                }
            }
        }

        frontiers
    }
}
