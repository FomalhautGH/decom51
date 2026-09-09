use crate::{
    common::BlockId,
    opt::{cfg::BasicBlock, structured_cfg::StructuredBlock},
};
use std::collections::HashMap;

pub type Dominators = HashMap<BlockId, BlockId>;

pub struct GraphNode<'a> {
    pub id: &'a BlockId,
    pub successors: &'a Vec<BlockId>,
    pub predecessors: &'a Vec<BlockId>,
}

impl<'a> From<&'a BasicBlock> for GraphNode<'a> {
    fn from(block: &'a BasicBlock) -> Self {
        Self {
            id: &block.id,
            successors: &block.successors,
            predecessors: &block.predecessors,
        }
    }
}

impl<'a> From<&'a StructuredBlock> for GraphNode<'a> {
    fn from(block: &'a StructuredBlock) -> Self {
        Self {
            id: &block.id,
            successors: &block.successors,
            predecessors: &block.predecessors,
        }
    }
}

type NodeEdges<'a> = (&'a BlockId, &'a Vec<BlockId>, &'a Vec<BlockId>);

impl<'a> From<NodeEdges<'a>> for GraphNode<'a> {
    fn from((id, successors, predecessors): NodeEdges<'a>) -> Self {
        Self {
            id,
            successors,
            predecessors,
        }
    }
}

pub struct CHKAlgorithm;

// Cooper-Harvey-Kennedy algorithm for calculating the immediate dominators of the basic blocks
impl CHKAlgorithm {
    pub fn execute(blocks: &[GraphNode], entry_id: BlockId) -> Dominators {
        let mut entry_index = None;
        let id_to_idx: HashMap<BlockId, usize> = blocks
            .iter()
            .enumerate()
            .map(|(idx, b)| {
                if *b.id == entry_id {
                    entry_index = Some(idx);
                }

                (*b.id, idx)
            })
            .collect();

        let entry_index =
            entry_index.expect(&format!("Entry block with ID {entry_id} was not found"));

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

            for &block_idx in &rpo {
                let block = &blocks[block_idx];
                let mut new_idom = None;

                for &p_id in block.predecessors {
                    if let Some(&p_idx) = id_to_idx.get(&p_id)
                        && idom[p_idx].is_some()
                    {
                        new_idom = Some(p_idx);
                        break;
                    }
                }

                if let Some(mut target_idom_idx) = new_idom {
                    for &p_id in block.predecessors {
                        if let Some(&p_idx) = id_to_idx.get(&p_id)
                            && p_idx != target_idom_idx
                            && idom[p_idx].is_some()
                        {
                            target_idom_idx =
                                Self::intersect(p_idx, target_idom_idx, &idom, &post_order_idx);
                        }
                    }

                    if idom[block_idx] != Some(target_idom_idx) {
                        idom[block_idx] = Some(target_idom_idx);
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
                result.insert(*blk_id, *dom_id);
            }
        }

        result
    }

    fn dfs_post_order(
        idx: BlockId,
        blocks: &[GraphNode],
        id_to_idx: &HashMap<BlockId, usize>,
        visited: &mut [bool],
        post_order: &mut Vec<BlockId>,
    ) {
        visited[idx] = true;
        for &succ_id in blocks[idx].successors {
            if let Some(&succ_idx) = id_to_idx.get(&succ_id)
                && !visited[succ_idx]
            {
                Self::dfs_post_order(succ_idx, blocks, id_to_idx, visited, post_order);
            }
        }
        post_order.push(idx);
    }

    fn intersect(
        mut b1: BlockId,
        mut b2: BlockId,
        idom: &[Option<BlockId>],
        post_order_idx: &[BlockId],
    ) -> BlockId {
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
