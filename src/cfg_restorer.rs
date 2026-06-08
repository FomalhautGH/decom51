use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    ops::Range,
};

use crate::common::Ir51;

type BlockId = usize;

#[derive(Debug)]
pub struct BasicBlock {
    pub id: BlockId,
    pub ir_range: Range<usize>,
    pub successors: Vec<BlockId>,
    pub predecessors: Vec<BlockId>,
}

#[derive(Debug)]
pub struct ControlFlowGraph {
    pub ir_code: Vec<Ir51>,
    pub blocks: HashMap<BlockId, BasicBlock>,
    pub start_block: BlockId,
}

#[derive(Debug)]
pub struct CFGRestorer;

impl CFGRestorer {
    pub fn restore_raw_cfg(ir_code: &[Ir51]) -> ControlFlowGraph {
        let leaders = CFGRestorer::find_leaders(ir_code);
        let mut blocks = CFGRestorer::create_blocks(ir_code, &leaders);

        CFGRestorer::fill_successors(&mut blocks, ir_code);
        CFGRestorer::fill_predecessors(&mut blocks);

        println!("{blocks:#?}");

        let blocks = blocks.into_iter().map(|b| (b.id, b)).collect();

        ControlFlowGraph {
            ir_code: ir_code.to_vec(),
            blocks,
            start_block: 0, // TODO: to change in the future cuz the main doesn't always start at
                            // block 0
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

    fn fill_successors(blocks: &mut [BasicBlock], ir_code: &[Ir51]) {
        for i in 0..blocks.len() {
            let end = blocks[i].ir_range.end - 1;
            let last_instr = ir_code[end];

            match last_instr {
                Ir51::BranchIf {
                    true_jmp,
                    false_jmp,
                    ..
                } => {
                    let blockid_true = CFGRestorer::find_blockid_where(blocks, true_jmp);
                    let blockid_false = CFGRestorer::find_blockid_where(blocks, false_jmp);
                    blocks[i].successors.push(blockid_true);
                    blocks[i].successors.push(blockid_false);
                }

                Ir51::Jump(whr) => {
                    let blockid = CFGRestorer::find_blockid_where(blocks, whr);
                    blocks[i].successors.push(blockid);
                }

                // Last block doesn't have successors
                _ if i == blocks.len() - 1 => {}

                _ => blocks[i]
                    .successors
                    .push(CFGRestorer::find_blockid_where(blocks, end + 1)),
            }
        }
    }

    fn find_blockid_where(blocks: &[BasicBlock], addr: usize) -> usize {
        blocks
            .binary_search_by(|b| {
                if b.ir_range.contains(&addr) {
                    Ordering::Equal
                } else {
                    b.ir_range.start.cmp(&addr)
                }
            })
            .map(|i| blocks[i].id)
            .unwrap_or_else(|_| unreachable!("Didnt find block with {addr} inside."))
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
            });
        }

        let last_leader = leaders.last().unwrap_or(&0);

        // All remaining irs go to the last block
        let last_block = BasicBlock {
            id: leaders.len() - 1,
            ir_range: *last_leader..ir_code.len(),
            successors: Vec::new(),
            predecessors: Vec::new(),
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

                _ => {}
            };
        }

        let mut leaders: Vec<usize> = leaders.into_iter().collect();
        leaders.sort();

        leaders
    }
}
