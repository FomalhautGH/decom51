use std::collections::HashMap;

use crate::{
    common::{BlockId, Ir51, Operand, SSAVar, is_ssa_smaller},
    opt::{
        cfg_restorer::{BasicBlock, ControlFlowGraph},
        dessa::interference::InterferenceGraph,
    },
};

struct MoveInfo {
    dest: Operand,
    src: Operand,
    blockid: BlockId,
    idx: usize,
}

type Mappings = HashMap<SSAVar, SSAVar>;

pub struct VariableCoalescing {
    interferences: InterferenceGraph,
    mappings: Mappings,
}

impl VariableCoalescing {
    pub fn new(interferences: InterferenceGraph) -> Self {
        Self {
            interferences,
            mappings: HashMap::new(),
        }
    }

    pub fn execute(&mut self, cfg: &mut ControlFlowGraph) {
        let mut mov_queue = Self::init_mov_queue(cfg);

        while let Some(mov) = mov_queue.pop() {
            let ssadest = self.find_mapping_for(&mov.dest).unwrap();
            let ssasrc = self.find_mapping_for(&mov.src).unwrap();

            if ssadest == ssasrc {
                // TODO: group these 3 instructions
                let movblock = cfg.blocks.get_mut(&mov.blockid).unwrap();
                let movir = &mut movblock.irs[mov.idx];
                *movir = Ir51::Nop;
                continue;
            }

            if !self.interferences.contains(&(ssadest, ssasrc))
                && !self.interferences.contains(&(ssasrc, ssadest))
            {
                self.fusion(mov, cfg);
            }
        }

        self.renaming(cfg);
        cfg.remove_nops();

        if cfg!(debug_assertions) {
            println!("======================== VARIABLE COALESCING ========================");
            let mut sorted_blocks: Vec<&BasicBlock> = cfg.blocks.values().collect();
            sorted_blocks.sort_by_key(|b| b.id);
            for block in sorted_blocks {
                print!("{}", block);
            }
        }
    }

    fn renaming(&self, cfg: &mut ControlFlowGraph) {
        for block in cfg.blocks.values_mut() {
            for ir in block.irs.iter_mut() {
                self.rename(ir);
            }
        }
    }

    fn rename(&self, ir: &mut Ir51) {
        let replace = |op: &mut Operand| {
            if let Some((addr, version)) = self.find_mapping_for(op) {
                *op = Operand::SSAVar { addr, version }
            }
        };

        match ir {
            Ir51::Move { dest, src } => {
                replace(dest);
                replace(src);
            }

            Ir51::BinOp { dest, lhs, rhs, .. } => {
                replace(dest);
                replace(lhs);
                replace(rhs);
            }

            Ir51::Store {
                dest_ptr, value, ..
            } => {
                replace(dest_ptr);
                replace(value);
            }

            Ir51::Load { dest, src_ptr, .. } => {
                replace(dest);
                replace(src_ptr);
            }

            Ir51::BranchIf { cond, .. } => replace(cond),

            Ir51::Jump(_) | Ir51::Call(_) | Ir51::Return | Ir51::Nop => {}
            Ir51::InvalidIr => unreachable!("Invalid IR Touched"),
        }
    }

    fn find_mapping_for(&self, op: &Operand) -> Option<SSAVar> {
        let ssaop = op.to_ssavar()?;

        let mut it = ssaop;
        while let Some(mapping) = self.mappings.get(&it) {
            it = *mapping;
        }

        Some(it)
    }

    fn fusion(&mut self, mov: MoveInfo, cfg: &mut ControlFlowGraph) {
        let movblock = cfg.blocks.get_mut(&mov.blockid).unwrap();
        let movir = &mut movblock.irs[mov.idx];

        // Map x -> y
        let x = self.find_mapping_for(&mov.dest).unwrap();
        let y = self.find_mapping_for(&mov.src).unwrap();
        self.mappings.insert(x, y);

        // Create new intereferences for y cuz now y intereferes with everything
        // x interefered before
        let mut new_itf: Vec<(SSAVar, SSAVar)> = Vec::new();
        for (a, b) in &self.interferences {
            let (a, b) = (*a, *b);

            let itf = if x == a {
                (y, b)
            } else if x == b {
                (a, y)
            } else {
                continue;
            };

            if is_ssa_smaller(itf.0, itf.1) {
                new_itf.push((itf.0, itf.1));
            } else {
                new_itf.push((itf.1, itf.0));
            }
        }

        self.interferences.retain(|(a, b)| {
            let (a, b) = (*a, *b);
            a != x && b != x
        });

        self.interferences.extend(new_itf);
        *movir = Ir51::Nop;
    }

    fn init_mov_queue(cfg: &ControlFlowGraph) -> Vec<MoveInfo> {
        let mut queue = Vec::new();

        for block in cfg.blocks.values() {
            for (i, ir) in block.irs.iter().enumerate() {
                if let Ir51::Move { dest, src } = *ir
                    && dest.to_ssavar().is_some()
                    && src.to_ssavar().is_some()
                {
                    queue.push(MoveInfo {
                        dest,
                        src,
                        blockid: block.id,
                        idx: i,
                    });
                }
            }
        }

        queue
    }
}
