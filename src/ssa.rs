use crate::{
    cfg_restorer::{BasicBlock, ControlFlowGraph},
    common::Ir51,
};
use std::collections::{HashMap, HashSet};

pub struct StaticSingleAssignment;

impl StaticSingleAssignment {
    pub fn ssa(cfg: &mut ControlFlowGraph, ir: &mut [Ir51]) {
        let mut blocks: Vec<&mut BasicBlock> = cfg.blocks.values_mut().collect();

        let shared_blocks: Vec<&BasicBlock> = blocks.iter().map(|b| &**b).collect();
        let idoms = StaticSingleAssignment::calculate_idoms(&shared_blocks);
        let dom_frontiers = StaticSingleAssignment::calculate_frontiers(&idoms, &shared_blocks);

        println!("{idoms:?}");
        println!("{dom_frontiers:?}");
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
        StaticSingleAssignment::dfs_post_order(
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
                            target_idom_idx = StaticSingleAssignment::intersect(
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
                StaticSingleAssignment::dfs_post_order(
                    succ_idx, blocks, id_to_idx, visited, post_order,
                );
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
