use core::slice;
use std::collections::HashMap;

use crate::common::Register;
use crate::parser::{Instruction, Parser};
use crate::parser;

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
    Lt,
    Gt,
    Le,
    Ge,
    Ne
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Ir51 {
    Move { dest: Operand, src: Operand },
    BinOp { op: BinOp, dest: Operand, lhs: Operand, rhs: Operand },

    Store { space: AddrSpace, ptr: Operand, value: Operand },
    Load { space: AddrSpace, dest_ptr: Operand },

    Jump(usize),
    BranchIf { cond: Operand, true_jmp: Result<u16, usize>, false_jmp: u16 },
    Call(usize),
    Return,

    End,
}

pub struct Lifter<'a> {
    parser: &'a mut Parser<'a>,
    labels_index: HashMap<usize, u16>,
    index: u16,
}

impl<'a> Lifter<'a> {
    pub fn new(parser: &'a mut Parser<'a>) -> Self {
        Self { parser, labels_index: HashMap::new(), index: 1 }
    }

    pub fn get_ir_instructions(&mut self) -> Vec<Ir51> {
        let mut ir_code = vec![];

        loop {
            let instr = self.parser.parse_next();
            if instr == Instruction::End { break; }

            println!("{}: ", self.index);

            let irs = match instr {
                Instruction::Label(label) => { 
                    self.labels_index.insert(label, self.index);
                    self.index -= 1;
                    vec![]
                }

                Instruction::Lcall(label) => vec![Ir51::Call(label)],

                Instruction::Mov(dest, src) => self.mov(dest, src),
                Instruction::Clr(dest) => self.mov(dest, parser::Operand::Data8(0)),
                Instruction::Inc(dest) => self.increment(dest),

                Instruction::Subb(dest, op) => self.subb(dest, op),
                Instruction::Jnc(label) | Instruction::Ljmp(label) => self.jump(label),
                Instruction::Cjne(lhs, rhs, label) => self.cjne(lhs, rhs, label),
                Instruction::Push(value) => self.push(value),
                Instruction::Pop(value) => self.pop(value),

                Instruction::Ret => vec![Ir51::Return],
                Instruction::Db(_) => vec![],

                _ => todo!("{instr:?}")
            };

            println!("{irs:?}");

            self.index += 1;
            ir_code.extend(irs);
        }

        self.fix_jmps(&mut ir_code);
        ir_code
    }

    fn fix_jmps<'b>(&self, ir_code: &'b mut Vec<Ir51>) -> &'b mut Vec<Ir51> {
        for ir in ir_code.iter_mut() {
            if let Ir51::BranchIf { cond, true_jmp, false_jmp } = ir 
                && let Err(label) = true_jmp {
                
                let true_jmp = Ok(self.labels_index.get(label).expect("No label found")).copied();
                *ir = Ir51::BranchIf { cond: *cond, true_jmp, false_jmp: *false_jmp }
            }
        }

        ir_code
    }

    fn push(&mut self, value: u8) -> Vec<Ir51> {
        vec![
            Ir51::Store { space: AddrSpace::iData, ptr: Operand::Register(Register::SP), value: Operand::Costant(value.into()) },
            Ir51::BinOp { op: BinOp::Sub, dest: Operand::Register(Register::SP), lhs: Operand::Register(Register::SP), rhs: Operand::Costant(1) }
        ]
    }

    fn pop(&mut self, value: u8) -> Vec<Ir51> {
        vec![
            Ir51::Load { space: AddrSpace::iData, dest_ptr: Operand::Register(Register::SP) },
            Ir51::BinOp { op: BinOp::Add, dest: Operand::Register(Register::SP), lhs: Operand::Register(Register::SP), rhs: Operand::Costant(1) }
        ]
    }

    fn jump(&mut self, label: usize) -> Vec<Ir51> {
        vec![Ir51::Jump(label)]
    }

    fn increment(&mut self, dest: parser::Operand) -> Vec<Ir51> {
        let dest: Operand = match dest {
            parser::Operand::Reg(reg) => Operand::Register(reg),
            _ => todo!("{dest:?}")
        };

        vec![Ir51::BinOp { op: BinOp::Add, dest: dest.clone(), lhs: dest, rhs: Operand::Costant(1) }]
    }

    fn subb(&mut self, dest: parser::Operand, src: parser::Operand) -> Vec<Ir51> {
        let dest: Operand = match dest {
            parser::Operand::Reg(reg) => Operand::Register(reg),
            _ => todo!("{dest:?}")
        };

        let op: Operand = match src {
            parser::Operand::Reg(reg) => Operand::Register(reg),
            _ => todo!("{src:?}")
        };

        vec![Ir51::BinOp { op: BinOp::Sub, dest: dest.clone(), lhs: dest, rhs: op }]
    }

    fn cjne(&mut self, lhs: parser::Operand, rhs: parser::Operand, label: usize) -> Vec<Ir51> {
        let lhs: Operand = match lhs {
            parser::Operand::Reg(reg) => Operand::Register(reg),
            _ => todo!("{lhs:?}")
        };

        let rhs: Operand = match rhs {
            parser::Operand::Data8(imm8) => Operand::Costant(imm8.into()),
            _ => todo!("{rhs:?}")
        };

        let true_jmp = Err(label);
        let false_jmp = self.index + 1;

        vec![
            Ir51::BinOp { op: BinOp::Ne, dest: Operand::Temporary(0), lhs, rhs },
            Ir51::BinOp { op: BinOp::Lt, dest: Operand::Register(Register::C), lhs, rhs },
            Ir51::BranchIf { cond: Operand::Temporary(0), true_jmp, false_jmp }
        ]
    }

    fn mov(&mut self, dest: parser::Operand, src: parser::Operand) -> Vec<Ir51> {
        let dest: Operand = match dest {
            parser::Operand::IramAddress(addr) => Operand::Address(addr.into(), AddrSpace::iData),
            parser::Operand::Reg(reg) => Operand::Register(reg),
            _ => todo!("{dest:?}")
        };

        let src: Operand = match src {
            parser::Operand::Data8(imm8) => Operand::Costant(imm8.into()),
            parser::Operand::Data16(imm16) => Operand::Costant(imm16),
            parser::Operand::IramAddress(addr) => Operand::Address(addr.into(), AddrSpace::iData),
            parser::Operand::Reg(reg) => Operand::Register(reg),
            _ => todo!("{src:?}")
        };

        vec![Ir51::Move { dest, src }]
    }
}
