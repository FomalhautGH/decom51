use crate::{
    cfg_restorer::{BasicBlock, BlockId, ControlFlowGraph, Phi},
    common::{AddrSpace, Ir51, Operand},
};
use std::collections::{HashMap, HashSet};

pub struct SSABuilder;

// TODO: Invece di usare tanti hashset e hashmap che conofondono, usa `type` per rendere
// comprensibile il tipo
impl SSABuilder {
    pub fn build(cfg: &mut ControlFlowGraph) {
        let shared_blocks: Vec<&BasicBlock> = cfg.blocks.values().collect();
        let idoms = Self::calculate_idoms(&shared_blocks);
        let dom_frontiers = Self::calculate_frontiers(&idoms, &shared_blocks);
        let variables = Self::find_variables(&shared_blocks, &cfg.ir_code);

        Self::insert_phis(&mut cfg.blocks, &dom_frontiers, &variables);
        Self::fill_phis(cfg, &idoms, &variables);

        for (i, ir) in cfg.ir_code.iter().enumerate() {
            println!("{i}: {ir:?}")
        }

        // println!("{:#?}", cfg.blocks);
        // println!("{idoms:?}");
        // println!("{variables:?}");
        // println!("{dom_frontiers:?}");
    }

    fn fill_phis(
        cfg: &mut ControlFlowGraph,
        idoms: &HashMap<usize, usize>,
        variables: &HashMap<u16, HashSet<usize>>,
    ) {
        let mut version_counter: HashMap<u16, usize> = HashMap::new();
        let mut dom_tree: HashMap<usize, HashSet<usize>> = HashMap::new();

        for (block, &idom) in idoms {
            if *block == idom {
                continue;
            } // Avoid cyclic in the tree
            dom_tree.entry(idom).or_default().insert(*block);
        }

        let mut stack: HashMap<u16, Vec<usize>> = HashMap::new();
        let mut to_visit = vec![cfg.start_block];
        while let Some(block_index) = to_visit.pop() {
            let block = cfg
                .blocks
                .get_mut(&block_index)
                .expect("Non existent block for some reason");

            // Phis
            for phi in block.phis.iter_mut() {
                let Phi {
                    operand: Operand::Address(var, space),
                    ..
                } = phi
                else {
                    unreachable!("Phi without address operand")
                };

                let counter = version_counter
                    .entry(*var)
                    .and_modify(|counter| *counter += 1)
                    .or_insert(1);

                stack.entry(*var).or_default().push(*counter);
                phi.operand = Operand::SSAVar {
                    id: *var,
                    version: *counter,
                };
            }

            macro_rules! address_to_ssavar {(
                    reads: [ $( $read_var:ident ),* ],
                    writes: [ $( $write_var:ident ),* ],
                    build: $builder:expr
                ) => {{
                    $( let $read_var = Self::read_op($read_var, &stack);)*
                    $( let $write_var = Self::write_op($write_var, &mut stack, &mut version_counter);)*
                    $builder
                }};
            }

            // ------------------------------------
            // Blocks
            let ir_range = block.ir_range.clone();
            for instr in &mut cfg.ir_code[ir_range] {
                let newinstr = match &instr {
                    Ir51::Move { dest, src } => address_to_ssavar! {
                        reads: [src],
                        writes: [dest],
                        build: Ir51::Move { src, dest }
                    },

                    Ir51::BinOp { op, dest, lhs, rhs } => address_to_ssavar! {
                        reads: [lhs, rhs],
                        writes: [dest],
                        build: Ir51::BinOp { op: *op, lhs, rhs, dest }
                    },

                    Ir51::Store { space, ptr, value } => address_to_ssavar! {
                        reads: [ptr, value],
                        writes: [],
                        build: Ir51::Store { space: *space, ptr, value }
                    },

                    Ir51::Load { space, dest_ptr } => address_to_ssavar! {
                        reads: [],
                        writes: [dest_ptr],
                        build: Ir51::Load { space: *space, dest_ptr }
                    },

                    Ir51::BranchIf {
                        cond,
                        true_jmp,
                        false_jmp,
                    } => address_to_ssavar! {
                        reads: [cond],
                        writes: [],
                        build: Ir51::BranchIf { cond, true_jmp: *true_jmp, false_jmp: *false_jmp }
                    },

                    Ir51::Jump(tgt) => Ir51::Jump(*tgt),
                    Ir51::Call(tgt) => Ir51::Call(*tgt),
                    Ir51::Return => Ir51::Return,
                    Ir51::InvalidIr => Ir51::InvalidIr,
                };

                *instr = newinstr;
            }

            if let Some(dominated_blocks) = dom_tree.get(&block_index) {
                to_visit.extend(dominated_blocks);
            }
        }

        println!("{:#?}", cfg.blocks)
    }

    fn read_op(op: &Operand, stack: &HashMap<u16, Vec<usize>>) -> Operand {
        if let Operand::Address(id, AddrSpace::iData) = op {
            let version = stack.get(id).and_then(|v| v.last()).copied().unwrap_or(0);
            Operand::SSAVar { id: *id, version }
        } else {
            *op
        }
    }

    fn write_op(
        op: &Operand,
        stack: &mut HashMap<u16, Vec<usize>>,
        version_counter: &mut HashMap<u16, usize>,
    ) -> Operand {
        if let Operand::Address(id, AddrSpace::iData) = op {
            let counter = version_counter
                .entry(*id)
                .and_modify(|c| *c += 1)
                .or_insert(1);
            stack.entry(*id).or_default().push(*counter);
            Operand::SSAVar {
                id: *id,
                version: *counter,
            }
        } else {
            *op
        }
    }

    fn insert_phis(
        blocks: &mut HashMap<BlockId, BasicBlock>,
        frontiers: &HashMap<usize, HashSet<usize>>,
        variables: &HashMap<u16, HashSet<usize>>,
    ) {
        for (var, worklist) in variables {
            let mut worklist: Vec<usize> = worklist.iter().copied().collect();
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
    fn find_variables(blocks: &[&BasicBlock], ir: &[Ir51]) -> HashMap<u16, HashSet<usize>> {
        let mut vars: HashMap<u16, HashSet<usize>> = HashMap::new();

        for b in blocks {
            for i in b.ir_range.clone() {
                let ir = ir[i];

                // TODO: Non usare _ => {} negli altri match sulle ir, sistemarli
                match ir {
                    Ir51::Move {
                        dest: Operand::Address(addr, _),
                        src,
                    } => {
                        vars.entry(addr).or_default().insert(b.id);
                    }

                    Ir51::BinOp {
                        op,
                        dest: Operand::Address(addr, _),
                        lhs,
                        rhs,
                    } => {
                        vars.entry(addr).or_default().insert(b.id);
                    }

                    Ir51::Load {
                        space,
                        dest_ptr: Operand::Address(addr, _),
                    } => {
                        vars.entry(addr).or_default().insert(b.id);
                    }

                    Ir51::BranchIf {
                        cond: Operand::Address(addr, _),
                        true_jmp,
                        false_jmp,
                    } => {
                        vars.entry(addr).or_default().insert(b.id);
                    }

                    Ir51::BranchIf {
                        cond,
                        true_jmp,
                        false_jmp,
                    } => {}

                    Ir51::Load { space, dest_ptr } => {}
                    Ir51::BinOp { op, dest, lhs, rhs } => {}
                    Ir51::Move { dest, src } => {}
                    Ir51::Store { space, ptr, value } => {}
                    Ir51::Jump(_) => {}
                    Ir51::Call(_) => {}
                    Ir51::Return => {}
                    Ir51::InvalidIr => {}
                }
            }
        }

        vars
    }

    fn calculate_frontiers(
        idom: &HashMap<usize, usize>,
        blocks: &[&BasicBlock],
    ) -> HashMap<usize, HashSet<usize>> {
        let mut frontiers: HashMap<usize, HashSet<usize>> = HashMap::new();

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

    // TODO: Assicurati di capire bene questo algoritmo
    // Cooper-Harvey-Kennedy algorithm for calculating the immediate dominators of the ir blocks
    fn calculate_idoms(blocks: &[&BasicBlock]) -> HashMap<usize, usize> {
        let id_to_idx: HashMap<usize, usize> = blocks
            .iter()
            .enumerate()
            .map(|(idx, b)| (b.id, idx))
            .collect();

        let entry_index = *id_to_idx.get(&0).expect("Entry block with ID 0 not found");

        let mut post_order = Vec::new();
        let mut visited = vec![false; blocks.len()];
        Self::dfs_post_order(
            entry_index,
            blocks,
            &id_to_idx,
            &mut visited,
            &mut post_order,
        );

        let mut post_order_idx = vec![0; blocks.len()];
        for (po_pos, &blk_idx) in post_order.iter().enumerate() {
            post_order_idx[blk_idx] = po_pos;
        }

        let mut rpo = post_order;
        rpo.reverse();
        rpo.retain(|&idx| idx != entry_index);

        let mut idom = vec![None; blocks.len()];
        idom[entry_index] = Some(entry_index);

        let mut changed = true;
        while changed {
            changed = false;

            for &b_idx in &rpo {
                let block = &blocks[b_idx];
                let mut new_idom = None;

                for &p_id in &block.predecessors {
                    if let Some(&p_idx) = id_to_idx.get(&p_id)
                        && idom[p_idx].is_some()
                    {
                        new_idom = Some(p_idx);
                        break;
                    }
                }

                if let Some(mut target_idom_idx) = new_idom {
                    for &p_id in &block.predecessors {
                        if let Some(&p_idx) = id_to_idx.get(&p_id)
                            && p_idx != target_idom_idx
                            && idom[p_idx].is_some()
                        {
                            target_idom_idx = Self::intersect(
                                p_idx,
                                target_idom_idx,
                                &idom,
                                &post_order_idx,
                            );
                        }
                    }

                    if idom[b_idx] != Some(target_idom_idx) {
                        idom[b_idx] = Some(target_idom_idx);
                        changed = true;
                    }
                }
            }
        }

        let mut result = HashMap::new();
        for (blk_idx, dom_opt) in idom.iter().enumerate() {
            if let Some(dom_idx) = dom_opt {
                let blk_id = blocks[blk_idx].id;
                let dom_id = blocks[*dom_idx].id;
                result.insert(blk_id, dom_id);
            }
        }

        result
    }

    fn dfs_post_order(
        idx: usize,
        blocks: &[&BasicBlock],
        id_to_idx: &HashMap<usize, usize>,
        visited: &mut [bool],
        post_order: &mut Vec<usize>,
    ) {
        visited[idx] = true;
        for &succ_id in &blocks[idx].successors {
            if let Some(&succ_idx) = id_to_idx.get(&succ_id)
                && !visited[succ_idx]
            {
                Self::dfs_post_order(succ_idx, blocks, id_to_idx, visited, post_order);
            }
        }
        post_order.push(idx);
    }

    fn intersect(
        mut b1: usize,
        mut b2: usize,
        idom: &[Option<usize>],
        post_order_idx: &[usize],
    ) -> usize {
        while b1 != b2 {
            while post_order_idx[b1] < post_order_idx[b2] {
                b1 = idom[b1].unwrap();
            }
            while post_order_idx[b2] < post_order_idx[b1] {
                b2 = idom[b2].unwrap();
            }
        }
        b1
    }
}
