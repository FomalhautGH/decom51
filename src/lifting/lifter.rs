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

            Instruction::Subb(dest, op) => self.subb(dest, op),
            Instruction::Jnc(label) => self.jnc(label),
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

    fn push(&mut self, value: u8) -> Vec<Ir51> {
        Ir51::generate_push(Operand::Var(value.into(), AddrSpace::iData), 1)
    }

    fn pop(&mut self, value: u8) -> Vec<Ir51> {
        Ir51::generate_pop(Operand::Var(value.into(), AddrSpace::iData), 1)
    }

    fn jump(&mut self, label: usize) -> Vec<Ir51> {
        vec![Ir51::Jump(label)]
    }

    fn increment(&mut self, dest: Argument) -> Vec<Ir51> {
        let dest: Operand = match dest {
            Argument::Reg(reg) => Operand::Register(reg),
            _ => todo!("{dest:?}"),
        };

        vec![Ir51::BinOp {
            op: BinOp::Add,
            dest,
            lhs: dest,
            rhs: Operand::Constant(1),
        }]
    }

    fn subb(&mut self, dest: Argument, src: Argument) -> Vec<Ir51> {
        let dest: Operand = match dest {
            Argument::Reg(reg) => Operand::Register(reg),
            _ => todo!("{dest:?}"),
        };

        let src: Operand = match src {
            Argument::Reg(reg) => Operand::Register(reg),
            _ => todo!("{src:?}"),
        };

        vec![
            Ir51::BinOp {
                op: BinOp::Lt,
                dest: Operand::Temp(1),
                lhs: dest,
                rhs: src,
            },
            Ir51::BinOp {
                op: BinOp::Sub,
                dest: Operand::Temp(0),
                lhs: dest,
                rhs: src,
            },
            Ir51::BinOp {
                op: BinOp::Lt,
                dest: Operand::Temp(2),
                lhs: Operand::Temp(0),
                rhs: Operand::Register(Register::C),
            },
            Ir51::BinOp {
                op: BinOp::Sub,
                dest,
                lhs: Operand::Temp(0),
                rhs: Operand::Register(Register::C),
            },
            Ir51::BinOp {
                op: BinOp::Or,
                dest: Operand::Register(Register::C),
                lhs: Operand::Temp(1),
                rhs: Operand::Temp(2),
            },
        ]
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
            Argument::IramAddress(addr) => Operand::Var(addr.into(), AddrSpace::iData),
            Argument::Reg(reg) => Operand::Register(reg),
            _ => todo!("{dest:?}"),
        };

        let src: Operand = match src {
            Argument::Data8(imm8) => Operand::Constant(imm8.into()),
            Argument::Data16(imm16) => Operand::Constant(imm16),
            Argument::IramAddress(addr) => Operand::Var(addr.into(), AddrSpace::iData),
            Argument::Reg(reg) => Operand::Register(reg),
            _ => todo!("{src:?}"),
        };

        vec![Ir51::Move { dest, src }]
    }
}
