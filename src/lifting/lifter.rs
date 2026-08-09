use std::collections::{HashMap, HashSet};

use crate::common::{AddrSpace, BinOp, Ir51, Operand, Register};
use crate::syntax::{Argument, Instruction};

pub struct Lifter {
    labels_index: HashMap<usize, usize>,
    current_index: usize,
}

impl Lifter {
    pub fn new() -> Self {
        Self {
            labels_index: HashMap::new(),
            current_index: 0,
        }
    }

    pub fn get_ir_instructions(&mut self, instructions: &[Instruction]) -> Vec<Ir51> {
        let mut ir_code = vec![];

        for instr in instructions {
            if *instr == Instruction::End {
                break;
            }

            let irs = self.get_ir_from_instruction(*instr);
            self.current_index += irs.len();

            ir_code.extend(irs);
        }

        self.fix_jmps(&mut ir_code);
        self.edge_contraction(&mut ir_code);

        if cfg!(debug_assertions) {
            println!("======================== IR CODE ========================");
            for (i, ir) in ir_code.iter().enumerate() {
                println!("{i}: {ir}")
            }
        }

        ir_code
    }

    fn fix_jmps(&self, ir_code: &mut [Ir51]) {
        for ir in ir_code.iter_mut() {
            let new_ir = match ir {
                Ir51::BranchIf {
                    cond,
                    true_jmp,
                    false_jmp,
                } => Ir51::BranchIf {
                    cond: *cond,
                    true_jmp: *self.labels_index.get(true_jmp).expect("Label not found"),
                    false_jmp: *false_jmp,
                },

                Ir51::Jump(label) => {
                    Ir51::Jump(*self.labels_index.get(label).expect("Label not found"))
                }

                _ => *ir,
            };

            *ir = new_ir;
        }
    }

    fn edge_contraction(&mut self, ir_code: &mut Vec<Ir51>) {
        for i in 0..ir_code.len() {
            let mut ir = ir_code[i];

            // TODO: always check for other instructions like these
            match &mut ir {
                Ir51::BranchIf {
                    true_jmp,
                    false_jmp,
                    ..
                } => {
                    *true_jmp = Lifter::is_single_ljmp(*true_jmp, ir_code).unwrap_or(*true_jmp);
                    *false_jmp = Lifter::is_single_ljmp(*false_jmp, ir_code).unwrap_or(*false_jmp);
                }

                Ir51::Jump(label) => {
                    *label = Lifter::is_single_ljmp(*label, ir_code).unwrap_or(*label);
                }

                _ => continue,
            }

            ir_code[i] = ir;
        }

        self.invalid_jumps(ir_code);
        self.patch_jumps(ir_code);
    }

    fn invalid_jumps(&mut self, ir_code: &mut Vec<Ir51>) -> Option<()> {
        let saved_jumps = self.save_jumps(ir_code);

        for i in 1..ir_code.len() {
            let prev_i = i - 1;
            let [prev_ir, ir] = ir_code.get_disjoint_mut([prev_i, i]).ok()?;

            match ir {
                Ir51::Jump(label) => {
                    if !saved_jumps.contains(label) && prev_ir.is_flow_terminator() {
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

    fn save_jumps(&self, ir_code: &Vec<Ir51>) -> HashSet<usize> {
        let mut saved_jumps = HashSet::new();

        for ir in ir_code {
            match ir {
                Ir51::BranchIf {
                    true_jmp,
                    false_jmp,
                    ..
                } => {
                    saved_jumps.insert(*true_jmp);
                    saved_jumps.insert(*false_jmp);
                }

                Ir51::Jump(label) => {
                    saved_jumps.insert(*label);
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

        saved_jumps
    }

    fn patch_jumps(&mut self, ir_code: &mut Vec<Ir51>) {
        let mut index_map = HashMap::new();
        let mut real_index = 0;

        // We track the shifted indices before retaining
        for (i, ir) in ir_code.iter().enumerate() {
            if !matches!(ir, Ir51::Nop) {
                index_map.insert(i, real_index);
                real_index += 1;
            }
        }

        // Retain
        ir_code.retain(|ir| !matches!(ir, Ir51::Nop));

        // Pathing
        for ir in ir_code {
            match ir {
                Ir51::BranchIf {
                    true_jmp,
                    false_jmp,
                    ..
                } => {
                    *true_jmp = index_map[true_jmp];
                    *false_jmp = index_map[false_jmp];
                }

                Ir51::Jump(label) => {
                    *label = index_map[label];
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

    fn is_single_ljmp(jmp_index: usize, ir_code: &mut [Ir51]) -> Option<usize> {
        let ir = ir_code.get_mut(jmp_index)?;

        if let Ir51::Jump(jmp) = *ir {
            Some(jmp)
        } else {
            None
        }
    }

    fn get_ir_from_instruction(&mut self, instr: Instruction) -> Vec<Ir51> {
        match instr {
            Instruction::Label(label) => {
                self.labels_index.insert(label, self.current_index);
                vec![]
            }

            Instruction::Lcall(label) => vec![Ir51::Call(label)],

            Instruction::Mov(dest, src) => self.mov(dest, src),
            Instruction::Clr(dest) => self.mov(dest, Argument::Data8(0)),
            Instruction::Inc(dest) => self.increment(dest),
            Instruction::Dec(dest) => self.decrement(dest),

            Instruction::Add(dest, op) => self.add(dest, op),
            Instruction::Addc(dest, op) => self.addc(dest, op),
            Instruction::Subb(dest, op) => self.subb(dest, op),
            Instruction::Anl(dest, op) => self.anl(dest, op),
            Instruction::Orl(dest, op) => self.orl(dest, op),
            Instruction::Xrl(dest, op) => self.xrl(dest, op),
            Instruction::MulAb => self.mul_ab(),
            Instruction::DivAb => self.div_ab(),
            Instruction::Jnc(label) => self.jnc(label),
            Instruction::Jz(label) => self.jz(label),
            Instruction::Jnz(label) => self.jnz(label),
            Instruction::Ljmp(label) => self.jump(label),
            Instruction::Cjne(lhs, rhs, label) => self.cjne(lhs, rhs, label),
            Instruction::Push(value) => self.push(value),
            Instruction::Pop(value) => self.pop(value),

            Instruction::Ret => vec![Ir51::Return],
            Instruction::Db(_) => vec![],

            _ => todo!("{instr:?}"),
        }
    }

    fn jnc(&mut self, label: usize) -> Vec<Ir51> {
        let true_jmp = label;
        let false_jmp = self.current_index + 2;

        vec![
            Ir51::BinOp {
                op: BinOp::Eq,
                dest: Operand::Temp(0),
                lhs: Operand::Register(Register::C),
                rhs: Operand::Constant(0),
            },
            Ir51::BranchIf {
                cond: Operand::Temp(0),
                true_jmp,
                false_jmp,
            },
        ]
    }

    fn anl(&mut self, dest: Argument, op: Argument) -> Vec<Ir51> {
        let dest_op: Operand = match dest {
            Argument::Reg(reg) => Operand::Register(reg),
            Argument::IramAddress(addr) => Operand::Var(addr.into(), AddrSpace::iData, 1),
            _ => todo!("{dest:?}"),
        };

        let src_op: Operand = match op {
            Argument::Reg(reg) => Operand::Register(reg),
            Argument::IramAddress(addr) => Operand::Var(addr.into(), AddrSpace::iData, 1),
            Argument::Data8(imm) => Operand::Constant(imm.into()),
            _ => todo!("{op:?}"),
        };

        vec![Ir51::BinOp {
            op: BinOp::And,
            dest: dest_op,
            lhs: dest_op,
            rhs: src_op,
        }]
    }

    fn orl(&mut self, dest: Argument, op: Argument) -> Vec<Ir51> {
        let dest_op: Operand = match dest {
            Argument::Reg(reg) => Operand::Register(reg),
            Argument::IramAddress(addr) => Operand::Var(addr.into(), AddrSpace::iData, 1),
            _ => todo!("{dest:?}"),
        };

        let src_op: Operand = match op {
            Argument::Reg(reg) => Operand::Register(reg),
            Argument::IramAddress(addr) => Operand::Var(addr.into(), AddrSpace::iData, 1),
            Argument::Data8(imm) => Operand::Constant(imm.into()),
            _ => todo!("{op:?}"),
        };

        vec![Ir51::BinOp {
            op: BinOp::Or,
            dest: dest_op,
            lhs: dest_op,
            rhs: src_op,
        }]
    }

    fn xrl(&mut self, dest: Argument, op: Argument) -> Vec<Ir51> {
        let dest_op: Operand = match dest {
            Argument::Reg(reg) => Operand::Register(reg),
            Argument::IramAddress(addr) => Operand::Var(addr.into(), AddrSpace::iData, 1),
            _ => todo!("{dest:?}"),
        };

        let src_op: Operand = match op {
            Argument::Reg(reg) => Operand::Register(reg),
            Argument::IramAddress(addr) => Operand::Var(addr.into(), AddrSpace::iData, 1),
            Argument::Data8(imm) => Operand::Constant(imm.into()),
            _ => todo!("{op:?}"),
        };

        vec![Ir51::BinOp {
            op: BinOp::Xor,
            dest: dest_op,
            lhs: dest_op,
            rhs: src_op,
        }]
    }

    fn jz(&mut self, label: usize) -> Vec<Ir51> {
        let true_jmp = label;
        let false_jmp = self.current_index + 2;

        vec![
            Ir51::BinOp {
                op: BinOp::Eq,
                dest: Operand::Temp(0),
                lhs: Operand::Register(Register::A),
                rhs: Operand::Constant(0),
            },
            Ir51::BranchIf {
                cond: Operand::Temp(0),
                true_jmp,
                false_jmp,
            },
        ]
    }

    fn jnz(&mut self, label: usize) -> Vec<Ir51> {
        let true_jmp = label;
        let false_jmp = self.current_index + 2;

        vec![
            Ir51::BinOp {
                op: BinOp::Ne,
                dest: Operand::Temp(0),
                lhs: Operand::Register(Register::A),
                rhs: Operand::Constant(0),
            },
            Ir51::BranchIf {
                cond: Operand::Temp(0),
                true_jmp,
                false_jmp,
            },
        ]
    }

    fn mul_ab(&mut self) -> Vec<Ir51> {
        vec![Ir51::BinOp {
            op: BinOp::Mul,
            dest: Operand::Register(Register::A),
            lhs: Operand::Register(Register::A),
            rhs: Operand::Register(Register::B),
        }]
    }

    fn div_ab(&mut self) -> Vec<Ir51> {
        vec![Ir51::BinOp {
            op: BinOp::Div,
            dest: Operand::Register(Register::A),
            lhs: Operand::Register(Register::A),
            rhs: Operand::Register(Register::B),
        }]
    }

    fn push(&mut self, value: u8) -> Vec<Ir51> {
        Ir51::generate_push(Operand::Var(value.into(), AddrSpace::iData, 1), 1)
    }

    fn pop(&mut self, value: u8) -> Vec<Ir51> {
        Ir51::generate_pop(Operand::Var(value.into(), AddrSpace::iData, 1), 1)
    }

    fn jump(&mut self, label: usize) -> Vec<Ir51> {
        vec![Ir51::Jump(label)]
    }

    fn increment(&mut self, dest: Argument) -> Vec<Ir51> {
        let dest: Operand = match dest {
            Argument::Reg(reg) => Operand::Register(reg),
            Argument::IramAddress(addr) => Operand::Var(addr.into(), AddrSpace::iData, 1),
            _ => todo!("{dest:?}"),
        };

        vec![Ir51::BinOp {
            op: BinOp::Add,
            dest,
            lhs: dest,
            rhs: Operand::Constant(1),
        }]
    }

    fn decrement(&mut self, dest: Argument) -> Vec<Ir51> {
        let dest: Operand = match dest {
            Argument::Reg(reg) => Operand::Register(reg),
            Argument::IramAddress(addr) => Operand::Var(addr.into(), AddrSpace::iData, 1),
            _ => todo!("{dest:?}"),
        };

        vec![Ir51::BinOp {
            op: BinOp::Sub,
            dest,
            lhs: dest,
            rhs: Operand::Constant(1),
        }]
    }

    fn add(&mut self, dest: Argument, src: Argument) -> Vec<Ir51> {
        let dest: Operand = match dest {
            Argument::Reg(Register::A) => Operand::Register(Register::A),
            _ => todo!("{dest:?}"),
        };

        let src: Operand = match src {
            Argument::Reg(reg) => Operand::Register(reg),
            Argument::IramAddress(addr) => Operand::Var(addr.into(), AddrSpace::iData, 1),
            Argument::Data8(imm8) => Operand::Constant(imm8.into()),
            _ => todo!("{src:?}"),
        };

        vec![Ir51::BinOp {
            op: BinOp::Add,
            dest,
            lhs: dest,
            rhs: src,
        }]
    }

    fn addc(&mut self, dest: Argument, src: Argument) -> Vec<Ir51> {
        let dest: Operand = match dest {
            Argument::Reg(Register::A) => Operand::Register(Register::A),
            _ => todo!("{dest:?}"),
        };

        let src: Operand = match src {
            Argument::Reg(reg) => Operand::Register(reg),
            Argument::IramAddress(addr) => Operand::Var(addr.into(), AddrSpace::iData, 1),
            Argument::Data8(imm8) => Operand::Constant(imm8.into()),
            _ => todo!("{src:?}"),
        };

        vec![Ir51::BinOp {
            op: BinOp::Addc,
            dest,
            lhs: dest,
            rhs: src,
        }]
    }

    fn subb(&mut self, dest: Argument, src: Argument) -> Vec<Ir51> {
        let dest: Operand = match dest {
            Argument::Reg(reg) => Operand::Register(reg),
            Argument::IramAddress(addr) => Operand::Var(addr.into(), AddrSpace::iData, 1),
            _ => todo!("{dest:?}"),
        };

        let src: Operand = match src {
            Argument::Reg(reg) => Operand::Register(reg),
            Argument::IramAddress(addr) => Operand::Var(addr.into(), AddrSpace::iData, 1),
            Argument::Data8(imm8) => Operand::Constant(imm8.into()),
            _ => todo!("{src:?}"),
        };

        vec![Ir51::BinOp {
            op: BinOp::Subb,
            dest,
            lhs: dest,
            rhs: src,
        }]
    }

    fn cjne(&mut self, lhs: Argument, rhs: Argument, label: usize) -> Vec<Ir51> {
        let lhs: Operand = match lhs {
            Argument::Reg(reg) => Operand::Register(reg),
            _ => todo!("{lhs:?}"),
        };

        let rhs: Operand = match rhs {
            Argument::Data8(imm8) => Operand::Constant(imm8.into()),
            _ => todo!("{rhs:?}"),
        };

        let true_jmp = label;
        let false_jmp = self.current_index + 3; // cuz cjne is equivalent to 3 ir instuctions

        vec![
            Ir51::BinOp {
                op: BinOp::Ne,
                dest: Operand::Temp(0),
                lhs,
                rhs,
            },
            Ir51::BinOp {
                op: BinOp::Lt,
                dest: Operand::Register(Register::C),
                lhs,
                rhs,
            },
            Ir51::BranchIf {
                cond: Operand::Temp(0),
                true_jmp,
                false_jmp,
            },
        ]
    }

    fn mov(&mut self, dest: Argument, src: Argument) -> Vec<Ir51> {
        let dest: Operand = match dest {
            Argument::IramAddress(addr) => Operand::Var(addr.into(), AddrSpace::iData, 1),
            Argument::Reg(reg) => Operand::Register(reg),
            _ => todo!("{dest:?}"),
        };

        let src: Operand = match src {
            Argument::Data8(imm8) => Operand::Constant(imm8.into()),
            Argument::Data16(imm16) => Operand::Constant(imm16),
            Argument::IramAddress(addr) => Operand::Var(addr.into(), AddrSpace::iData, 1),
            Argument::Reg(reg) => Operand::Register(reg),
            _ => todo!("{src:?}"),
        };

        vec![Ir51::Move { dest, src }]
    }
}
