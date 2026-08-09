use crate::{
    common::{Addr, AddrSpace, BinOp, Constant, Ir51, Operand, TempIndex},
    opt::cfg::{BasicBlock, ControlFlowGraph},
};

#[derive(Debug, Clone)]
struct FusedOperation {
    start_idx: usize,
    end_idx: usize,
    op: BinOp,
    dest: Operand,
    lhs: Operand,
    rhs: Operand,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
// TODO: We probably need to have a size in classic as well
pub enum TrackedValue {
    Classic(Addr),
    Temporary(TempIndex),
    Constant(Constant),
}

impl TrackedValue {
    pub fn from_operand(op: &Operand) -> Option<Self> {
        match op {
            Operand::Var(addr, _, _) => Some(Self::Classic(*addr)),
            Operand::Local(id) => Some(Self::Classic(*id as Addr)),
            Operand::Temp(id) => Some(Self::Temporary(*id)),
            Operand::Constant(value) => Some(Self::Constant(*value)),

            Operand::Register(_) | Operand::TempSSAVar { .. } | Operand::SSAVar { .. } => None,
        }
    }
}

#[derive(Default)]
pub struct VariableWidthAggregation;

// TODO: this phase probably needs to be more general
impl VariableWidthAggregation {
    pub fn execute(&mut self, cfg: &mut ControlFlowGraph) {
        self.concatenate_variables(cfg);

        if cfg!(debug_assertions) {
            println!("======================== VAR WIDTH AGGREGATION ========================");
            let mut sorted_blocks: Vec<&BasicBlock> = cfg.blocks.values().collect();
            sorted_blocks.sort_by_key(|b| b.id);
            for block in sorted_blocks {
                print!("{}", block);
            }
        }
    }

    fn concatenate_variables(&mut self, cfg: &mut ControlFlowGraph) {
        let mut modified = true;
        while modified {
            modified = false;

            for block in cfg.blocks.values_mut() {
                modified |= self.aggregate_intra_block(block);
            }
        }
    }

    fn aggregate_intra_block(&mut self, block: &mut BasicBlock) -> bool {
        if let Some(fusedop) = Self::search_nbit_operation(&block.irs) {
            let with = vec![Ir51::BinOp {
                op: fusedop.op,
                dest: fusedop.dest,
                lhs: fusedop.lhs,
                rhs: fusedop.rhs,
            }];

            // TODO: we shouldn't use splice if there are other instruction in between start idx and
            // end idx but its fine for now
            block.irs.splice(fusedop.start_idx..=fusedop.end_idx, with);
            true
        } else {
            false
        }
    }

    fn search_nbit_operation(irs: &[Ir51]) -> Option<FusedOperation> {
        for (i, ir) in irs.iter().enumerate() {
            if let Ir51::BinOp { op, .. } = ir {
                let fuse_result = match op {
                    BinOp::Addc => Self::try_fusing_addc(irs, i),
                    BinOp::Subb => todo!(),

                    BinOp::Or => todo!(),
                    BinOp::And => todo!(),
                    BinOp::Xor => todo!(),

                    _ => None,
                };

                if fuse_result.is_some() {
                    return fuse_result;
                }
            }
        }

        None
    }

    fn try_fusing_addc(irs: &[Ir51], addc_index: usize) -> Option<FusedOperation> {
        let Ir51::BinOp {
            op: BinOp::Addc,
            dest,
            lhs,
            rhs,
        } = irs[addc_index]
        else {
            unreachable!()
        };

        let _ = rhs.to_var_addr()?;
        let (lhs_high, _) = Self::find_last_initialization(&lhs, irs, addc_index)?;
        let (dest_high, end_idx) = Self::find_next_usage(&dest, irs, addc_index)?;
        let (_, _) = (lhs_high.to_var_addr()?, dest_high.to_var_addr()?);

        let add_index = Self::find_last_add(irs, addc_index)?;
        let Ir51::BinOp {
            op: BinOp::Add,
            dest,
            lhs,
            rhs,
        } = irs[add_index]
        else {
            unreachable!()
        };

        let rhs_low = rhs.to_var_addr()?;
        let (lhs_low, start_idx) = Self::find_last_initialization(&lhs, irs, add_index)?;
        let (dest_low, _) = Self::find_next_usage(&dest, irs, add_index)?;
        let (lhs_low, dest_low) = (lhs_low.to_var_addr()?, dest_low.to_var_addr()?);

        let op = BinOp::Add;
        let lhs = Operand::Var(lhs_low, AddrSpace::iData, 2);
        let rhs = Operand::Var(rhs_low, AddrSpace::iData, 2);
        let dest = Operand::Var(dest_low, AddrSpace::iData, 2);

        Some(FusedOperation {
            start_idx,
            end_idx,
            op,
            dest,
            lhs,
            rhs,
        })
    }

    pub fn find_last_add(irs: &[Ir51], from: usize) -> Option<usize> {
        for (i, ir) in irs.iter().enumerate().take(from).rev() {
            if let Ir51::BinOp { op: BinOp::Add, .. } = ir {
                return Some(i);
            }
        }

        None
    }

    // Finds the next instruciton that used the forop Operand for an assignent and returns the
    // destination and the index of the instruction in was found into
    pub fn find_next_usage(forop: &Operand, irs: &[Ir51], from: usize) -> Option<(Operand, usize)> {
        for (i, ir) in irs.iter().enumerate().skip(from) {
            match ir {
                Ir51::Move { dest, src } if src == forop => return Some((*dest, i)),
                _ => {}
            }
        }

        None
    }

    // Finds the last instruction that assigned to the forop argument and returns the source and the
    // index of the instruction it was found into
    pub fn find_last_initialization(
        forop: &Operand,
        irs: &[Ir51],
        from: usize,
    ) -> Option<(Operand, usize)> {
        for (i, ir) in irs.iter().enumerate().take(from).rev() {
            match ir {
                Ir51::Move { dest, src } if dest == forop => return Some((*src, i)),
                _ => {}
            }
        }

        None
    }
}
