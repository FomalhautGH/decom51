use std::collections::{HashMap, HashSet};

use crate::{
    cfg_restorer::{BasicBlock, BlockId, ControlFlowGraph, Phi},
    common::{Addr, AddrSpace, Ir51, Operand},
    ssa::{Frontiers, VarsBlocks, chk_algorithm::Dominators},
};

type DominatorsTree = HashMap<BlockId, HashSet<BlockId>>;

pub struct Phis;

impl Phis {
    pub fn insert_phis(
        cfg: &mut ControlFlowGraph,
        idoms: &Dominators,
        frontiers: &Frontiers,
        variables: &VarsBlocks,
    ) {
        Self::place_phis(&mut cfg.blocks, frontiers, variables);
        Self::fill_phis(cfg, idoms);
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

    fn address_to_ssavar(
        ir: Ir51,
        mut stack: &mut HashMap<Addr, Vec<usize>>,
        mut version_counter: &mut HashMap<Addr, usize>,
    ) -> Ir51 {
        // Macro to keep the match cleaner
        macro_rules! address_to_ssavar {(
                reads: [ $( $read_var:ident ),* ],
                writes: [ $( $write_var:ident ),* ],
                build: $builder:expr
            ) => {{
                $( let $read_var = Self::read_op(&$read_var, &stack);)*
                $( let $write_var = Self::write_op(&$write_var, &mut stack, &mut version_counter);)*
                $builder
            }};
        }

        match ir {
            Ir51::Move { dest, src } => address_to_ssavar! {
                reads: [src],
                writes: [dest],
                build: Ir51::Move { src, dest }
            },

            Ir51::BinOp { op, dest, lhs, rhs } => address_to_ssavar! {
                reads: [lhs, rhs],
                writes: [dest],
                build: Ir51::BinOp { op, lhs, rhs, dest }
            },

            Ir51::Store { space, ptr, value } => address_to_ssavar! {
                reads: [ptr, value],
                writes: [],
                build: Ir51::Store { space, ptr, value }
            },

            Ir51::Load { space, dest_ptr } => address_to_ssavar! {
                reads: [],
                writes: [dest_ptr],
                build: Ir51::Load { space, dest_ptr }
            },

            Ir51::BranchIf {
                cond,
                true_jmp,
                false_jmp,
            } => address_to_ssavar! {
                reads: [cond],
                writes: [],
                build: Ir51::BranchIf { cond, true_jmp, false_jmp }
            },

            Ir51::Jump(tgt) => Ir51::Jump(tgt),
            Ir51::Call(tgt) => Ir51::Call(tgt),
            Ir51::Return => Ir51::Return,
            Ir51::InvalidIr => unreachable!("Invalid IR touched"),
        }
    }

    fn fill_phis(cfg: &mut ControlFlowGraph, idoms: &Dominators) {
        let mut version_counter: HashMap<Addr, usize> = HashMap::new();
        let dom_tree = Self::get_dominators_tree(idoms);

        let mut stack: HashMap<Addr, Vec<usize>> = HashMap::new();
        let mut to_visit = vec![cfg.start_block];

        while let Some(block_index) = to_visit.pop() {
            let current_block = cfg
                .blocks
                .get_mut(&block_index)
                .expect("Non existent block for some reason");

            let ir_range = current_block.ir_range.clone();

            // Map each address operands to SSAVars
            cfg.ir_code[ir_range].iter_mut().for_each(|ir| {
                *ir = Self::address_to_ssavar(*ir, &mut stack, &mut version_counter)
            });

            if let Some(dominated_blocks) = dom_tree.get(&block_index) {
                to_visit.extend(dominated_blocks);
            }
        }
    }

    // When reading a variable we just take the top value of the stack and use it as the correct
    // version to read
    fn read_op(op: &Operand, stack: &HashMap<Addr, Vec<usize>>) -> Operand {
        if let Operand::Address(id, AddrSpace::iData) = op {
            let version = stack.get(id).and_then(|v| v.last()).copied().unwrap_or(0);
            Operand::SSAVar { addr: *id, version }
        } else {
            *op
        }
    }

    // When writing a variable we increment the counter that keeps track of the new versions and
    // push that version on the version stack
    fn write_op(
        op: &Operand,
        stack: &mut HashMap<Addr, Vec<usize>>,
        version_counter: &mut HashMap<Addr, usize>,
    ) -> Operand {
        if let Operand::Address(id, AddrSpace::iData) = op {
            let counter = version_counter
                .entry(*id)
                .and_modify(|ver| *ver += 1)
                .or_insert(1);
            stack.entry(*id).or_default().push(*counter);
            Operand::SSAVar {
                addr: *id,
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
}
