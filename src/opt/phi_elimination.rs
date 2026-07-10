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

type NewEdges = HashMap<CriticalEdgeInfo, BlockId>;

#[derive(Default)]
pub struct PhiElimination {
    new_edges: NewEdges,
}

impl PhiElimination {
    pub fn eliminate(&mut self, cfg: &mut ControlFlowGraph) {
        for block in cfg.blocks.clone().values() {
            self.eliminate_phis(block, cfg);
        }

        // Modify edges for critical edged and add new blocks to the cfg
        for (edge, new_blockid) in &self.new_edges {
            let from_block = cfg.blocks.get_mut(&edge.fromid).unwrap();
            Self::critical_edge(from_block, *new_blockid, edge.toid);
        }

        // Clean up all the phis
        for block in cfg.blocks.values_mut() {
            block.phis.clear();
        }

        if cfg!(debug_assertions) {
            println!("======================== PHI ELIMINATION ========================");
            for block in cfg.blocks.values() {
                println!("{:#?}", block);
            }
        }
    }

    fn eliminate_phis(&mut self, block: &BasicBlock, cfg: &mut ControlFlowGraph) {
        let mut block_idx = cfg.blocks.len();

        for phi in &block.phis {
            for (from, incoming) in &phi.incoming {
                let from_block = cfg.blocks.get_mut(from).unwrap();

                if from_block.successors.len() > 1 && 1 < block.predecessors.len() {
                    let key = CriticalEdgeInfo {
                        fromid: from_block.id,
                        toid: block.id,
                    };

                    let middle_blockid = self.new_edges.entry(key).or_insert_with(|| {
                        let mut new_block = BasicBlock::new(block_idx);
                        new_block.irs.push(Ir51::Jump(block.id));

                        new_block.predecessors.push(*from);
                        new_block.successors.push(block.id);

                        cfg.blocks.insert(new_block.id, new_block);

                        block_idx += 1;
                        block_idx - 1
                    });

                    let middle_block = cfg.blocks.get_mut(middle_blockid).unwrap();
                    Self::insert_move(&mut middle_block.irs, phi.operand, *incoming);
                } else {
                    Self::insert_move(&mut from_block.irs, phi.operand, *incoming);
                }
            }
        }
    }

    fn critical_edge(block: &mut BasicBlock, middle_blockid: BlockId, to_block: BlockId) {
        // If a block has more than one successor then it MUST have at least
        // one ir and the last one MUST be a branchif (for now)
        match block.irs.last_mut().expect("Empty block for some reason") {
            Ir51::BranchIf {
                true_jmp,
                false_jmp,
                ..
            } => {
                let is_true = *true_jmp == to_block;
                let is_false = *false_jmp == to_block;

                if is_true {
                    *true_jmp = middle_blockid;
                } else if is_false {
                    *false_jmp = middle_blockid;
                }

                if !is_true && !is_false {
                    unreachable!("BranchIf {{{true_jmp}, {false_jmp}}} but found {to_block}");
                }
            }
            _ => unreachable!("Only beanchifs can be the last instruction of a critical edge"),
        }

        for suc in &mut block.successors {
            if *suc == to_block {
                *suc = middle_blockid;
            }
        }
    }

    fn insert_move(irs: &mut Vec<Ir51>, dest: Operand, src: Operand) {
        let last_ir = irs.last().unwrap_or(&Ir51::Nop);

        let mov = Ir51::Move { dest, src };
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

            Ir51::InvalidIr => unreachable!("Invalid IR Touched")
        }

        let irs_len = irs.len();
        irs.insert(irs_len - shift, mov);
    }
}
