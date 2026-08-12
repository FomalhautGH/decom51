use std::collections::HashMap;

use crate::{
    common::{BlockId, Ir51, Operand, SSAVar, is_ssa_smaller},
    opt::{
        cfg::{BasicBlock, ControlFlowGraph},
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

    // Runs the coalescing pass: merges non-interfering move operands, renames variables, and drops dead NOPs.
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
        cfg.remove_useless_irs();

        if cfg!(debug_assertions) {
            println!("======================== VARIABLE COALESCING ========================");
            let mut sorted_blocks: Vec<&BasicBlock> = cfg.blocks.values().collect();
            sorted_blocks.sort_by_key(|b| b.id);
            for block in sorted_blocks {
                print!("{}", block);
            }
        }
    }

    // Iterates through all instructions and phis across all basic blocks to apply variable renamings.
    fn renaming(&self, cfg: &mut ControlFlowGraph) {
        for block in cfg.blocks.values_mut() {
            for ir in block.irs.iter_mut() {
                self.rename(ir);
            }

            for phi in block.phis.iter_mut() {
                self.replace(&mut phi.operand);
                for incoming in phi.incoming.values_mut() {
                    self.replace(incoming);
                }
            }
        }
    }

    // Updates the SSA operands of a single instruction to their coalesced target variables.
    fn rename(&self, ir: &mut Ir51) {
        match ir {
            Ir51::Move { dest, src } => {
                self.replace(dest);
                self.replace(src);
            }

            Ir51::BinOp { dest, lhs, rhs, .. } => {
                self.replace(dest);
                self.replace(lhs);
                self.replace(rhs);
            }

            Ir51::Store {
                dest_ptr, value, ..
            } => {
                self.replace(dest_ptr);
                self.replace(value);
            }

            Ir51::Load { dest, src_ptr, .. } => {
                self.replace(dest);
                self.replace(src_ptr);
            }

            Ir51::BranchIf { cond, .. } => self.replace(cond),

            Ir51::Jump(_) | Ir51::Call(_) | Ir51::Return | Ir51::Nop => {}
        }
    }

    fn replace(&self, op: &mut Operand) {
        if let Some((addr, version)) = self.find_mapping_for(op) {
            *op = Operand::SSAVar { addr, version }
        }
    }

    // Resolves the canonical coalesced SSAVar by following the mapping chain.
    fn find_mapping_for(&self, op: &Operand) -> Option<SSAVar> {
        let ssaop = op.to_ssavar()?;

        let mut it = ssaop;
        while let Some(mapping) = self.mappings.get(&it) {
            it = *mapping;
        }

        Some(it)
    }

    // Merges dest and src SSA variables, updates the interference graph, and turns the MOV into a NOP.
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

    // Collects all register-to-register SSA move instructions from the CFG into a work queue.
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
