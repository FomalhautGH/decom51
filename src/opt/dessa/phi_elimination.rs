use std::collections::HashMap;

use crate::{
    common::{BlockId, Ir51, Operand},
    opt::cfg_restorer::{BasicBlock, ControlFlowGraph},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
struct CriticalEdgeInfo {
    fromid: BlockId,
    toid: BlockId,
}

// This type represent a sequence of phis to be converted as moves
// in a block by the block the phi is generated from and the dest
// and src operands.
type PhiList = HashMap<BlockId, Vec<(Operand, Operand)>>;
type NewEdges = HashMap<CriticalEdgeInfo, BlockId>;

pub struct PhiElimination {
    block_idx_counter: BlockId,
    new_edges: NewEdges,
}

impl PhiElimination {
    pub fn new(cfg: &ControlFlowGraph) -> Self {
        Self {
            block_idx_counter: cfg.blocks.keys().max().unwrap_or(&0) + 1,
            new_edges: HashMap::new(),
        }
    }

    pub fn eliminate(&mut self, cfg: &mut ControlFlowGraph) {
        for block in cfg.blocks.clone().values() {
            self.convert_phis_to_mov(block, cfg);
        }

        // Modify edges for critical edged and add new blocks to the cfg
        for (edge, new_blockid) in &self.new_edges {
            let [from_block, to_block] = cfg.blocks.get_disjoint_mut([&edge.fromid, &edge.toid]);
            let [from_block, to_block] = [from_block.unwrap(), to_block.unwrap()];
            Self::insert_middle_block(from_block, *new_blockid, to_block);
        }

        // Clean up all the phis
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

    fn convert_phis_to_mov(&mut self, block: &BasicBlock, cfg: &mut ControlFlowGraph) {
        let mut phi_list: PhiList = HashMap::new();
        let mut final_list: PhiList = HashMap::new();

        for phi in &block.phis {
            for from_blockid in phi.incoming.keys() {
                phi_list.insert(*from_blockid, Vec::new());
                final_list.insert(*from_blockid, Vec::new());
            }
        }

        for phi in &block.phis {
            for (from_blockid, incoming_operand) in &phi.incoming {
                phi_list
                    .entry(*from_blockid)
                    .or_default()
                    .push((phi.operand, *incoming_operand));
            }
        }

        for (from_blockid, phis) in phi_list.iter_mut() {
            let mut temp_counter = 0;
            let mut do_temp = false;

            while let Some((dest, src)) = phis.pop() {
                let temp = Operand::Temp(temp_counter);

                for (_, s) in &mut *phis {
                    if dest == *s {
                        *s = temp;
                        do_temp = true;
                    }
                }

                if do_temp {
                    final_list
                        .entry(*from_blockid)
                        .or_default()
                        .push((temp, dest));
                    temp_counter += 1;
                }

                final_list
                    .entry(*from_blockid)
                    .or_default()
                    .push((dest, src));

                do_temp = false;
            }
        }

        for (from_blockid, assignments) in final_list {
            if from_blockid == block.id {
                continue;
            }

            let [from_block, to_block] = cfg.blocks.get_disjoint_mut([&from_blockid, &block.id]);
            let [from_block, to_block] = [from_block.unwrap(), to_block.unwrap()];

            let (which_block, start) = if from_block.successors.len() > 1 {
                if 1 < block.predecessors.len() {
                    let middle_blockid = self.critical_edge(from_block.id, block.id, cfg);
                    let middle_block = cfg.blocks.get_mut(&middle_blockid).unwrap();
                    (middle_block, false)
                } else {
                    (to_block, true)
                }
            } else {
                (from_block, false)
            };

            for (dest, src) in assignments {
                Self::insert_move(which_block, dest, src, start);
            }
        }
    }

    // Inserts new block in the cfg if it doesn't already exists and returns its id
    // otherwise it just returns the id of the previus existing block
    fn critical_edge(
        &mut self,
        fromid: BlockId,
        toid: BlockId,
        cfg: &mut ControlFlowGraph,
    ) -> BlockId {
        let key = CriticalEdgeInfo { fromid, toid };

        *self.new_edges.entry(key).or_insert_with(|| {
            let mut new_block = BasicBlock::new(self.block_idx_counter);
            new_block.irs.push(Ir51::Jump(toid));

            new_block.predecessors.push(fromid);
            new_block.successors.push(toid);

            cfg.blocks.insert(new_block.id, new_block);

            self.block_idx_counter += 1;
            self.block_idx_counter - 1
        })
    }

    fn insert_middle_block(
        from_block: &mut BasicBlock,
        middle_blockid: BlockId,
        to_block: &mut BasicBlock,
    ) {
        // If a block has more than one successor then it MUST have at least
        // one ir and the last one MUST be a branchif (for now)
        match from_block
            .irs
            .last_mut()
            .expect("Empty block for some reason")
        {
            Ir51::BranchIf {
                true_jmp,
                false_jmp,
                ..
            } => {
                let is_true = *true_jmp == to_block.id;
                let is_false = *false_jmp == to_block.id;

                if is_true {
                    *true_jmp = middle_blockid;
                } else if is_false {
                    *false_jmp = middle_blockid;
                }

                if !is_true && !is_false {
                    unreachable!();
                }
            }
            _ => unreachable!("Only beanchifs can be the last instruction of a critical edge"),
        }

        for suc in &mut from_block.successors {
            if *suc == to_block.id {
                *suc = middle_blockid;
            }
        }

        for pred in &mut to_block.predecessors {
            if *pred == from_block.id {
                *pred = middle_blockid;
            }
        }
    }

    // Inserts a move in a block with the option to insert it at the start or at the end
    // but before the last jump
    fn insert_move(block: &mut BasicBlock, dest: Operand, src: Operand, start: bool) {
        let irs = &mut block.irs;
        let mov = Ir51::Move { dest, src };

        if start {
            irs.insert(0, mov);
        } else {
            let irs_len = irs.len();
            let last_ir = irs.last().unwrap_or(&Ir51::Nop);

            let mut shift = 0;
            match last_ir {
                Ir51::Jump(_) |
                Ir51::BranchIf { .. } => shift = 1,

                Ir51::Call(_) | // TODO: Tecnically a jump when we support it
                Ir51::Move { .. } |
                Ir51::BinOp { .. } |
                Ir51::Store { .. } |
                Ir51::Load { .. } |
                Ir51::Return |
                Ir51::Nop => {}
            }

            irs.insert(irs_len - shift, mov);
        }
    }
}
