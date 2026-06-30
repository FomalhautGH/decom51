use crate::{
    common::Ir51,
    opt::{cfg_restorer::CFGRestorer, const_propagation::ConstPropagation, ssa::SSABuilder},
};

mod cfg_restorer;
mod const_propagation;
mod ssa;

pub struct Optimizer;

impl Optimizer {
    pub fn optimize(ir_code: &mut [Ir51]) {
        let mut cfg = CFGRestorer::restore_raw_cfg(ir_code);
        SSABuilder::build(&mut cfg);

        let mut const_prop = ConstPropagation::default();
        const_prop.execute(&mut cfg);
    }
}
