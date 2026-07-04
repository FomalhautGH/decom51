use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    ops::Range,
};

use crate::common::{BlockId, Ir51, Phi};

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: BlockId,
    pub ir_range: Range<usize>,
    pub successors: Vec<BlockId>,
    pub predecessors: Vec<BlockId>,
    pub phis: Vec<Phi>,
}

impl BasicBlock {
    pub fn find_blockid_where(blocks: &[BasicBlock], index: usize) -> BlockId {
        blocks
            .binary_search_by(|b| {
                if b.ir_range.contains(&index) {
                    Ordering::Equal
                } else {
                    b.ir_range.start.cmp(&index)
                }
            })
            .map(|i| blocks[i].id)
            .unwrap_or_else(|_| unreachable!("Didnt find block with {index} inside."))
    }
}

pub type HashBlocks = HashMap<BlockId, BasicBlock>;

#[derive(Debug)]
pub struct ControlFlowGraph {
    pub ir_code: Vec<Ir51>,
    pub blocks: HashBlocks,
    pub start_block: BlockId,
}

#[derive(Debug)]
pub struct CFGRestorer;

impl CFGRestorer {
    pub fn restore_raw_cfg(ir_code: &mut [Ir51]) -> ControlFlowGraph {
        let leaders = CFGRestorer::find_leaders(ir_code);
        let mut blocks = CFGRestorer::create_blocks(ir_code, &leaders);

        CFGRestorer::fill_successors(&mut blocks, ir_code);
        CFGRestorer::fill_predecessors(&mut blocks);

        let blocks = blocks.into_iter().map(|b| (b.id, b)).collect();

        if cfg!(debug_assertions) {
            println!("======================== IR CODE AFTER CFG ========================");
            for (i, ir) in ir_code.iter().enumerate() {
                println!("{i}: {ir:?}")
            }
        }

        ControlFlowGraph {
            ir_code: ir_code.to_vec(),
            blocks,
            start_block: 0, // TODO: to change in the future cuz the main doesn't always start at block 0
        }
    }

    fn fill_predecessors(blocks: &mut [BasicBlock]) {
        for i in 0..blocks.len() {
            for &s in &blocks[i].successors {
                let predecessors = &mut blocks[s].predecessors;
                predecessors.push(i);
            }
        }
    }

    fn fill_successors(blocks: &mut [BasicBlock], ir_code: &mut [Ir51]) {
        for i in 0..blocks.len() {
            let end = blocks[i].ir_range.end - 1;
            let last_instr = ir_code.get_mut(end).unwrap();

            match last_instr {
                Ir51::BranchIf {
                    true_jmp,
                    false_jmp,
                    ..
                } => {
                    *true_jmp = BasicBlock::find_blockid_where(blocks, *true_jmp);
                    *false_jmp = BasicBlock::find_blockid_where(blocks, *false_jmp);
                    blocks[i].successors.push(*true_jmp);
                    blocks[i].successors.push(*false_jmp);
                }

                Ir51::Jump(whr) => {
                    *whr = BasicBlock::find_blockid_where(blocks, *whr);
                    blocks[i].successors.push(*whr);
                }

                Ir51::Move { .. }
                | Ir51::BinOp { .. }
                | Ir51::Store { .. }
                | Ir51::Load { .. }
                | Ir51::Call(_)
                | Ir51::Nop
                | Ir51::Return => {
                    if i != blocks.len() - 1 {
                        blocks[i]
                            .successors
                            .push(BasicBlock::find_blockid_where(blocks, end + 1));
                    }
                }

                Ir51::InvalidIr => unreachable!("Invalid IR touched"),
            }
        }
    }

    fn create_blocks(ir_code: &[Ir51], leaders: &[usize]) -> Vec<BasicBlock> {
        let mut blocks = Vec::new();

        for (i, leader) in leaders.windows(2).enumerate() {
            let [start, end] = *leader else {
                unreachable!()
            };

            blocks.push(BasicBlock {
                id: i,
                ir_range: start..end,
                successors: Vec::new(),
                predecessors: Vec::new(),
                phis: Vec::new(),
            });
        }

        let last_leader = leaders.last().unwrap_or(&0);

        // All remaining irs go to the last block
        let last_block = BasicBlock {
            id: leaders.len() - 1,
            ir_range: *last_leader..ir_code.len(),
            successors: Vec::new(),
            predecessors: Vec::new(),
            phis: Vec::new(),
        };

        blocks.push(last_block);
        blocks
    }

    fn find_leaders(ir_code: &[Ir51]) -> Vec<usize> {
        let mut leaders = HashSet::new();

        // First instruction always a leader
        leaders.insert(0);

        let mut after_jump = false;
        for (i, ir) in ir_code.iter().enumerate() {
            if after_jump {
                leaders.insert(i);
                after_jump = false;
            }

            match ir {
                Ir51::BranchIf {
                    true_jmp,
                    false_jmp,
                    ..
                } => {
                    leaders.insert(*true_jmp);
                    leaders.insert(*false_jmp);
                    after_jump = true;
                }

                Ir51::Jump(jmp) => {
                    leaders.insert(*jmp);
                    after_jump = true;
                }

                Ir51::Move { .. }
                | Ir51::BinOp { .. }
                | Ir51::Store { .. }
                | Ir51::Load { .. }
                | Ir51::Call(_)
                | Ir51::Return
                | Ir51::Nop => {}
                Ir51::InvalidIr => unreachable!("Invalid IR touched"),
            };
        }

        let mut leaders: Vec<usize> = leaders.into_iter().collect();
        leaders.sort();

        leaders
    }
}
