use std::collections::HashMap;

use crate::{
    common::{BlockId, Ir51, Operand},
    opt::cfg::{BasicBlock, ControlFlowGraph},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
struct CriticalEdge {
    from: BlockId,
    to: BlockId,
}

// Maps a predecessor block ID to the list of (dest, src) copy operations required for phi lowering.
type PredecessorCopies = HashMap<BlockId, Vec<(Operand, Operand)>>;
type SplitEdgeBlocks = HashMap<CriticalEdge, BlockId>;

pub struct PhiElimination {
    next_block_id: BlockId,
    split_edge_blocks: SplitEdgeBlocks,
}

impl PhiElimination {
    pub fn new(cfg: &ControlFlowGraph) -> Self {
        Self {
            next_block_id: cfg.blocks.keys().max().unwrap_or(&0) + 1,
            split_edge_blocks: HashMap::new(),
        }
    }

    pub fn eliminate(&mut self, cfg: &mut ControlFlowGraph) {
        for block in cfg.blocks.clone().values() {
            self.lower_block_phis(block, cfg);
        }

        // Redirect CFG edges through newly created split blocks for critical edges
        for (edge, split_block_id) in &self.split_edge_blocks {
            let [pred_block, succ_block] = cfg.blocks.get_disjoint_mut([&edge.from, &edge.to]);
            let [pred_block, succ_block] = [pred_block.unwrap(), succ_block.unwrap()];
            Self::splice_split_block(pred_block, *split_block_id, succ_block);
        }

        // Clean up all phi nodes across all basic blocks
        for block in cfg.blocks.values_mut() {
            block.phis.clear();
        }

        // TODO: Move this somewhere else
        cfg.remove_useless_irs();

        if cfg!(debug_assertions) {
            println!("======================== PHI ELIMINATION ========================");
            let mut sorted_blocks: Vec<&BasicBlock> = cfg.blocks.values().collect();
            sorted_blocks.sort_by_key(|b| b.id);
            for block in sorted_blocks {
                print!("{}", block);
            }
        }
    }

    fn lower_block_phis(&mut self, block: &BasicBlock, cfg: &mut ControlFlowGraph) {
        let (mut raw_copies, mut final_copies) = Self::group_phis_by_predecessor(block);
        Self::resolve_parallel_copies(&mut raw_copies, &mut final_copies);
        self.place_moves(cfg, final_copies, block);
    }

    fn group_phis_by_predecessor(block: &BasicBlock) -> (PredecessorCopies, PredecessorCopies) {
        let mut raw_copies: PredecessorCopies = HashMap::new();
        let mut final_copies: PredecessorCopies = HashMap::new();

        for phi in &block.phis {
            for pred_id in phi.incoming.keys() {
                raw_copies.insert(*pred_id, Vec::new());
                final_copies.insert(*pred_id, Vec::new());
            }
        }

        for phi in &block.phis {
            for (pred_id, incoming_operand) in &phi.incoming {
                raw_copies
                    .entry(*pred_id)
                    .or_default()
                    .push((phi.operand, *incoming_operand));
            }
        }

        (raw_copies, final_copies)
    }

    // Resolves parallel copy conflicts / cycles using temporary variables
    fn resolve_parallel_copies(
        raw_copies: &mut PredecessorCopies,
        final_copies: &mut PredecessorCopies,
    ) {
        for (pred_id, copies) in raw_copies.iter_mut() {
            let mut temp_id = 0;
            let mut needs_temp = false;

            while let Some((dest, src)) = copies.pop() {
                let temp = Operand::Temp(temp_id);

                for (_, pending_src) in &mut *copies {
                    if dest == *pending_src {
                        *pending_src = temp;
                        needs_temp = true;
                    }
                }

                if needs_temp {
                    final_copies
                        .entry(*pred_id)
                        .or_default()
                        .push((temp, dest));
                    temp_id += 1;
                }

                final_copies
                    .entry(*pred_id)
                    .or_default()
                    .push((dest, src));

                needs_temp = false;
            }
        }
    }

    fn place_moves(
        &mut self,
        cfg: &mut ControlFlowGraph,
        final_copies: PredecessorCopies,
        target_block: &BasicBlock,
    ) {
        for (pred_id, assignments) in final_copies {
            if pred_id == target_block.id {
                continue;
            }

            let [pred_block, succ_block] =
                cfg.blocks.get_disjoint_mut([&pred_id, &target_block.id]);
            let [pred_block, succ_block] = [pred_block.unwrap(), succ_block.unwrap()];

            let (destination_block, at_entry) = if pred_block.successors.len() > 1 {
                if target_block.predecessors.len() > 1 {
                    let split_block_id =
                        self.split_critical_edge(pred_block.id, target_block.id, cfg);
                    let split_block = cfg.blocks.get_mut(&split_block_id).unwrap();
                    (split_block, false)
                } else {
                    (succ_block, true)
                }
            } else {
                (pred_block, false)
            };

            for (dest, src) in assignments {
                Self::insert_move(destination_block, dest, src, at_entry);
            }
        }
    }

    // Inserts a move instruction in a block, either at entry or before the block terminator
    fn insert_move(block: &mut BasicBlock, dest: Operand, src: Operand, at_entry: bool) {
        let irs = &mut block.irs;
        let mov = Ir51::Move { dest, src };

        if at_entry {
            irs.insert(0, mov);
        } else {
            let irs_len = irs.len();
            let last_ir = irs.last().unwrap_or(&Ir51::Nop);

            let mut offset_from_end = 0;
            match last_ir {
                Ir51::Jump(_) |
                Ir51::BranchIf { .. } => offset_from_end = 1,

                Ir51::Call(_) | // TODO: Technically a jump when we support it
                Ir51::Move { .. } |
                Ir51::BinOp { .. } |
                Ir51::Store { .. } |
                Ir51::Load { .. } |
                Ir51::Return |
                Ir51::Nop => {}
            }

            irs.insert(irs_len - offset_from_end, mov);
        }
    }

    // Inserts a split block along a critical edge if one does not already exist, returning its ID
    fn split_critical_edge(
        &mut self,
        from_id: BlockId,
        to_id: BlockId,
        cfg: &mut ControlFlowGraph,
    ) -> BlockId {
        let edge = CriticalEdge {
            from: from_id,
            to: to_id,
        };

        *self.split_edge_blocks.entry(edge).or_insert_with(|| {
            let mut split_block = BasicBlock::new(self.next_block_id);
            split_block.irs.push(Ir51::Jump(to_id));

            split_block.predecessors.push(from_id);
            split_block.successors.push(to_id);

            cfg.blocks.insert(split_block.id, split_block);

            self.next_block_id += 1;
            self.next_block_id - 1
        })
    }

    fn splice_split_block(
        pred_block: &mut BasicBlock,
        split_block_id: BlockId,
        succ_block: &mut BasicBlock,
    ) {
        // If a block has more than one successor, its terminator must be a BranchIf
        match pred_block
            .irs
            .last_mut()
            .expect("Terminator instruction expected in block with multiple successors")
        {
            Ir51::BranchIf {
                true_jmp,
                false_jmp,
                ..
            } => {
                let is_true = *true_jmp == succ_block.id;
                let is_false = *false_jmp == succ_block.id;

                if is_true {
                    *true_jmp = split_block_id;
                } else if is_false {
                    *false_jmp = split_block_id;
                }

                if !is_true && !is_false {
                    unreachable!("Branch destination must match successor block");
                }
            }
            _ => unreachable!(
                "Only BranchIf can be the terminator instruction of a critical edge source"
            ),
        }

        for succ in &mut pred_block.successors {
            if *succ == succ_block.id {
                *succ = split_block_id;
            }
        }

        for pred in &mut succ_block.predecessors {
            if *pred == pred_block.id {
                *pred = split_block_id;
            }
        }
    }
}
