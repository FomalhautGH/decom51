use crate::lifter::{Ir51, Ir51Type};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
};

type BlockId = usize;

#[derive(Debug)]
pub struct BasicBlock {
    pub id: BlockId,
    pub instructions: Vec<Ir51>,
    pub successors: Vec<BlockId>,
    pub predecessors: Vec<BlockId>,
}

#[derive(Debug)]
pub struct ControlFlowGraph {
    pub blocks: HashMap<BlockId, BasicBlock>,
    pub start_block: BlockId,
}

#[derive(Debug)]
pub struct CFGRestorer;

impl CFGRestorer {
    pub fn restore_raw_cfg(ir_code: &[Ir51]) -> ControlFlowGraph {
        let leaders = CFGRestorer::find_leaders(ir_code);
        let mut blocks = CFGRestorer::create_blocks(ir_code, &leaders);

        CFGRestorer::fill_successors(&mut blocks);
        CFGRestorer::fill_predecessors(&mut blocks);

        let blocks = blocks.into_iter().map(|b| (b.id, b)).collect();

        ControlFlowGraph {
            blocks,
            start_block: 0,
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

    fn fill_successors(blocks: &mut [BasicBlock]) {
        for i in 0..blocks.len() {
            let last_instr = blocks[i]
                .instructions
                .last()
                .expect("Empty BasicBlock for some reason");

            match last_instr.instr {
                Ir51Type::BranchIf {
                    true_jmp,
                    false_jmp,
                    ..
                } => {
                    let blockid_true = CFGRestorer::find_blockid_where(blocks, true_jmp);
                    let blockid_false = CFGRestorer::find_blockid_where(blocks, false_jmp);
                    blocks[i].successors.push(blockid_true);
                    blocks[i].successors.push(blockid_false);
                }

                Ir51Type::Jump(whr) => {
                    let blockid = CFGRestorer::find_blockid_where(blocks, whr);
                    blocks[i].successors.push(blockid);
                }

                _ if i == blocks.len() - 1 => {} // Last block doesn't have successors
                _ => blocks[i].successors.push(CFGRestorer::find_blockid_where(
                    blocks,
                    last_instr.index + 1,
                )),
            }
        }
    }

    fn find_blockid_where(blocks: &[BasicBlock], addr: usize) -> usize {
        let i = blocks
            .binary_search_by(|el| {
                let start = el.instructions.first().expect("Empty Block").index;
                let end = el.instructions.last().expect("Empty Block").index;

                if (start..=end).contains(&addr) {
                    Ordering::Equal
                } else {
                    start.cmp(&addr)
                }
            })
            .expect("Didnt find block with that addr inside");

        blocks[i].id
    }

    fn create_blocks(ir_code: &[Ir51], leaders: &[usize]) -> Vec<BasicBlock> {
        let mut blocks = Vec::new();
        let mut ir_iter = ir_code.iter();
        let mut current_ir = ir_iter.next();

        for (i, leader) in leaders.windows(2).enumerate() {
            let mut block = BasicBlock {
                id: i,
                instructions: Vec::new(),
                successors: Vec::new(),
                predecessors: Vec::new(),
            };

            if let [_, end] = leader {
                while let Some(Ir51 { index, instr }) = current_ir
                    && index < end
                {
                    block.instructions.push(Ir51 {
                        index: *index,
                        instr: *instr,
                    });
                    current_ir = ir_iter.next();
                }
            }

            blocks.push(block);
        }

        // All remaining irs go to the last block
        let mut last_block = BasicBlock {
            id: leaders.len() - 1,
            instructions: Vec::new(),
            successors: Vec::new(),
            predecessors: Vec::new(),
        };

        for rem_ir in ir_iter {
            last_block.instructions.push(*rem_ir);
        }

        blocks.push(last_block);
        blocks
    }

    fn find_leaders(ir_code: &[Ir51]) -> Vec<usize> {
        let mut leaders = HashSet::new();

        // First instruction always a leader
        if let Some(ir) = ir_code.first() {
            leaders.insert(ir.index);
        }

        // All jump targets and instructions after a jump are considered
        // leaders
        let mut after_jump = false;
        for ir in ir_code {
            let Ir51 { index, instr } = ir;

            if after_jump {
                leaders.insert(*index);
                after_jump = false;
            }

            match instr {
                Ir51Type::BranchIf {
                    true_jmp,
                    false_jmp,
                    ..
                } => {
                    leaders.insert(*true_jmp);
                    leaders.insert(*false_jmp);
                    after_jump = true;
                }

                Ir51Type::Jump(jmp) => {
                    leaders.insert(*jmp);
                    after_jump = true;
                }
                _ => {}
            };
        }

        let mut leaders: Vec<usize> = leaders.into_iter().collect();
        leaders.sort();

        leaders
    }
}
