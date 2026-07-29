use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};

use crate::{
    common::BlockId,
    mir::Mir51,
    opt::cfg::{BasicBlock, ControlFlowGraph},
};

#[derive(Debug)]
pub enum BlockKind {
    Flat(Vec<Mir51>),
    Macro {
        header: Box<StructuredBlock>,
        blocks: Vec<StructuredBlock>,
    },
}

#[derive(Debug)]
pub struct StructuredBlock {
    pub id: BlockId,
    pub successors: Vec<BlockId>,
    pub predecessors: Vec<BlockId>,
    pub kind: BlockKind,
}

impl StructuredBlock {
    pub fn from(block: BasicBlock) -> Self {
        Self {
            id: block.id,
            successors: block.successors,
            predecessors: block.predecessors,
            kind: BlockKind::Flat(block.mirs),
        }
    }
}

impl Display for StructuredBlock {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let indent = f.width().unwrap_or(0);
        let indent_str = " ".repeat(indent);

        match &self.kind {
            BlockKind::Flat(mirs) => {
                writeln!(f, "{}Block {}:", indent_str, self.id)?;
                writeln!(f, "{} Predecessors: {:?}", indent_str, self.predecessors)?;
                writeln!(f, "{} Successors: {:?}", indent_str, self.successors)?;

                if !mirs.is_empty() {
                    writeln!(f, "{} Middle IR Instructions:", indent_str)?;
                    for (i, mir) in mirs.iter().enumerate() {
                        writeln!(f, "{} {}: {}", indent_str, i, mir)?;
                    }
                }
            }
            BlockKind::Macro { header, blocks } => {
                writeln!(f, "{}Macro Block {}:", indent_str, self.id)?;
                writeln!(f, "{} Predecessors: {:?}", indent_str, self.predecessors)?;
                writeln!(f, "{} Successors: {:?}", indent_str, self.successors)?;

                writeln!(f, "{} Header:", indent_str)?;
                write!(f, "{:width$}", header, width = indent + 4)?;

                if !blocks.is_empty() {
                    writeln!(f, "{} Body Blocks:", indent_str)?;
                    for child in blocks {
                        write!(f, "{:width$}", child, width = indent + 4)?;
                    }
                }
            }
        }

        Ok(())
    }
}

pub type StructuredBlockMap = HashMap<BlockId, StructuredBlock>;

#[derive(Debug)]
pub struct StructuredCFG {
    pub blocks: StructuredBlockMap,
    pub start_block: BlockId,
}

impl StructuredCFG {
    pub fn from(cfg: ControlFlowGraph) -> Self {
        let mut flat_blocks: StructuredBlockMap = HashMap::new();

        for block in cfg.blocks.into_values() {
            let block = StructuredBlock::from(block);
            flat_blocks.insert(block.id, block);
        }

        Self {
            blocks: flat_blocks,
            start_block: cfg.start_block,
        }
    }
}

// TODO: Maybe do the same thind for the normal CFG and remove all that debug_assertions
// messy thing
impl Display for StructuredCFG {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut sorted_blocks: Vec<&StructuredBlock> = self.blocks.values().collect();
        sorted_blocks.sort_by_key(|b| b.id);
        for block in sorted_blocks {
            write!(f, "{}", block)?;
        }
        Ok(())
    }
}
