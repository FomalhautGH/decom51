use std::io::SeekFrom;

use crate::{
    common::Register,
    lexer::{Lexer, TokenType},
};

#[derive(Debug, PartialEq, Eq)]
pub enum Operand {
    Data8(u8),
    Data16(u16),

    Reg(Register),
    // PtrR(RRegister),
    IramAddress(u8),
    BitAddress(u8),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Instruction {
    Mov(Operand, Operand),
    Clr(Operand),
    Subb(Operand, Operand),
    Cjne(Operand, Operand, usize),
    Inc(Operand),

    Lcall(usize),
    Jnc(usize),
    Ljmp(usize),

    Push(u8),
    Pop(u8),
    Ret,
    Nop,

    Label(usize),
    Db(u16),
    End,
}

pub struct Parser<'a> {
    lexer: &'a mut Lexer,
}

impl<'a> Parser<'a> {
    pub fn new(lexer: &'a mut Lexer) -> Self {
        Self { lexer }
    }

    pub fn parse_next(&mut self) -> Instruction {
        let tok = self.consume();

        match tok {
            TokenType::Mov => self.mov(),
            TokenType::Label(labnum) => self.label(labnum),
            TokenType::Clr => self.clr(),
            TokenType::Subb => self.subb(),
            TokenType::Lcall | TokenType::Ljmp | TokenType::Jnc => self.jump(tok),
            TokenType::Push | TokenType::Pop => self.pushpop(tok),
            TokenType::Inc => self.inc(),
            TokenType::Cjne => self.cjne(),
            TokenType::Db(data) => Instruction::Db(data),
            TokenType::Ret => Instruction::Ret,
            TokenType::End => Instruction::End,
            _ => todo!(),
        }
    }

    fn consume(&mut self) -> TokenType {
        self.lexer.next_token()
    }

    fn inc(&mut self) -> Instruction {
        let reg = self.consume();

        let op = match reg {
            TokenType::Reg(Register::R0) => Operand::Reg(Register::R0),
            TokenType::Reg(Register::R1) => Operand::Reg(Register::R1),
            TokenType::Reg(Register::R2) => Operand::Reg(Register::R2),
            TokenType::Reg(Register::R3) => Operand::Reg(Register::R3),
            TokenType::Reg(Register::R4) => Operand::Reg(Register::R4),
            TokenType::Reg(Register::R5) => Operand::Reg(Register::R5),
            TokenType::Reg(Register::R6) => Operand::Reg(Register::R6),
            TokenType::Reg(Register::R7) => Operand::Reg(Register::R7),
            _ => unreachable!(),
        };

        Instruction::Inc(op)
    }

    fn pushpop(&mut self, current_tok: TokenType) -> Instruction {
        let data = self.consume();

        if let TokenType::Address(addr) = data {
            return match current_tok {
                TokenType::Pop => Instruction::Pop(addr),
                TokenType::Push => Instruction::Push(addr),
                _ => unreachable!(),
            };
        }

        unreachable!()
    }

    fn jump(&mut self, current_tok: TokenType) -> Instruction {
        let label = self.consume();

        if let TokenType::Label(labnum) = label {
            return match current_tok {
                TokenType::Ljmp => Instruction::Ljmp(labnum),
                TokenType::Jnc => Instruction::Jnc(labnum),
                TokenType::Lcall => Instruction::Lcall(labnum),
                _ => unreachable!(),
            };
        }

        unreachable!()
    }

    fn subb(&mut self) -> Instruction {
        let op1 = self.consume();
        assert!(op1 == TokenType::Reg(Register::A));

        let _comma = self.consume();

        let op2 = self.consume();
        let op2 = match op2 {
            TokenType::Reg(Register::R0) => Operand::Reg(Register::R0),
            TokenType::Reg(Register::R1) => Operand::Reg(Register::R1),
            TokenType::Reg(Register::R2) => Operand::Reg(Register::R2),
            TokenType::Reg(Register::R3) => Operand::Reg(Register::R3),
            TokenType::Reg(Register::R4) => Operand::Reg(Register::R4),
            TokenType::Reg(Register::R5) => Operand::Reg(Register::R5),
            TokenType::Reg(Register::R6) => Operand::Reg(Register::R6),
            TokenType::Reg(Register::R7) => Operand::Reg(Register::R7),
            _ => unreachable!(),
        };

        Instruction::Subb(Operand::Reg(Register::A), op2)
    }

    fn clr(&mut self) -> Instruction {
        let op = self.consume();

        let op = match op {
            TokenType::Reg(Register::C) => Operand::Reg(Register::C),
            TokenType::Reg(Register::A) => Operand::Reg(Register::A),
            TokenType::Address(addr) => Operand::BitAddress(addr),
            _ => unreachable!(),
        };

        Instruction::Clr(op)
    }

    fn label(&mut self, labnum: usize) -> Instruction {
        let _semicolon = self.consume();
        Instruction::Label(labnum)
    }

    fn cjne(&mut self) -> Instruction {
        let op1 = self.consume();
        let _comma = self.consume();
        let op2 = self.consume();
        let _comma = self.consume();
        let op3 = self.consume();

        let op1 = match op1 {
            TokenType::Escargot => todo!(),
            TokenType::Reg(Register::A) => Operand::Reg(Register::A),
            TokenType::Reg(Register::R0) => Operand::Reg(Register::R0),
            TokenType::Reg(Register::R1) => Operand::Reg(Register::R1),
            TokenType::Reg(Register::R2) => Operand::Reg(Register::R2),
            TokenType::Reg(Register::R3) => Operand::Reg(Register::R3),
            TokenType::Reg(Register::R4) => Operand::Reg(Register::R4),
            TokenType::Reg(Register::R5) => Operand::Reg(Register::R5),
            TokenType::Reg(Register::R6) => Operand::Reg(Register::R6),
            TokenType::Reg(Register::R7) => Operand::Reg(Register::R7),
            _ => unreachable!(),
        };

        let op2 = match op2 {
            TokenType::Address(addr) => Operand::IramAddress(addr),
            TokenType::Immediate(imm) => Operand::Data8(imm as u8),
            _ => unreachable!(),
        };

        let label = match op3 {
            TokenType::Label(labnum) => labnum,
            _ => unreachable!(),
        };

        Instruction::Cjne(op1, op2, label)
    }

    fn mov(&mut self) -> Instruction {
        let op1 = self.consume();
        let _comma = self.consume();
        let op2 = self.consume();

        let op1 = match op1 {
            TokenType::Escargot => todo!(),
            TokenType::Reg(Register::A) => Operand::Reg(Register::A),
            TokenType::Reg(Register::C) => todo!(),
            TokenType::Reg(Register::DPL) => Operand::Reg(Register::DPL),
            TokenType::Reg(Register::DPH) => Operand::Reg(Register::DPH),
            TokenType::Reg(Register::DPTR) => Operand::Reg(Register::DPTR),
            TokenType::Reg(Register::R0) => Operand::Reg(Register::R0),
            TokenType::Reg(Register::R1) => Operand::Reg(Register::R1),
            TokenType::Reg(Register::R2) => Operand::Reg(Register::R2),
            TokenType::Reg(Register::R3) => Operand::Reg(Register::R3),
            TokenType::Reg(Register::R4) => Operand::Reg(Register::R4),
            TokenType::Reg(Register::R5) => Operand::Reg(Register::R5),
            TokenType::Reg(Register::R6) => Operand::Reg(Register::R6),
            TokenType::Reg(Register::R7) => Operand::Reg(Register::R7),
            TokenType::Address(addr) => Operand::IramAddress(addr),
            _ => unreachable!(),
        };

        let op2 = match op2 {
            TokenType::Escargot => todo!(),
            TokenType::Reg(Register::A) => todo!(),
            TokenType::Reg(Register::C) => todo!(),
            TokenType::Reg(Register::DPL) => todo!(),
            TokenType::Reg(Register::DPH) => todo!(),
            TokenType::Reg(Register::R0) => Operand::Reg(Register::R0),
            TokenType::Reg(Register::R1) => Operand::Reg(Register::R1),
            TokenType::Reg(Register::R2) => Operand::Reg(Register::R2),
            TokenType::Reg(Register::R3) => Operand::Reg(Register::R3),
            TokenType::Reg(Register::R4) => Operand::Reg(Register::R4),
            TokenType::Reg(Register::R5) => Operand::Reg(Register::R5),
            TokenType::Reg(Register::R6) => Operand::Reg(Register::R6),
            TokenType::Reg(Register::R7) => Operand::Reg(Register::R7),
            TokenType::Address(addr) => Operand::IramAddress(addr),
            TokenType::Immediate(imm) => match op1 {
                Operand::Reg(Register::DPTR) => Operand::Data16(imm),
                _ => Operand::Data8(imm as u8),
            },
            _ => unreachable!(),
        };

        Instruction::Mov(op1, op2)
    }
}
