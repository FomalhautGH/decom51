use crate::{
    cfg_restorer::{BasicBlock, BlockId, ControlFlowGraph},
    common::{Addr, Ir51, Operand},
    ssa::{
        chk_algorithm::{CHKAlgorithm, Dominators},
        phis::Phis,
    },
};
use std::collections::{HashMap, HashSet};

mod chk_algorithm;
mod phis;

pub struct SSABuilder;

// Represents variables by their address and the block they are modified in
type VarsBlocks = HashMap<Addr, HashSet<BlockId>>;
type Frontiers = HashMap<BlockId, HashSet<BlockId>>;

impl SSABuilder {
    pub fn build(cfg: &mut ControlFlowGraph) {
        let shared_blocks: Vec<&BasicBlock> = cfg.blocks.values().collect();
        let immidiate_dominators = CHKAlgorithm::execute(&shared_blocks);
        let dominator_frontiers = Self::calculate_frontiers(&immidiate_dominators, &shared_blocks);
        let variables = Self::find_variables(&shared_blocks, &cfg.ir_code);

        Phis::insert_phis(cfg, &immidiate_dominators, &dominator_frontiers, &variables);

        for (i, ir) in cfg.ir_code.iter().enumerate() {
            println!("{i}: {ir:?}")
        }

        // println!("{:#?}", cfg.blocks);
        // println!("{idoms:?}");
        // println!("{variables:?}");
        // println!("{dom_frontiers:?}");
    }

    // TODO: To get a better version of this is to find where a variabile is read in order to avoid
    // placing a phi if a variable is not read in the frontiers
    fn find_variables(blocks: &[&BasicBlock], ir: &[Ir51]) -> VarsBlocks {
        let mut vars: VarsBlocks = HashMap::new();

        for b in blocks {
            for i in b.ir_range.clone() {
                let ir = ir[i];

                match ir {
                    Ir51::Move {
                        dest: Operand::Address(addr, _),
                        ..
                    } => {
                        vars.entry(addr).or_default().insert(b.id);
                    }

                    Ir51::BinOp {
                        dest: Operand::Address(addr, _),
                        ..
                    } => {
                        vars.entry(addr).or_default().insert(b.id);
                    }

                    Ir51::Load {
                        dest_ptr: Operand::Address(addr, _),
                        ..
                    } => {
                        vars.entry(addr).or_default().insert(b.id);
                    }

                    Ir51::BranchIf {
                        cond: Operand::Address(addr, _),
                        ..
                    } => {
                        vars.entry(addr).or_default().insert(b.id);
                    }

                    Ir51::BranchIf { .. } => {}
                    Ir51::Load { .. } => {}
                    Ir51::BinOp { .. } => {}
                    Ir51::Move { .. } => {}
                    Ir51::Store { .. } => {}
                    Ir51::Jump(_) => {}
                    Ir51::Call(_) => {}
                    Ir51::Return => {}
                    Ir51::InvalidIr => unreachable!("Invalid IR touched"),
                }
            }
        }

        vars
    }

    fn calculate_frontiers(idom: &Dominators, blocks: &[&BasicBlock]) -> Frontiers {
        let mut frontiers: Frontiers = HashMap::new();

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
}
