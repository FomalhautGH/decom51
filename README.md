# Decom51 Roadmap

## Description of the project
Decom51 is a custom decompilation pipeline. The primary goal of this decompiler is to translate low-level 8051 assembly code back into high-level, human-readable C code. 

While general-purpose decompilers (like Ghidra or Hex-Rays) are incredibly powerful, they must support hundreds of architectures. This often leads them to fall back on generic Intermediate Representations (like P-Code) that can produce verbose, unreadable C code when dealing with architecture-specific quirks—such as ALU carry/borrow flags or specialized conditional jumps. 

By building a specialized pipeline, this decompiler deeply understands the semantic intent behind specific subsets of instructions. It leverages advanced compiler theory—such as Static Single Assignment (SSA) form, Lattice Theory, and topological graph structuring—to aggressively optimize away low-level noise and reconstruct clean, idiomatic C constructs that look exactly like what a human would have originally written.

## Pipeline Phases

### 🟢 Phase 1: Frontend & Lifting (Completed)
The foundation of the pipeline. It parses assembly text files and lifts them into a custom, platform-independent Intermediate Representation.
- [x] **Assembly Parsing (`syntax`):** Reads the disassembly file content, tokenizing it via [lexer.rs](src/syntax/lexer.rs) and parsing it into native 8051 AST instructions using [parser.rs](src/syntax/parser.rs).
- [x] **IR Lifting (`lifting/lifter.rs`):** Translates native 8051 AST instructions into a custom Intermediate Representation ([Ir51](src/common.rs#L97)).
- [x] **Memory Normalization (`lifting/mem_normalizer.rs`):** Normalizes register operands (e.g., `R0-R7`, `A`, `B`, `SP`, `DPTR`, etc.) by mapping them to their physical address equivalents in the 8051 direct/indirect internal RAM space (`iData`), simplifying subsequent dataflow analysis.

### 🟢 Phase 2: Control Flow & SSA Form (Completed)
Transforms the flat list of IR instructions into a topological graph model where dataflow can be formally analyzed and tracked.
- [x] **Control Flow Graph (CFG) Construction (`opt/cfg_restorer.rs`):** Reconstructs the CFG by identifying basic block leaders, partitioning instructions into distinct [BasicBlock](src/opt/cfg_restorer.rs#L8) structures that directly own their instructions (`Vec<Ir51>`), and resolving execution paths (successors/predecessors).
- [x] **Dominance Analysis (`opt/ssa/chk_algorithm.rs`):** Calculates immediate dominators using the Cooper-Harvey-Kennedy (CHK) algorithm, building the dominator tree and dominance frontiers.
- [x] **SSA Generation (`opt/ssa/mod.rs`):** Algorithmically inserts Phi (Φ) nodes at dominance frontiers and renames variables and temporaries into versioned operands (`SSAVar` and `TempSSAVar`).

### 🟢 Phase 3: Analysis & Optimization (Completed)
The analysis engine of the decompiler. It computes variable state properties and optimizes the CFG by removing low-level noise.
- [x] **SCCP (Sparse Conditional Constant Propagation) (`opt/const_propagation.rs`):** A dataflow analysis engine using Lattice Theory (Top, Constant, Bottom) to discover absolute constants, predictable math operations, and static conditional jump branches.
- [x] **IR Rewriter & Constant Folding (`opt/rewriter.rs`):** A mutation pass acting on constant propagation results, replacing versioned SSA operands with literal constants, folding math operations, and converting conditional branches into direct jumps.
- [x] **Dead Block Elimination (DBE) (`opt/dbe.rs`):** Reaches out to the CFG blocks and prunes unreachable basic blocks, allowing their instructions to be cleaned up automatically.
- [x] **Dead Code Elimination (DCE) (`opt/dce.rs`):** Employs Def-Use chains to find unused operations, deleting dead instructions from basic block vectors and filtering out residual `Nop` instructions.

### 🟡 Phase 4: Recovery & Aggregation (In progress)
The bridge between machine logic and human logic. It hides the SSA mechanics and rebuilds complex mathematical expressions.
- [x] **De-SSA & Variable Coalescing:** Performs *Liveness Analysis* and builds Interference Graphs to figure out which SSA versions can share the same memory space, collapsing dozens of versions (e.g., `R7_1`, `R7_2`) back into standard local C variables.
- [x] **Variable Width Aggregation (High/Low Byte Merging):** Identifies pairs of 8-bit variables representing the high and low parts of the same 16-bit variable (a very common occurrence in 8051 code, e.g., combining `DPL` and `DPH` or adjacent registers) and merges them into single 16-bit variables.
- [ ] **Expression Aggregation:** Walks backward up the Def-Use chains to collapse vertical TAC instructions into horizontal, nested C expressions (transforming `t1 = a + b; t2 = t1 * c; res = t2;` directly into `res = (a + b) * c;`).

### 🔴 Phase 5: Control Flow Structuring (To Do)
The core challenge of decompilation. It pattern-matches the topology of the CFG to resurrect high-level control flow syntax.
- [ ] **Loop Recovery:** Analyzes *backedges* and *loop headers* in the Dominator Tree to detect and reconstruct `while`, `do-while`, and `for` loops.
- [ ] **Condition Recovery:** Maps branch divergence and convergence points to reconstruct logical `if`, `else if`, and `else` blocks, removing `goto`/`jump` statements entirely.
- [ ] **AST Generation:** Converts the topological graph into an Abstract Syntax Tree (AST), the final hierarchical representation of the C code.

### 🔴 Phase 6: Code Emission (To Do)
The final output stage.
- [ ] **C Code Generation:** Traverses the final AST to emit formatted, highly readable C source code, applying proper indentation, variable typing, and syntax sugar.
