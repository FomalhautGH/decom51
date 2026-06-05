use std::{
    fs::File,
    io::{self, Read},
};

use crate::common::Register;

#[derive(PartialEq, Eq, Debug)]
pub enum TokenType {
    Cseg,
    At,

    // Instructions
    Mov,
    Movx,
    Inc,
    Addc,
    Subb,
    Ljmp,
    Jnc,
    Clr,
    Lcall,
    Xrl,
    Add,
    Ret,
    Push,
    Pop,
    Nop,
    Cjne,

    // Registers
    Reg(Register),

    // Numbers
    Address(u8),
    Immediate(u16),

    // Other
    Comma,
    Escargot,
    SemiColon,
    Label(usize),
    Db(u16),
    End,
}

pub struct Lexer {
    file_content: String,
    index: usize,
    line: usize,
}

impl Lexer {
    pub fn new(file_path: &str) -> io::Result<Self> {
        let mut file = File::open(file_path)?;
        let mut contents = String::new();

        file.read_to_string(&mut contents)?;
        Ok(Self {
            file_content: contents,
            index: 0,
            line: 1,
        })
    }

    // Tecnically this is not the most correct thing for UTF-8 strings but it is
    // fine since the disassembler will never output non ASCII characters
    fn consume(&mut self) -> Option<char> {
        self.index += 1;
        self.file_content.chars().nth(self.index - 1)
    }

    fn peek(&self) -> Option<char> {
        self.file_content.chars().nth(self.index)
    }

    pub fn next_token(&mut self) -> TokenType {
        let mut current_char = self.consume();
        while let Some(cc) = current_char
            && (cc == ' ' || cc == '\t')
        {
            current_char = self.consume();
        }

        if let Some(ch) = current_char
            && ch == ';'
        {
            while let Some(cc) = current_char
                && cc != '\n'
            {
                current_char = self.consume();
            }
        }

        if let Some(c) = current_char {
            match c {
                ',' => TokenType::Comma,
                ':' => TokenType::SemiColon,
                '@' => TokenType::Escargot,
                '#' => self.immediate(),
                '\n' => {
                    self.line += 1;
                    self.next_token()
                }
                // TODO: Addresses are hexadecimals but here we only take decimal
                // cuz some instructions start with a hexadecimal character
                c if c.is_ascii_digit() => self.address(),
                c if c.is_alphabetic() => self.keyword(),
                _ => unreachable!(),
            }
        } else {
            TokenType::End
        }
    }

    fn keyword(&mut self) -> TokenType {
        let kyw_start = self.index - 1;
        while let Some(c) = self.peek()
            && c.is_alphabetic()
        {
            self.consume();
        }
        let kyw_end = self.index;

        let kyw = self
            .file_content
            .get(kyw_start..kyw_end)
            .expect("Malformed instruction");

        match kyw {
            "MOV" => TokenType::Mov,
            "A" => TokenType::Reg(Register::A),
            "NOP" => TokenType::Nop,
            "ADD" => TokenType::Add,
            "PUSH" => TokenType::Push,
            "POP" => TokenType::Pop,
            "CJNE" => TokenType::Cjne,
            "MOVX" => TokenType::Movx,
            "DPTR" => TokenType::Reg(Register::DPTR),
            "INC" => TokenType::Inc,
            "ADDC" => TokenType::Addc,
            "SUBB" => TokenType::Subb,
            "LJMP" => TokenType::Ljmp,
            "JNC" => TokenType::Jnc,
            "CLR" => TokenType::Clr,
            "LCALL" => TokenType::Lcall,
            "XRL" => TokenType::Xrl,
            "DPL" => TokenType::Reg(Register::DPL),
            "DPH" => TokenType::Reg(Register::DPH),
            "RET" => TokenType::Ret,
            "C" => TokenType::Reg(Register::C),
            "DB" => self.data(),
            "L" => self.label(),
            "R" => {
                let index = self
                    .consume()
                    .expect("R registers used without specifying wich one");

                let index: u32 = index.to_digit(10).expect("Malformed use of R registers");

                match index {
                    0 => TokenType::Reg(Register::R0),
                    1 => TokenType::Reg(Register::R1),
                    2 => TokenType::Reg(Register::R2),
                    3 => TokenType::Reg(Register::R3),
                    4 => TokenType::Reg(Register::R4),
                    5 => TokenType::Reg(Register::R5),
                    6 => TokenType::Reg(Register::R6),
                    7 => TokenType::Reg(Register::R7),
                    _ => unreachable!("Unknown R register: R{index}"),
                }
            }
            "END" => TokenType::End,
            _ => unreachable!("Unknown keyword: {kyw}"),
        }
    }

    fn data(&mut self) -> TokenType {
        let _b = self.consume();

        let num_start = self.index;
        while let Some(c) = self.peek()
            && c.is_ascii_hexdigit()
        {
            self.consume();
        }
        let num_end = self.index;

        let num = self
            .file_content
            .get(num_start..num_end)
            .expect("Non ASCII number");

        let num: u16 = u16::from_str_radix(num, 16).expect("Malformed data in Lexer");

        let _h = self.consume();
        TokenType::Db(num)
    }

    fn label(&mut self) -> TokenType {
        let num_start = self.index;
        while let Some(c) = self.peek()
            && c.is_ascii_digit()
        {
            self.consume();
        }
        let num_end = self.index;

        let number = self
            .file_content
            .get(num_start..num_end)
            .expect("Non ASCII number");

        let number: usize = number
            .parse::<usize>()
            .expect("Malformed label number in Lexer");

        TokenType::Label(number)
    }

    fn immediate(&mut self) -> TokenType {
        let imm_start = self.index;
        while let Some(c) = self.peek()
            && c.is_ascii_hexdigit()
        {
            self.consume();
        }
        let imm_end = self.index;

        let _h = self.consume(); // Consume the h at the end

        let imm = self
            .file_content
            .get(imm_start..imm_end)
            .expect("Non ASCII number");

        let imm: u16 = u16::from_str_radix(imm, 16).expect("Malformed address number in Lexer");

        TokenType::Immediate(imm)
    }

    fn address(&mut self) -> TokenType {
        let addr_start = self.index - 1; // -1 because I already consumed the first digit
        while let Some(c) = self.peek()
            && c.is_ascii_hexdigit()
        {
            self.consume();
        }
        let addr_end = self.index;

        let _h = self.consume(); // Consume the h at the end

        let addr = self
            .file_content
            .get(addr_start..addr_end)
            .expect("Non ASCII number");

        let addr: u8 = u8::from_str_radix(addr, 16).expect("Malformed address number in Lexer");

        TokenType::Address(addr)
    }
}
