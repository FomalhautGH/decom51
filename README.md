# Decom51 Roadmap

## Description of the project
Decom51 is a custom decompilation pipeline. The primary goal of this decompiler is to translate low-level 8051 machine code back into high-level, human-readable C code. 

While general-purpose decompilers (like Ghidra or Hex-Rays) are incredibly powerful, they must support hundreds of architectures. This often leads them to fall back on generic Intermediate Representations (like P-Code) that can produce verbose, unreadable C code when dealing with architecture-specific quirks—such as ALU carry/borrow flags or specialized conditional jumps. 

By building a specialized pipeline, this decompiler deeply understands the semantic intent behind specific subsets of instructions. It leverages advanced compiler theory—such as Static Single Assignment (SSA) form, Lattice Theory, and topological graph structuring—to aggressively optimize away low-level noise and reconstruct clean, idiomatic C constructs that look exactly like what a human would have originally written.

## Pipeline Phases

### 🟢 Phase 1: Frontend & Lifting (Completed)
The foundation of the pipeline. It strips away the raw binary format and abstract machine instructions into a clean, platform-independent representation.
- [x] **Binary Parsing & Disassembly (`syntax`):** Reads the raw binary and parses it into assembly instructions.
- [x] **IR Lifting:** Translates native opcodes into a custom Intermediate Representation (IR).

### 🟢 Phase 2: Control Flow & SSA Form (Completed)
Transforms a flat list of instructions into a strict mathematical model where data flow can be formally analyzed.
- [x] **Control Flow Graph (CFG) Construction:** Builds a directed graph mapping out all basic blocks, jumps, and execution paths.
- [x] **Dominance Analysis:** Calculates the Dominator Tree and Dominance Frontiers to understand the exact topological hierarchy of the code.
- [x] **SSA (Static Single Assignment) Generation:** Assigns strict versioning to every variable mutation and algorithmically places Phi (Φ) nodes at control flow convergence points. Every variable is now written exactly once.

### 🟡 Phase 3: Analysis & Optimization (In Progress)
The "brain" of the decompiler. It simulates the program mathematically to discover absolute truths and simplifies the graph accordingly.
- [x] **SCCP (Sparse Conditional Constant Propagation):** A complex dataflow analysis engine using Lattice Theory to track variables across the CFG, discovering absolute constants, predictable math, and unreachable branches.
- [x] **IR Rewriter & Constant Folding:** A mutation pass that acts on the SCCP's findings. It physically replaces SSA variables with literal constants, pre-calculates mathematical operations, and converts static conditional branches (`BranchIf`) into direct `Jump`s.
- [ ] **Dead Code Elimination (DCE):** The cleanup pass. It sweeps through Def-Use chains to remove "orphan" math operations (where the result is never read) and prunes entirely disconnected basic blocks left over from the Rewriter.

### 🔴 Phase 4: Recovery & Aggregation (To Do)
The bridge between machine logic and human logic. It hides the SSA mechanics and rebuilds complex mathematical expressions.
- [ ] **De-SSA & Variable Coalescing:** Performs *Liveness Analysis* and builds Interference Graphs to figure out which SSA versions can share the same memory space, collapsing dozens of versions (e.g., `R7_1`, `R7_2`) back into standard local C variables.
- [ ] **Expression Aggregation:** Walks backward up the Def-Use chains to collapse vertical TAC instructions into horizontal, nested C expressions (transforming `t1 = a + b; t2 = t1 * c; res = t2;` directly into `res = (a + b) * c;`).

### 🔴 Phase 5: Control Flow Structuring (To Do)
The core challenge of decompilation. It pattern-matches the topology of the CFG to resurrect high-level control flow syntax.
- [ ] **Loop Recovery:** Analyzes *backedges* and *loop headers* in the Dominator Tree to detect and reconstruct `while`, `do-while`, and `for` loops.
- [ ] **Condition Recovery:** Maps branch divergence and convergence points to reconstruct logical `if`, `else if`, and `else` blocks, removing `goto`/`jump` statements entirely.
- [ ] **AST Generation:** Converts the topological graph into an Abstract Syntax Tree (AST), the final hierarchical representation of the C code.

### 🔴 Phase 6: Code Emission (To Do)
The final output stage.
- [ ] **C Code Generation:** Traverses the final AST to emit formatted, highly readable C source code, applying proper indentation, variable typing, and syntax sugar.
