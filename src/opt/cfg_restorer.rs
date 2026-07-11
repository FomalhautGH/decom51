use std::collections::{HashMap, HashSet};

use crate::common::{BlockId, Ir51, Operand, Phi};

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: BlockId,
    pub irs: Vec<Ir51>,
    pub successors: Vec<BlockId>,
    pub predecessors: Vec<BlockId>,
    pub phis: Vec<Phi>,
}

impl BasicBlock {
    // TODO: Check if there are places where i can use this new function
    pub fn new(id: BlockId) -> Self {
        Self {
            id,
            irs: Vec::new(),
            successors: Vec::new(),
            predecessors: Vec::new(),
            phis: Vec::new(),
        }
    }
}

pub type HashBlocks = HashMap<BlockId, BasicBlock>;

#[derive(Debug)]
pub struct ControlFlowGraph {
    pub blocks: HashBlocks,
    pub start_block: BlockId,
}

impl ControlFlowGraph {
    pub fn remove_nops(&mut self) {
        // TODO: Use a better null operand for the phis
        for block in self.blocks.values_mut() {
            block.irs.retain(|ir| !matches!(ir, Ir51::Nop));
            block
                .phis
                .retain(|phi| !matches!(phi.operand, Operand::Constant(0)));
        }
    }
}

#[derive(Debug)]
pub struct CFGRestorer;

impl CFGRestorer {
    pub fn restore_raw_cfg(ir_code: &mut [Ir51]) -> ControlFlowGraph {
        let leaders = CFGRestorer::find_leaders(ir_code);
        let mut blocks = CFGRestorer::create_blocks(ir_code, &leaders);

        CFGRestorer::fill_successors(&mut blocks, &leaders);
        CFGRestorer::fill_predecessors(&mut blocks);

        let blocks = blocks.into_iter().map(|b| (b.id, b)).collect();

        if cfg!(debug_assertions) {
            println!("======================== IR CODE AFTER CFG ========================");
            for (i, ir) in ir_code.iter().enumerate() {
                println!("{i}: {ir:?}")
            }
        }

        ControlFlowGraph {
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

    fn fill_successors(blocks: &mut [BasicBlock], leaders: &[usize]) {
        let num_blocks = blocks.len();

        for (i, block) in blocks.iter_mut().enumerate() {
            let last_instr = block.irs.last_mut().unwrap();

            match last_instr {
                Ir51::BranchIf {
                    true_jmp,
                    false_jmp,
                    ..
                } => {
                    *true_jmp = CFGRestorer::find_block_id(leaders, *true_jmp);
                    *false_jmp = CFGRestorer::find_block_id(leaders, *false_jmp);
                    block.successors.push(*true_jmp);
                    block.successors.push(*false_jmp);
                }

                Ir51::Jump(whr) => {
                    *whr = CFGRestorer::find_block_id(leaders, *whr);
                    block.successors.push(*whr);
                }

                Ir51::Move { .. }
                | Ir51::BinOp { .. }
                | Ir51::Store { .. }
                | Ir51::Load { .. }
                | Ir51::Call(_)
                | Ir51::Nop => {
                    if i != num_blocks - 1 {
                        block.successors.push(i + 1);
                    }
                }

                // If it ends with a return it should not have any successors since
                // the cfg is single for every function
                Ir51::Return => {}
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
                irs: ir_code[start..end].to_vec(),
                successors: Vec::new(),
                predecessors: Vec::new(),
                phis: Vec::new(),
            });
        }

        let last_leader = leaders.last().unwrap_or(&0);

        // All remaining irs go to the last block
        let last_block = BasicBlock {
            id: leaders.len() - 1,
            irs: ir_code[*last_leader..].to_vec(),
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

    // User leaders to find which block index is a part of
    fn find_block_id(leaders: &[usize], index: usize) -> BlockId {
        // The Err case works because it gives you the index of the element that is greater
        // than the one it wants to find, so the index is inside the block with
        // that leader - 1
        match leaders.binary_search(&index) {
            Ok(idx) => idx,
            Err(idx) => idx - 1,
        }
    }
}
