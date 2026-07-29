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
    pub fn execute(&mut self, cfg: StructuredCFG) {
        self.extracted_hirs.clear();

        let mut block_stack = vec![cfg.start_block];

        while let Some(block_id) = block_stack.pop() {
            let structured_block = cfg.blocks.get(&block_id).unwrap();

            let hirs = self.extract_block(structured_block);
            self.extracted_hirs.extend(hirs);

            if structured_block.successors.is_empty() {
                break;
            } else if let [successor] = structured_block.successors.as_slice() {
                block_stack.push(*successor);
            } else {
                unreachable!("Block order should be sequential")
            }
        }

        if cfg!(debug_assertions) {
            for hir in &self.extracted_hirs {
                println!("{}", hir);
            }
        }
    }

    fn extract_block(&self, block: &StructuredBlock) -> Vec<Hir51> {
        match &block.kind {
            BlockKind::Flat(mirs) => self.extract_flat_block(mirs),
            BlockKind::Macro { header, blocks } => self.extract_macro_block(header, blocks),
        }
    }

    fn extract_flat_block(&self, mirs: &[Mir51]) -> Vec<Hir51> {
        let mut hirs = Vec::new();

        for mir in mirs {
            hirs.push(Hir51::from(mir.clone()));
        }

        hirs
    }

    fn extract_macro_block(
        &self,
        header: &StructuredBlock,
        body_blocks: &[StructuredBlock],
    ) -> Vec<Hir51> {
        let BlockKind::Flat(ref header_mirs) = header.kind else {
            unreachable!()
        };

        let last_instruction = header_mirs.last().unwrap();

        let loop_condition = match last_instruction {
            Mir51::Goto(block_id) if *block_id == header.id => Expr::Constant(1),
            Mir51::If {
                cond,
                true_jmp,
                false_jmp,
            } => cond.clone(),
            _ => unreachable!(),
        };

        let mut loop_body = Vec::new();
        for block in body_blocks {
            loop_body.extend(self.extract_block(block));
        }

        loop_body.pop(); // Remove last goto of while loop

        let while_loop = Hir51::While {
            cond: loop_condition,
            body: loop_body,
        };

        vec![while_loop]
    }
}
