use crate::{
    common::Register,
    syntax::lexer::{Lexer, TokenType},
};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Argument {
    Data8(u8),
    Data16(u16),

    Reg(Register),
    // PtrR(RRegister),
    IramAddress(u8),
    BitAddress(u8),
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Instruction {
    Mov(Argument, Argument),
    Clr(Argument),
    Add(Argument, Argument),
    Addc(Argument, Argument),
    Subb(Argument, Argument),
    Anl(Argument, Argument),
    Orl(Argument, Argument),
    Xrl(Argument, Argument),
    MulAb,
    DivAb,
    Cjne(Argument, Argument, usize),
    Inc(Argument),
    Dec(Argument),

    Lcall(usize),
    Jnc(usize),
    Jz(usize),
    Jnz(usize),
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

    pub fn parse(&mut self) -> Vec<Instruction> {
        let mut instructions = Vec::new();

        loop {
            let tok = self.consume();

            let instr = match tok {
                TokenType::Mov => self.mov(),
                TokenType::Label(labnum) => self.label(labnum),
                TokenType::Clr => self.clr(),
                TokenType::Add => self.add(),
                TokenType::Addc => self.addc(),
                TokenType::Subb => self.subb(),
                TokenType::Anl => self.anl(),
                TokenType::Orl => self.orl(),
                TokenType::Xrl => self.xrl(),
                TokenType::Mul => self.mul_ab(),
                TokenType::Div => self.div_ab(),
                TokenType::Lcall
                | TokenType::Ljmp
                | TokenType::Jnc
                | TokenType::Jz
                | TokenType::Jnz => self.jump(tok),
                TokenType::Push | TokenType::Pop => self.pushpop(tok),
                TokenType::Inc => self.inc(),
                TokenType::Dec => self.dec(),
                TokenType::Cjne => self.cjne(),
                TokenType::Db(data) => Instruction::Db(data),
                TokenType::Ret => Instruction::Ret,
                TokenType::End => Instruction::End,
                _ => todo!("{tok:?}"),
            };

            if instr == Instruction::End {
                break;
            }

            instructions.push(instr)
        }

        instructions
    }

    fn consume(&mut self) -> TokenType {
        self.lexer.next_token()
    }

    fn inc(&mut self) -> Instruction {
        let reg = self.consume();

        let op = match reg {
            TokenType::Reg(r) => Argument::Reg(r),
            TokenType::Address(addr) => Argument::IramAddress(addr),
            _ => unreachable!(),
        };

        Instruction::Inc(op)
    }

    fn dec(&mut self) -> Instruction {
        let reg = self.consume();

        let op = match reg {
            TokenType::Reg(r) => Argument::Reg(r),
            TokenType::Address(addr) => Argument::IramAddress(addr),
            _ => unreachable!(),
        };

        Instruction::Dec(op)
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
                TokenType::Jz => Instruction::Jz(labnum),
                TokenType::Jnz => Instruction::Jnz(labnum),
                TokenType::Lcall => Instruction::Lcall(labnum),
                _ => unreachable!(),
            };
        }

        unreachable!()
    }

    fn add(&mut self) -> Instruction {
        let op1 = self.consume();
        assert!(op1 == TokenType::Reg(Register::A));

        let _comma = self.consume();

        let op2 = self.consume();
        let op2 = match op2 {
            TokenType::Reg(reg) => Argument::Reg(reg),
            TokenType::Address(addr) => Argument::IramAddress(addr),
            TokenType::Immediate(imm) => Argument::Data8(imm as u8),
            _ => unreachable!(),
        };

        Instruction::Add(Argument::Reg(Register::A), op2)
    }

    fn addc(&mut self) -> Instruction {
        let op1 = self.consume();
        assert!(op1 == TokenType::Reg(Register::A));

        let _comma = self.consume();

        let op2 = self.consume();
        let op2 = match op2 {
            TokenType::Reg(reg) => Argument::Reg(reg),
            TokenType::Address(addr) => Argument::IramAddress(addr),
            TokenType::Immediate(imm) => Argument::Data8(imm as u8),
            _ => unreachable!(),
        };

        Instruction::Addc(Argument::Reg(Register::A), op2)
    }

    fn subb(&mut self) -> Instruction {
        let op1 = self.consume();
        assert!(op1 == TokenType::Reg(Register::A));

        let _comma = self.consume();

        let op2 = self.consume();
        let op2 = match op2 {
            TokenType::Reg(Register::R0) => Argument::Reg(Register::R0),
            TokenType::Reg(Register::R1) => Argument::Reg(Register::R1),
            TokenType::Reg(Register::R2) => Argument::Reg(Register::R2),
            TokenType::Reg(Register::R3) => Argument::Reg(Register::R3),
            TokenType::Reg(Register::R4) => Argument::Reg(Register::R4),
            TokenType::Reg(Register::R5) => Argument::Reg(Register::R5),
            TokenType::Reg(Register::R6) => Argument::Reg(Register::R6),
            TokenType::Reg(Register::R7) => Argument::Reg(Register::R7),
            TokenType::Address(addr) => Argument::IramAddress(addr),
            TokenType::Immediate(imm) => Argument::Data8(imm as u8),
            _ => unreachable!(),
        };

        Instruction::Subb(Argument::Reg(Register::A), op2)
    }

    fn anl(&mut self) -> Instruction {
        let op1 = self.consume();
        let _comma = self.consume();
        let op2 = self.consume();

        let arg1 = match op1 {
            TokenType::Reg(r) => Argument::Reg(r),
            TokenType::Address(addr) => Argument::IramAddress(addr),
            _ => unreachable!(),
        };

        let arg2 = match op2 {
            TokenType::Reg(r) => Argument::Reg(r),
            TokenType::Address(addr) => Argument::IramAddress(addr),
            TokenType::Immediate(imm) => Argument::Data8(imm as u8),
            _ => unreachable!(),
        };

        Instruction::Anl(arg1, arg2)
    }

    fn orl(&mut self) -> Instruction {
        let op1 = self.consume();
        let _comma = self.consume();
        let op2 = self.consume();

        let arg1 = match op1 {
            TokenType::Reg(r) => Argument::Reg(r),
            TokenType::Address(addr) => Argument::IramAddress(addr),
            _ => unreachable!(),
        };

        let arg2 = match op2 {
            TokenType::Reg(r) => Argument::Reg(r),
            TokenType::Address(addr) => Argument::IramAddress(addr),
            TokenType::Immediate(imm) => Argument::Data8(imm as u8),
            _ => unreachable!(),
        };

        Instruction::Orl(arg1, arg2)
    }

    fn xrl(&mut self) -> Instruction {
        let op1 = self.consume();
        let _comma = self.consume();
        let op2 = self.consume();

        let arg1 = match op1 {
            TokenType::Reg(r) => Argument::Reg(r),
            TokenType::Address(addr) => Argument::IramAddress(addr),
            _ => unreachable!(),
        };

        let arg2 = match op2 {
            TokenType::Reg(r) => Argument::Reg(r),
            TokenType::Address(addr) => Argument::IramAddress(addr),
            TokenType::Immediate(imm) => Argument::Data8(imm as u8),
            _ => unreachable!(),
        };

        Instruction::Xrl(arg1, arg2)
    }

    fn mul_ab(&mut self) -> Instruction {
        let _op = self.consume();
        Instruction::MulAb
    }

    fn div_ab(&mut self) -> Instruction {
        let _op = self.consume();
        Instruction::DivAb
    }

    fn clr(&mut self) -> Instruction {
        let op = self.consume();

        let op = match op {
            TokenType::Reg(Register::C) => Argument::Reg(Register::C),
            TokenType::Reg(Register::A) => Argument::Reg(Register::A),
            TokenType::Address(addr) => Argument::BitAddress(addr),
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
            TokenType::Reg(Register::A) => Argument::Reg(Register::A),
            TokenType::Reg(Register::R0) => Argument::Reg(Register::R0),
            TokenType::Reg(Register::R1) => Argument::Reg(Register::R1),
            TokenType::Reg(Register::R2) => Argument::Reg(Register::R2),
            TokenType::Reg(Register::R3) => Argument::Reg(Register::R3),
            TokenType::Reg(Register::R4) => Argument::Reg(Register::R4),
            TokenType::Reg(Register::R5) => Argument::Reg(Register::R5),
            TokenType::Reg(Register::R6) => Argument::Reg(Register::R6),
            TokenType::Reg(Register::R7) => Argument::Reg(Register::R7),
            _ => unreachable!(),
        };

        let op2 = match op2 {
            TokenType::Address(addr) => Argument::IramAddress(addr),
            TokenType::Immediate(imm) => Argument::Data8(imm as u8),
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
            TokenType::Reg(Register::A) => Argument::Reg(Register::A),
            TokenType::Reg(Register::B) => Argument::Reg(Register::B),
            TokenType::Reg(Register::C) => todo!(),
            TokenType::Reg(Register::DPL) => Argument::Reg(Register::DPL),
            TokenType::Reg(Register::DPH) => Argument::Reg(Register::DPH),
            TokenType::Reg(Register::DPTR) => Argument::Reg(Register::DPTR),
            TokenType::Reg(Register::R0) => Argument::Reg(Register::R0),
            TokenType::Reg(Register::R1) => Argument::Reg(Register::R1),
            TokenType::Reg(Register::R2) => Argument::Reg(Register::R2),
            TokenType::Reg(Register::R3) => Argument::Reg(Register::R3),
            TokenType::Reg(Register::R4) => Argument::Reg(Register::R4),
            TokenType::Reg(Register::R5) => Argument::Reg(Register::R5),
            TokenType::Reg(Register::R6) => Argument::Reg(Register::R6),
            TokenType::Reg(Register::R7) => Argument::Reg(Register::R7),
            TokenType::Address(addr) => Argument::IramAddress(addr),
            _ => unreachable!(),
        };

        let op2 = match op2 {
            TokenType::Escargot => todo!(),
            TokenType::Reg(Register::C) => todo!(),
            TokenType::Reg(Register::DPL) => todo!(),
            TokenType::Reg(Register::DPH) => todo!(),
            TokenType::Reg(Register::A) => Argument::Reg(Register::A),
            TokenType::Reg(Register::B) => Argument::Reg(Register::B),
            TokenType::Reg(Register::R0) => Argument::Reg(Register::R0),
            TokenType::Reg(Register::R1) => Argument::Reg(Register::R1),
            TokenType::Reg(Register::R2) => Argument::Reg(Register::R2),
            TokenType::Reg(Register::R3) => Argument::Reg(Register::R3),
            TokenType::Reg(Register::R4) => Argument::Reg(Register::R4),
            TokenType::Reg(Register::R5) => Argument::Reg(Register::R5),
            TokenType::Reg(Register::R6) => Argument::Reg(Register::R6),
            TokenType::Reg(Register::R7) => Argument::Reg(Register::R7),
            TokenType::Address(addr) => Argument::IramAddress(addr),
            TokenType::Immediate(imm) => match op1 {
                Argument::Reg(Register::DPTR) => Argument::Data16(imm),
                _ => Argument::Data8(imm as u8),
            },
            _ => unreachable!(),
        };

        Instruction::Mov(op1, op2)
    }
}
