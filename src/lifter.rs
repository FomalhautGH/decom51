use core::slice;
use std::collections::HashMap;

use crate::common::{AddrSpace, BinOp, Ir51, Operand, Register};
use crate::parser;
use crate::parser::{Instruction, Parser};

pub struct Lifter<'a> {
    parser: &'a mut Parser<'a>,
    labels_index: HashMap<usize, usize>,
    current_index: usize,
}

impl<'a> Lifter<'a> {
    pub fn new(parser: &'a mut Parser<'a>) -> Self {
        Self {
            parser,
            labels_index: HashMap::new(),
            current_index: 0,
        }
    }

    pub fn get_ir_instructions(&mut self) -> Vec<Ir51> {
        let mut ir_code = vec![];

        loop {
            let instr = self.parser.parse_next();
            if instr == Instruction::End {
                break;
            }

            let irs = self.get_ir_from_instruction(instr);
            self.current_index += irs.len();

            ir_code.extend(irs);
        }

        self.fix_jmps(&mut ir_code);
        self.edge_contraction(&mut ir_code);

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

        self.shift_jumps(ir_code);
    }

    fn shift_jumps(&mut self, ir_code: &mut Vec<Ir51>) {
        let mut index_map = HashMap::new();
        let mut real_index = 0;

        // We track the shifted indices before retaining
        for (i, ir) in ir_code.iter().enumerate() {
            if !matches!(ir, Ir51::InvalidIr) {
                index_map.insert(i, real_index);
                real_index += 1;
            }
        }

        // Retain
        ir_code.retain(|ir| !matches!(ir, Ir51::InvalidIr));

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
                _ => {}
            }
        }
    }

    fn is_single_ljmp(jmp_index: usize, ir_code: &mut [Ir51]) -> Option<usize> {
        let ir = ir_code.get_mut(jmp_index)?;

        if let Ir51::Jump(jmp) = *ir {
            *ir = Ir51::InvalidIr;
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
            Instruction::Clr(dest) => self.mov(dest, parser::Operand::Data8(0)),
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
                dest: Operand::Temporary(0),
                lhs: Operand::Register(Register::C),
                rhs: Operand::Costant(0),
            },
            Ir51::BranchIf {
                cond: Operand::Temporary(0),
                true_jmp,
                false_jmp,
            },
        ]
    }

    fn push(&mut self, value: u8) -> Vec<Ir51> {
        vec![
            Ir51::Store {
                space: AddrSpace::iData,
                ptr: Operand::Register(Register::SP),
                value: Operand::Costant(value.into()),
            },
            Ir51::BinOp {
                op: BinOp::Sub,
                dest: Operand::Register(Register::SP),
                lhs: Operand::Register(Register::SP),
                rhs: Operand::Costant(1),
            },
        ]
    }

    fn pop(&mut self, value: u8) -> Vec<Ir51> {
        vec![
            Ir51::Load {
                space: AddrSpace::iData,
                dest_ptr: Operand::Register(Register::SP),
            },
            Ir51::BinOp {
                op: BinOp::Add,
                dest: Operand::Register(Register::SP),
                lhs: Operand::Register(Register::SP),
                rhs: Operand::Costant(1),
            },
        ]
    }

    fn jump(&mut self, label: usize) -> Vec<Ir51> {
        vec![Ir51::Jump(label)]
    }

    fn increment(&mut self, dest: parser::Operand) -> Vec<Ir51> {
        let dest: Operand = match dest {
            parser::Operand::Reg(reg) => Operand::Register(reg),
            _ => todo!("{dest:?}"),
        };

        vec![Ir51::BinOp {
            op: BinOp::Add,
            dest,
            lhs: dest,
            rhs: Operand::Costant(1),
        }]
    }

    fn subb(&mut self, dest: parser::Operand, src: parser::Operand) -> Vec<Ir51> {
        let dest: Operand = match dest {
            parser::Operand::Reg(reg) => Operand::Register(reg),
            _ => todo!("{dest:?}"),
        };

        let op: Operand = match src {
            parser::Operand::Reg(reg) => Operand::Register(reg),
            _ => todo!("{src:?}"),
        };

        vec![Ir51::BinOp {
            op: BinOp::Sub,
            dest,
            lhs: dest,
            rhs: op,
        }]
    }

    fn cjne(&mut self, lhs: parser::Operand, rhs: parser::Operand, label: usize) -> Vec<Ir51> {
        let lhs: Operand = match lhs {
            parser::Operand::Reg(reg) => Operand::Register(reg),
            _ => todo!("{lhs:?}"),
        };

        let rhs: Operand = match rhs {
            parser::Operand::Data8(imm8) => Operand::Costant(imm8.into()),
            _ => todo!("{rhs:?}"),
        };

        let true_jmp = label;
        let false_jmp = self.current_index + 3; // cuz cjne is equivalent to 3 ir instuctions

        vec![
            Ir51::BinOp {
                op: BinOp::Ne,
                dest: Operand::Temporary(0),
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
                cond: Operand::Temporary(0),
                true_jmp,
                false_jmp,
            },
        ]
    }

    fn mov(&mut self, dest: parser::Operand, src: parser::Operand) -> Vec<Ir51> {
        let dest: Operand = match dest {
            parser::Operand::IramAddress(addr) => Operand::Address(addr.into(), AddrSpace::iData),
            parser::Operand::Reg(reg) => Operand::Register(reg),
            _ => todo!("{dest:?}"),
        };

        let src: Operand = match src {
            parser::Operand::Data8(imm8) => Operand::Costant(imm8.into()),
            parser::Operand::Data16(imm16) => Operand::Costant(imm16),
            parser::Operand::IramAddress(addr) => Operand::Address(addr.into(), AddrSpace::iData),
            parser::Operand::Reg(reg) => Operand::Register(reg),
            _ => todo!("{src:?}"),
        };

        vec![Ir51::Move { dest, src }]
    }
}
