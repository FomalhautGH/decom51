use std::collections::HashSet;

use crate::{
    hir::Hir51,
    mir::{Expr, Mir51},
    opt::structured_cfg::{BlockKind, StructuredBlock, StructuredCFG},
};

#[derive(Default)]
pub struct ControlLogicExtraction {
    pub extracted_hirs: Vec<Hir51>,
}

impl ControlLogicExtraction {
    pub fn execute(&mut self, mut cfg: StructuredCFG) {
        self.extracted_hirs.clear();

        let mut block_stack = vec![cfg.start_block];

        while let Some(block_id) = block_stack.pop() {
            let structured_block = cfg.blocks.remove(&block_id).unwrap();
            let next_id = structured_block.successors.first().copied();

            let hirs = self.extract_block(structured_block);
            self.extracted_hirs.extend(hirs);

            if let Some(next_id) = next_id {
                block_stack.push(next_id);
            }
        }
    }

    fn extract_block(&self, block: StructuredBlock) -> Vec<Hir51> {
        match block.kind {
            BlockKind::Flat(mirs) => self.extract_flat_block(mirs),
            BlockKind::Loop { header, body } => self.extract_loop_block(*header, body),
            BlockKind::Condition {
                condition,
                true_body,
                false_body,
            } => self.extract_condition_block(*condition, true_body, false_body),
        }
    }

    fn extract_condition_block(
        &self,
        condition: StructuredBlock,
        true_body: Vec<StructuredBlock>,
        false_body: Vec<StructuredBlock>,
    ) -> Vec<Hir51> {
        todo!()
    }

    fn extract_flat_block(&self, mirs: Vec<Mir51>) -> Vec<Hir51> {
        let mut hirs = Vec::new();

        for mir in mirs {
            hirs.push(Hir51::from(mir));
        }

        hirs
    }

    fn extract_loop_block(
        &self,
        header: StructuredBlock,
        body: Vec<StructuredBlock>,
    ) -> Vec<Hir51> {
        let BlockKind::Flat(header_mirs) = header.kind else {
            unreachable!()
        };

        // We use an hashset to understant which block ids are part of the loop
        // so that we can understand the correct condition of the while loop
        let mut loop_body = Vec::new();
        let mut body_block_ids = HashSet::new();

        for block in body {
            body_block_ids.insert(block.id);
            loop_body.extend(self.extract_block(block));
        }

        let last_instruction = header_mirs.into_iter().next_back().unwrap();
        let loop_condition = match last_instruction {
            Mir51::Goto(block_id) if block_id == header.id => Expr::Constant(1),
            Mir51::If {
                cond,
                true_jmp,
                false_jmp,
            } => {
                if body_block_ids.contains(&true_jmp) {
                    cond
                } else if body_block_ids.contains(&false_jmp) {
                    cond.negate()
                } else {
                    unreachable!()
                }
            }
            _ => unreachable!(),
        };

        loop_body.pop(); // Remove last goto of while loop

        let while_loop = Hir51::While {
            cond: loop_condition,
            body: loop_body,
        };

        vec![while_loop]
    }
}
