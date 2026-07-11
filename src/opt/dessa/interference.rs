use crate::{
    common::{BlockId, Ir51, Operand, Phi, SSAVar, format_addr, is_ssa_smaller},
    opt::{
        cfg_restorer::{BasicBlock, ControlFlowGraph},
        dessa::liveness::LocalSet,
    },
};
use std::collections::HashSet;

// We put an Arc when the two SSAVars interefere with each other,
// to avoid duplicates since it is a simmetric relationship we always
// put the 'smallest' one first; the smallest beign the one with the `addr`
// and version smaller.
//
// Tho SSAVars interfere with each other when they need to *live* at
// the same time
pub type InterferenceGraph = HashSet<(SSAVar, SSAVar)>;

// TODO: put this method in a better place
pub fn do_they_interfere(graph: &InterferenceGraph, a: SSAVar, b: SSAVar) -> bool {
    graph.contains(&(a, b)) || graph.contains(&(b, a))
}

pub struct Interference<'a> {
    out_sets: &'a LocalSet,
    interferences: InterferenceGraph,
}

impl<'a> Interference<'a> {
    pub fn new(out_sets: &'a LocalSet) -> Self {
        Self {
            out_sets,
            interferences: HashSet::new(),
        }
    }

    pub fn build_graph(&mut self, cfg: &ControlFlowGraph) -> InterferenceGraph {
        for block in cfg.blocks.values() {
            self.analyze_block(block);
        }

        if cfg!(debug_assertions) {
            println!("Interferences graph:");
            let mut sorted_interferences: Vec<&(SSAVar, SSAVar)> =
                self.interferences.iter().collect();
            sorted_interferences.sort_by_key(|pair| {
                format!(
                    "{}_{} <-> {}_{}",
                    format_addr(pair.0.0),
                    pair.0.1,
                    format_addr(pair.1.0),
                    pair.1.1
                )
            });
            for pair in sorted_interferences {
                println!(
                    "  {}_{} <-> {}_{}",
                    format_addr(pair.0.0),
                    pair.0.1,
                    format_addr(pair.1.0),
                    pair.1.1
                );
            }
        }

        self.interferences.clone()
    }

    fn analyze_block(&mut self, block: &BasicBlock) {
        let mut liveset = self.liveset_init(block.id);

        for ir in block.irs.iter().rev() {
            self.analyze_ir(ir, &mut liveset);
        }

        self.analyze_phis(&block.phis, &mut liveset);
    }

    fn analyze_phis(&mut self, phis: &[Phi], liveset: &mut HashSet<SSAVar>) {
        let mut phi_destinations: Vec<Operand> = Vec::with_capacity(phis.len());
        for phi in phis.iter() {
            self.add_write(&phi.operand, liveset);
            phi_destinations.push(phi.operand);
        }

        // Since Phis happen "simultainously" all the destinations interfere with
        // each other
        for i in 0..phi_destinations.len() {
            for j in (i + 1)..phi_destinations.len() {
                let ssavar1 = phi_destinations[i].to_ssavar().unwrap();
                let ssavar2 = phi_destinations[j].to_ssavar().unwrap();

                if is_ssa_smaller(ssavar1, ssavar2) {
                    self.interferences.insert((ssavar1, ssavar2));
                } else {
                    self.interferences.insert((ssavar2, ssavar1));
                }
            }
        }
    }

    fn analyze_ir(&mut self, ir: &Ir51, liveset: &mut HashSet<SSAVar>) {
        match ir {
            Ir51::Move { dest, src } => {
                self.add_write(dest, liveset);
                self.add_read(src, liveset);
            }

            Ir51::BinOp { dest, lhs, rhs, .. } => {
                self.add_write(dest, liveset);
                self.add_read(lhs, liveset);
                self.add_read(rhs, liveset);
            }

            Ir51::Store {
                dest_ptr, value, ..
            } => {
                self.add_read(dest_ptr, liveset);
                self.add_read(value, liveset);
            }

            Ir51::Load { dest, src_ptr, .. } => {
                self.add_write(dest, liveset);
                self.add_read(src_ptr, liveset);
            }

            Ir51::BranchIf { cond, .. } => self.add_read(cond, liveset),

            Ir51::Nop | Ir51::Jump(_) | Ir51::Call(_) | Ir51::Return => {}
            Ir51::InvalidIr => unreachable!("Invalid IR Touched"),
        }
    }

    fn add_read(&mut self, op: &Operand, liveset: &mut HashSet<SSAVar>) {
        let Operand::SSAVar { addr, version } = *op else {
            return;
        };

        let op = (addr, version);
        liveset.insert(op);
    }

    // TODO: Maybe add a methond on operand that converts in SSAVars
    fn add_write(&mut self, dest: &Operand, liveset: &mut HashSet<SSAVar>) {
        let Operand::SSAVar { addr, version } = *dest else {
            return;
        };

        let dest = (addr, version);
        liveset.remove(&dest);

        for ssavar in liveset.iter() {
            if is_ssa_smaller(dest, *ssavar) {
                self.interferences.insert((dest, *ssavar));
            } else {
                self.interferences.insert((*ssavar, dest));
            }
        }
    }

    fn liveset_init(&mut self, blockid: BlockId) -> HashSet<SSAVar> {
        // The liveset is a set that represents all the SSAVars than need
        // to live at the same moment, since we iterate the irs of the block
        // in reverse, at the start it needs to be the same as the out_set
        let liveset = self.out_sets[&blockid].clone();

        // TODO: Try to improve this nesthed loops by doing less loops like
        // the one in analyze_phis
        for i in &liveset {
            for j in &liveset {
                if is_ssa_smaller(*i, *j) {
                    self.interferences.insert((*i, *j));
                }
            }
        }

        liveset
    }
}
