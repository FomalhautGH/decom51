use core::slice;
use std::collections::HashMap;

use crate::common::Register;
use crate::parser;
use crate::parser::{Instruction, Parser};

#[allow(non_camel_case_types)]
#[derive(Debug, PartialEq, Clone, Copy)]
// TODO: controllare la prensenza di altri spazi di memoria
pub enum AddrSpace {
    iData,
    xData,
    Code,
}

// TODO: fare il mapping indirizzo registro perche' ci potrebbe essere nell'assembly cosi'
// che abbiamo il registro corretto per quell'indirizzo
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Operand {
    Register(Register),
    Address(u16, AddrSpace),
    Temporary(usize),
    Costant(u16),
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum BinOp {
    Add,
    Sub,
    Eq,
    Lt,
    Gt,
    Le,
    Ge,
    Ne,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Ir51Type {
    Move {
        dest: Operand,
        src: Operand,
    },
    BinOp {
        op: BinOp,
        dest: Operand,
        lhs: Operand,
        rhs: Operand,
    },

    Store {
        space: AddrSpace,
        ptr: Operand,
        value: Operand,
    },
    Load {
        space: AddrSpace,
        dest_ptr: Operand,
    },

    Jump(usize),
    BranchIf {
        cond: Operand,
        true_jmp: usize,
        false_jmp: usize,
    },
    Call(usize),
    Return,

    End,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Ir51 {
    pub index: usize,
    pub instr: Ir51Type,
}

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
            current_index: 1,
        }
    }

    pub fn get_ir_instructions(&mut self) -> Vec<Ir51> {
        let mut ir_code = vec![];

        loop {
            let instr = self.parser.parse_next();
            if instr == Instruction::End {
                break;
            }

            let irs = match instr {
                Instruction::Label(label) => {
                    self.labels_index.insert(label, self.current_index);
                    vec![]
                }

                Instruction::Lcall(label) => vec![Ir51Type::Call(label)],

                Instruction::Mov(dest, src) => self.mov(dest, src),
                Instruction::Clr(dest) => self.mov(dest, parser::Operand::Data8(0)),
                Instruction::Inc(dest) => self.increment(dest),

                Instruction::Subb(dest, op) => self.subb(dest, op),
                Instruction::Jnc(label) => self.jnc(label),
                Instruction::Ljmp(label) => self.jump(label),
                Instruction::Cjne(lhs, rhs, label) => self.cjne(lhs, rhs, label),
                Instruction::Push(value) => self.push(value),
                Instruction::Pop(value) => self.pop(value),

                Instruction::Ret => vec![Ir51Type::Return],
                Instruction::Db(_) => vec![],

                _ => todo!("{instr:?}"),
            };

            let ir = irs.iter().map(|t| Ir51 {
                index: self.current_index,
                instr: *t,
            });
            ir_code.extend(ir);

            self.current_index += irs.len();
        }

        self.fix_jmps(&mut ir_code);
        ir_code
    }

    fn fix_jmps(&self, ir_code: &mut [Ir51]) {
        for ir in ir_code.iter_mut() {
            match ir.instr {
                Ir51Type::BranchIf {
                    cond,
                    true_jmp,
                    false_jmp,
                } => {
                    let true_jmp = *self.labels_index.get(&true_jmp).expect("Label not found");
                    ir.instr = Ir51Type::BranchIf {
                        cond,
                        true_jmp,
                        false_jmp,
                    }
                }
                Ir51Type::Jump(label) => {
                    let jmp = *self.labels_index.get(&label).expect("Label not found");
                    ir.instr = Ir51Type::Jump(jmp)
                }
                _ => {}
            }
            if let Ir51Type::BranchIf {
                cond,
                true_jmp,
                false_jmp,
            } = ir.instr
            {}
        }
    }

    fn jnc(&mut self, label: usize) -> Vec<Ir51Type> {
        let true_jmp = label;
        let false_jmp = self.current_index + 2;

        vec![
            Ir51Type::BinOp {
                op: BinOp::Eq,
                dest: Operand::Temporary(0),
                lhs: Operand::Register(Register::C),
                rhs: Operand::Costant(0),
            },
            Ir51Type::BranchIf {
                cond: Operand::Temporary(0),
                true_jmp,
                false_jmp,
            },
        ]
    }

    fn push(&mut self, value: u8) -> Vec<Ir51Type> {
        vec![
            Ir51Type::Store {
                space: AddrSpace::iData,
                ptr: Operand::Register(Register::SP),
                value: Operand::Costant(value.into()),
            },
            Ir51Type::BinOp {
                op: BinOp::Sub,
                dest: Operand::Register(Register::SP),
                lhs: Operand::Register(Register::SP),
                rhs: Operand::Costant(1),
            },
        ]
    }

    fn pop(&mut self, value: u8) -> Vec<Ir51Type> {
        vec![
            Ir51Type::Load {
                space: AddrSpace::iData,
                dest_ptr: Operand::Register(Register::SP),
            },
            Ir51Type::BinOp {
                op: BinOp::Add,
                dest: Operand::Register(Register::SP),
                lhs: Operand::Register(Register::SP),
                rhs: Operand::Costant(1),
            },
        ]
    }

    fn jump(&mut self, label: usize) -> Vec<Ir51Type> {
        vec![Ir51Type::Jump(label)]
    }

    fn increment(&mut self, dest: parser::Operand) -> Vec<Ir51Type> {
        let dest: Operand = match dest {
            parser::Operand::Reg(reg) => Operand::Register(reg),
            _ => todo!("{dest:?}"),
        };

        vec![Ir51Type::BinOp {
            op: BinOp::Add,
            dest,
            lhs: dest,
            rhs: Operand::Costant(1),
        }]
    }

    fn subb(&mut self, dest: parser::Operand, src: parser::Operand) -> Vec<Ir51Type> {
        let dest: Operand = match dest {
            parser::Operand::Reg(reg) => Operand::Register(reg),
            _ => todo!("{dest:?}"),
        };

        let op: Operand = match src {
            parser::Operand::Reg(reg) => Operand::Register(reg),
            _ => todo!("{src:?}"),
        };

        vec![Ir51Type::BinOp {
            op: BinOp::Sub,
            dest,
            lhs: dest,
            rhs: op,
        }]
    }

    fn cjne(&mut self, lhs: parser::Operand, rhs: parser::Operand, label: usize) -> Vec<Ir51Type> {
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
            Ir51Type::BinOp {
                op: BinOp::Ne,
                dest: Operand::Temporary(0),
                lhs,
                rhs,
            },
            Ir51Type::BinOp {
                op: BinOp::Lt,
                dest: Operand::Register(Register::C),
                lhs,
                rhs,
            },
            Ir51Type::BranchIf {
                cond: Operand::Temporary(0),
                true_jmp,
                false_jmp,
            },
        ]
    }

    fn mov(&mut self, dest: parser::Operand, src: parser::Operand) -> Vec<Ir51Type> {
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

        vec![Ir51Type::Move { dest, src }]
    }
}
