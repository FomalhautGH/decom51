use std::collections::{HashMap, HashSet};

use crate::common::{AddrSpace, BinOp, Ir51, Operand, Register};
use crate::syntax::{Argument, Instruction};

#[derive(Default)]
pub struct Lifter {
    label_to_instruction_idx: HashMap<usize, usize>,
    current_ir_offset: usize,
}

impl Lifter {
    // Public Lifting Pipeline API
    pub fn lift_program(&mut self, instructions: &[Instruction]) -> Vec<Ir51> {
        let mut low_ir_stream = vec![];

        for instr in instructions {
            if *instr == Instruction::End {
                break;
            }

            let lifted_irs = self.lift_instruction(*instr);
            self.current_ir_offset += lifted_irs.len();

            low_ir_stream.extend(lifted_irs);
        }

        self.resolve_jump_targets(&mut low_ir_stream);
        self.contract_jump_edges(&mut low_ir_stream);

        if cfg!(debug_assertions) {
            println!("======================== IR CODE ========================");
            for (i, ir) in low_ir_stream.iter().enumerate() {
                println!("{i}: {ir}")
            }
        }

        low_ir_stream
    }

    // Pipeline Passes (Jump Resolution, Edge Contraction & Index Remapping)
    fn resolve_jump_targets(&self, low_ir_stream: &mut [Ir51]) {
        for ir in low_ir_stream.iter_mut() {
            let new_ir = match ir {
                Ir51::BranchIf {
                    cond,
                    true_jmp,
                    false_jmp,
                } => Ir51::BranchIf {
                    cond: *cond,
                    true_jmp: *self
                        .label_to_instruction_idx
                        .get(true_jmp)
                        .expect("Label not found"),
                    false_jmp: *false_jmp,
                },

                Ir51::Jump(label) => Ir51::Jump(
                    *self
                        .label_to_instruction_idx
                        .get(label)
                        .expect("Label not found"),
                ),

                _ => *ir,
            };

            *ir = new_ir;
        }
    }

    fn contract_jump_edges(&mut self, low_ir_stream: &mut Vec<Ir51>) {
        for i in 0..low_ir_stream.len() {
            let mut ir = low_ir_stream[i];

            // TODO: always check for other instructions like these
            match &mut ir {
                Ir51::BranchIf {
                    true_jmp,
                    false_jmp,
                    ..
                } => {
                    *true_jmp = Lifter::try_resolve_unconditional_jump(*true_jmp, low_ir_stream)
                        .unwrap_or(*true_jmp);
                    *false_jmp = Lifter::try_resolve_unconditional_jump(*false_jmp, low_ir_stream)
                        .unwrap_or(*false_jmp);
                }

                Ir51::Jump(label) => {
                    *label = Lifter::try_resolve_unconditional_jump(*label, low_ir_stream)
                        .unwrap_or(*label);
                }

                _ => continue,
            }

            low_ir_stream[i] = ir;
        }

        self.eliminate_unreachable_jumps(low_ir_stream);
        self.compact_and_remap_jumps(low_ir_stream);
    }

    fn eliminate_unreachable_jumps(&mut self, low_ir_stream: &mut [Ir51]) -> Option<()> {
        let reachable_jump_targets = self.collect_reachable_jump_targets(low_ir_stream);

        for i in 1..low_ir_stream.len() {
            let prev_i = i - 1;
            let [prev_ir, ir] = low_ir_stream.get_disjoint_mut([prev_i, i]).ok()?;

            match ir {
                Ir51::Jump(label) => {
                    if !reachable_jump_targets.contains(label) && prev_ir.is_flow_terminator() {
                        *ir = Ir51::Nop;
                    }
                }

                Ir51::BranchIf { .. } => {}
                Ir51::Move { .. } => {}
                Ir51::BinOp { .. } => {}
                Ir51::Store { .. } => {}
                Ir51::Load { .. } => {}
                Ir51::Call(_) => {}
                Ir51::Return => {}
                Ir51::Nop => {}
            }
        }

        Some(())
    }

    fn collect_reachable_jump_targets(&self, low_ir_stream: &[Ir51]) -> HashSet<usize> {
        let mut reachable_jump_targets = HashSet::new();

        for ir in low_ir_stream {
            match ir {
                Ir51::BranchIf {
                    true_jmp,
                    false_jmp,
                    ..
                } => {
                    reachable_jump_targets.insert(*true_jmp);
                    reachable_jump_targets.insert(*false_jmp);
                }

                Ir51::Jump(label) => {
                    reachable_jump_targets.insert(*label);
                }

                Ir51::Move { .. } => {}
                Ir51::BinOp { .. } => {}
                Ir51::Store { .. } => {}
                Ir51::Load { .. } => {}
                Ir51::Call(_) => {}
                Ir51::Return => {}
                Ir51::Nop => {}
            }
        }

        reachable_jump_targets
    }

    fn compact_and_remap_jumps(&mut self, low_ir_stream: &mut Vec<Ir51>) {
        let mut compacted_index_map = HashMap::new();
        let mut real_index = 0;

        // We track the shifted indices before retaining
        for (i, ir) in low_ir_stream.iter().enumerate() {
            if !matches!(ir, Ir51::Nop) {
                compacted_index_map.insert(i, real_index);
                real_index += 1;
            }
        }

        // Retain
        low_ir_stream.retain(|ir| !matches!(ir, Ir51::Nop));

        // Patching
        for ir in low_ir_stream {
            match ir {
                Ir51::BranchIf {
                    true_jmp,
                    false_jmp,
                    ..
                } => {
                    *true_jmp = compacted_index_map[true_jmp];
                    *false_jmp = compacted_index_map[false_jmp];
                }

                Ir51::Jump(label) => {
                    *label = compacted_index_map[label];
                }

                Ir51::Move { .. } => {}
                Ir51::BinOp { .. } => {}
                Ir51::Store { .. } => {}
                Ir51::Load { .. } => {}
                Ir51::Call(_) => {}
                Ir51::Return => {}
                Ir51::Nop => {}
            }
        }
    }

    fn try_resolve_unconditional_jump(
        jmp_index: usize,
        low_ir_stream: &mut [Ir51],
    ) -> Option<usize> {
        let ir = low_ir_stream.get_mut(jmp_index)?;

        if let Ir51::Jump(jmp) = *ir {
            Some(jmp)
        } else {
            None
        }
    }

    // Single Instruction Dispatcher
    fn lift_instruction(&mut self, instr: Instruction) -> Vec<Ir51> {
        match instr {
            Instruction::Label(label) => {
                self.label_to_instruction_idx
                    .insert(label, self.current_ir_offset);
                vec![]
            }

            Instruction::Lcall(label) => vec![Ir51::Call(label)],
            Instruction::Mov(dest, src) => self.lift_mov(dest, src),
            Instruction::Clr(dest) => self.lift_mov(dest, Argument::Data8(0)),
            Instruction::Inc(dest) => self.lift_inc(dest),
            Instruction::Dec(dest) => self.lift_dec(dest),
            Instruction::Add(dest, op) => self.lift_add(dest, op),
            Instruction::Addc(dest, op) => self.lift_addc(dest, op),
            Instruction::Subb(dest, op) => self.lift_subb(dest, op),
            Instruction::Anl(dest, op) => self.lift_anl(dest, op),
            Instruction::Orl(dest, op) => self.lift_orl(dest, op),
            Instruction::Xrl(dest, op) => self.lift_xrl(dest, op),
            Instruction::MulAb => self.lift_mul_ab(),
            Instruction::DivAb => self.lift_div_ab(),
            Instruction::Jc(label) => self.lift_jc(label),
            Instruction::Jnc(label) => self.lift_jnc(label),
            Instruction::Jz(label) => self.lift_jz(label),
            Instruction::Jnz(label) => self.lift_jnz(label),
            Instruction::Ljmp(label) => self.lift_jump(label),
            Instruction::Sjmp(label) => self.lift_jump(label),
            Instruction::Cjne(lhs, rhs, label) => self.lift_cjne(lhs, rhs, label),
            Instruction::Push(ram_addr) => self.lift_push(ram_addr),
            Instruction::Pop(ram_addr) => self.lift_pop(ram_addr),
            Instruction::Ret => vec![Ir51::Return],
            Instruction::Db(_) => vec![],
            _ => todo!("{instr:?}"),
        }
    }

    // Data Movement & Stack Operations
    fn lift_mov(&mut self, dest_arg: Argument, src_arg: Argument) -> Vec<Ir51> {
        let dest_operand: Operand = match dest_arg {
            Argument::IramAddress(addr) => Operand::Var {
                addr: addr.into(),
                space: AddrSpace::iData,
                size: 1,
            },
            Argument::Reg(reg) => Operand::Register(reg),
            _ => todo!("{dest_arg:?}"),
        };

        let src_operand: Operand = match src_arg {
            Argument::Data8(imm8) => Operand::Constant(imm8.into()),
            Argument::Data16(imm16) => Operand::Constant(imm16),
            Argument::IramAddress(addr) => Operand::Var {
                addr: addr.into(),
                space: AddrSpace::iData,
                size: 1,
            },
            Argument::Reg(reg) => Operand::Register(reg),
            _ => todo!("{src_arg:?}"),
        };

        vec![Ir51::Move {
            dest: dest_operand,
            src: src_operand,
        }]
    }

    fn lift_push(&mut self, ram_addr: u8) -> Vec<Ir51> {
        Ir51::generate_push(
            Operand::Var {
                addr: ram_addr.into(),
                space: AddrSpace::iData,
                size: 1,
            },
            1,
        )
    }

    fn lift_pop(&mut self, ram_addr: u8) -> Vec<Ir51> {
        Ir51::generate_pop(
            Operand::Var {
                addr: ram_addr.into(),
                space: AddrSpace::iData,
                size: 1,
            },
            1,
        )
    }

    // Arithmetic Operations
    fn lift_add(&mut self, dest_arg: Argument, src_arg: Argument) -> Vec<Ir51> {
        let dest_operand: Operand = match dest_arg {
            Argument::Reg(Register::A) => Operand::Register(Register::A),
            _ => todo!("{dest_arg:?}"),
        };

        let src_operand: Operand = match src_arg {
            Argument::Reg(reg) => Operand::Register(reg),
            Argument::IramAddress(addr) => Operand::Var {
                addr: addr.into(),
                space: AddrSpace::iData,
                size: 1,
            },
            Argument::Data8(imm8) => Operand::Constant(imm8.into()),
            _ => todo!("{src_arg:?}"),
        };

        vec![
            Ir51::BinOp {
                op: BinOp::Add,
                dest: Operand::Temp(0),
                lhs: dest_operand,
                rhs: src_operand,
            },
            Ir51::BinOp {
                op: BinOp::Gt,
                dest: Operand::Register(Register::C),
                lhs: Operand::Temp(0),
                rhs: Operand::Constant((1 << 8) - 1),
            },
            Ir51::Move {
                dest: dest_operand,
                src: Operand::Temp(0),
            },
        ]
    }

    fn lift_addc(&mut self, dest_arg: Argument, src_arg: Argument) -> Vec<Ir51> {
        let dest_operand: Operand = match dest_arg {
            Argument::Reg(Register::A) => Operand::Register(Register::A),
            _ => todo!("{dest_arg:?}"),
        };

        let src_operand: Operand = match src_arg {
            Argument::Reg(reg) => Operand::Register(reg),
            Argument::IramAddress(addr) => Operand::Var {
                addr: addr.into(),
                space: AddrSpace::iData,
                size: 1,
            },
            Argument::Data8(imm8) => Operand::Constant(imm8.into()),
            _ => todo!("{src_arg:?}"),
        };

        vec![Ir51::BinOp {
            op: BinOp::Addc,
            dest: dest_operand,
            lhs: dest_operand,
            rhs: src_operand,
        }]
    }

    fn lift_subb(&mut self, dest_arg: Argument, src_arg: Argument) -> Vec<Ir51> {
        let dest_operand: Operand = match dest_arg {
            Argument::Reg(reg) => Operand::Register(reg),
            Argument::IramAddress(addr) => Operand::Var {
                addr: addr.into(),
                space: AddrSpace::iData,
                size: 1,
            },
            _ => todo!("{dest_arg:?}"),
        };

        let src_operand: Operand = match src_arg {
            Argument::Reg(reg) => Operand::Register(reg),
            Argument::IramAddress(addr) => Operand::Var {
                addr: addr.into(),
                space: AddrSpace::iData,
                size: 1,
            },
            Argument::Data8(imm8) => Operand::Constant(imm8.into()),
            _ => todo!("{src_arg:?}"),
        };

        vec![
            Ir51::BinOp {
                op: BinOp::Add,
                dest: Operand::Temp(0),
                lhs: src_operand,
                rhs: Operand::Register(Register::C),
            },
            Ir51::BinOp {
                op: BinOp::Gt,
                dest: Operand::Register(Register::C),
                lhs: Operand::Temp(0),
                rhs: Operand::Register(Register::A),
            },
            Ir51::BinOp {
                op: BinOp::Subb,
                dest: dest_operand,
                lhs: dest_operand,
                rhs: src_operand,
            },
        ]
    }

    fn lift_inc(&mut self, dest_arg: Argument) -> Vec<Ir51> {
        let dest_operand: Operand = match dest_arg {
            Argument::Reg(reg) => Operand::Register(reg),
            Argument::IramAddress(addr) => Operand::Var {
                addr: addr.into(),
                space: AddrSpace::iData,
                size: 1,
            },
            _ => todo!("{dest_arg:?}"),
        };

        vec![Ir51::BinOp {
            op: BinOp::Add,
            dest: dest_operand,
            lhs: dest_operand,
            rhs: Operand::Constant(1),
        }]
    }

    fn lift_dec(&mut self, dest_arg: Argument) -> Vec<Ir51> {
        let dest_operand: Operand = match dest_arg {
            Argument::Reg(reg) => Operand::Register(reg),
            Argument::IramAddress(addr) => Operand::Var {
                addr: addr.into(),
                space: AddrSpace::iData,
                size: 1,
            },
            _ => todo!("{dest_arg:?}"),
        };

        vec![Ir51::BinOp {
            op: BinOp::Sub,
            dest: dest_operand,
            lhs: dest_operand,
            rhs: Operand::Constant(1),
        }]
    }

    fn lift_mul_ab(&mut self) -> Vec<Ir51> {
        vec![Ir51::BinOp {
            op: BinOp::Mul,
            dest: Operand::Register(Register::A),
            lhs: Operand::Register(Register::A),
            rhs: Operand::Register(Register::B),
        }]
    }

    fn lift_div_ab(&mut self) -> Vec<Ir51> {
        vec![Ir51::BinOp {
            op: BinOp::Div,
            dest: Operand::Register(Register::A),
            lhs: Operand::Register(Register::A),
            rhs: Operand::Register(Register::B),
        }]
    }

    // Bitwise / Logical Operations
    fn lift_anl(&mut self, dest_arg: Argument, src_arg: Argument) -> Vec<Ir51> {
        let dest_operand: Operand = match dest_arg {
            Argument::Reg(reg) => Operand::Register(reg),
            Argument::IramAddress(addr) => Operand::Var {
                addr: addr.into(),
                space: AddrSpace::iData,
                size: 1,
            },
            _ => todo!("{dest_arg:?}"),
        };

        let src_operand: Operand = match src_arg {
            Argument::Reg(reg) => Operand::Register(reg),
            Argument::IramAddress(addr) => Operand::Var {
                addr: addr.into(),
                space: AddrSpace::iData,
                size: 1,
            },
            Argument::Data8(imm) => Operand::Constant(imm.into()),
            _ => todo!("{src_arg:?}"),
        };

        vec![Ir51::BinOp {
            op: BinOp::And,
            dest: dest_operand,
            lhs: dest_operand,
            rhs: src_operand,
        }]
    }

    fn lift_orl(&mut self, dest_arg: Argument, src_arg: Argument) -> Vec<Ir51> {
        let dest_operand: Operand = match dest_arg {
            Argument::Reg(reg) => Operand::Register(reg),
            Argument::IramAddress(addr) => Operand::Var {
                addr: addr.into(),
                space: AddrSpace::iData,
                size: 1,
            },
            _ => todo!("{dest_arg:?}"),
        };

        let src_operand: Operand = match src_arg {
            Argument::Reg(reg) => Operand::Register(reg),
            Argument::IramAddress(addr) => Operand::Var {
                addr: addr.into(),
                space: AddrSpace::iData,
                size: 1,
            },
            Argument::Data8(imm) => Operand::Constant(imm.into()),
            _ => todo!("{src_arg:?}"),
        };

        vec![Ir51::BinOp {
            op: BinOp::Or,
            dest: dest_operand,
            lhs: dest_operand,
            rhs: src_operand,
        }]
    }

    fn lift_xrl(&mut self, dest_arg: Argument, src_arg: Argument) -> Vec<Ir51> {
        let dest_operand: Operand = match dest_arg {
            Argument::Reg(reg) => Operand::Register(reg),
            Argument::IramAddress(addr) => Operand::Var {
                addr: addr.into(),
                space: AddrSpace::iData,
                size: 1,
            },
            _ => todo!("{dest_arg:?}"),
        };

        let src_operand: Operand = match src_arg {
            Argument::Reg(reg) => Operand::Register(reg),
            Argument::IramAddress(addr) => Operand::Var {
                addr: addr.into(),
                space: AddrSpace::iData,
                size: 1,
            },
            Argument::Data8(imm) => Operand::Constant(imm.into()),
            _ => todo!("{src_arg:?}"),
        };

        vec![Ir51::BinOp {
            op: BinOp::Xor,
            dest: dest_operand,
            lhs: dest_operand,
            rhs: src_operand,
        }]
    }

    // Control Flow / Branching Operations
    fn lift_jump(&mut self, target_label: usize) -> Vec<Ir51> {
        vec![Ir51::Jump(target_label)]
    }

    fn lift_jc(&mut self, target_label: usize) -> Vec<Ir51> {
        let true_target_offset = target_label;
        let false_target_offset = self.current_ir_offset + 2;

        vec![
            Ir51::BinOp {
                op: BinOp::Ne,
                dest: Operand::Temp(0),
                lhs: Operand::Register(Register::C),
                rhs: Operand::Constant(0),
            },
            Ir51::BranchIf {
                cond: Operand::Temp(0),
                true_jmp: true_target_offset,
                false_jmp: false_target_offset,
            },
        ]
    }

    fn lift_jnc(&mut self, target_label: usize) -> Vec<Ir51> {
        let true_target_offset = target_label;
        let false_target_offset = self.current_ir_offset + 2;

        vec![
            Ir51::BinOp {
                op: BinOp::Eq,
                dest: Operand::Temp(0),
                lhs: Operand::Register(Register::C),
                rhs: Operand::Constant(0),
            },
            Ir51::BranchIf {
                cond: Operand::Temp(0),
                true_jmp: true_target_offset,
                false_jmp: false_target_offset,
            },
        ]
    }

    fn lift_jz(&mut self, target_label: usize) -> Vec<Ir51> {
        let true_target_offset = target_label;
        let false_target_offset = self.current_ir_offset + 2;

        vec![
            Ir51::BinOp {
                op: BinOp::Eq,
                dest: Operand::Temp(0),
                lhs: Operand::Register(Register::A),
                rhs: Operand::Constant(0),
            },
            Ir51::BranchIf {
                cond: Operand::Temp(0),
                true_jmp: true_target_offset,
                false_jmp: false_target_offset,
            },
        ]
    }

    fn lift_jnz(&mut self, target_label: usize) -> Vec<Ir51> {
        let true_target_offset = target_label;
        let false_target_offset = self.current_ir_offset + 2;

        vec![
            Ir51::BinOp {
                op: BinOp::Ne,
                dest: Operand::Temp(0),
                lhs: Operand::Register(Register::A),
                rhs: Operand::Constant(0),
            },
            Ir51::BranchIf {
                cond: Operand::Temp(0),
                true_jmp: true_target_offset,
                false_jmp: false_target_offset,
            },
        ]
    }

    fn lift_cjne(
        &mut self,
        lhs_arg: Argument,
        rhs_arg: Argument,
        target_label: usize,
    ) -> Vec<Ir51> {
        let lhs_operand: Operand = match lhs_arg {
            Argument::Reg(reg) => Operand::Register(reg),
            _ => todo!("{lhs_arg:?}"),
        };

        let rhs_operand: Operand = match rhs_arg {
            Argument::Data8(imm8) => Operand::Constant(imm8.into()),
            Argument::IramAddress(addr) => Operand::Var {
                addr: addr.into(),
                space: AddrSpace::iData,
                size: 1,
            },
            _ => todo!("{rhs_arg:?}"),
        };

        let true_target_offset = target_label;
        let false_target_offset = self.current_ir_offset + 3; // cuz cjne is equivalent to 3 ir instuctions

        // TODO: Clear carry if lhs is >= rhs?
        vec![
            Ir51::BinOp {
                op: BinOp::Ne,
                dest: Operand::Temp(0),
                lhs: lhs_operand,
                rhs: rhs_operand,
            },
            Ir51::BinOp {
                op: BinOp::Lt,
                dest: Operand::Register(Register::C),
                lhs: lhs_operand,
                rhs: rhs_operand,
            },
            Ir51::BranchIf {
                cond: Operand::Temp(0),
                true_jmp: true_target_offset,
                false_jmp: false_target_offset,
            },
        ]
    }
}
