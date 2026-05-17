use std::{
    fs::File,
    io::{self, Read},
};

#[derive(Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RRegister {
    R0 = 0,
    R1 = 1,
    R2 = 2,
    R3 = 3,
    R4 = 4,
    R5 = 5,
    R6 = 6,
    R7 = 7,
}

impl From<u32> for RRegister {
    fn from(value: u32) -> Self {
        match value {
            0 => Self::R0,
            1 => Self::R1,
            2 => Self::R2,
            3 => Self::R3,
            4 => Self::R4,
            5 => Self::R5,
            6 => Self::R6,
            7 => Self::R7,
            _ => unreachable!("Invalid R register index"),
        }
    }
}

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
    Dptr,
    Carry,
    Acc,
    Dpl,
    Dph,
    R(RRegister),

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
        while let Some(cc) = current_char && (cc == ' ' || cc == '\t') {
            current_char = self.consume();
        }

        if let Some(ch) = current_char && ch == ';' {
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
        while let Some(c) = self.peek() && c.is_alphabetic() {
            self.consume();
        }
        let kyw_end = self.index;

        let kyw = self
            .file_content
            .get(kyw_start..kyw_end)
            .expect("Malformed instruction");

        match kyw {
            "MOV" => TokenType::Mov,
            "A" => TokenType::Acc,
            "NOP" => TokenType::Nop,
            "ADD" => TokenType::Add,
            "PUSH" => TokenType::Push,
            "POP" => TokenType::Pop,
            "CJNE" => TokenType::Cjne,
            "MOVX" => TokenType::Movx,
            "DPTR" => TokenType::Dptr,
            "ACC" => TokenType::Acc,
            "INC" => TokenType::Inc,
            "ADDC" => TokenType::Addc,
            "SUBB" => TokenType::Subb,
            "LJMP" => TokenType::Ljmp,
            "JNC" => TokenType::Jnc,
            "CLR" => TokenType::Clr,
            "LCALL" => TokenType::Lcall,
            "XRL" => TokenType::Xrl,
            "DPL" => TokenType::Dpl,
            "DPH" => TokenType::Dph,
            "RET" => TokenType::Ret,
            "C" => TokenType::Carry,
            "DB" => self.data(),
            "L" => self.label(),
            "R" => {
                let index = self
                    .consume()
                    .expect("R registers used without specifying wich one");

                let index = index
                    .to_digit(10)
                    .expect("Malformed use of R registers")
                    .into();

                TokenType::R(index)
            }
            "END" => TokenType::End,
            _ => unreachable!("Unknown keyword: {kyw}"),
        }
    }

    fn data(&mut self) -> TokenType {
        let _b = self.consume();

        let num_start = self.index;
        while let Some(c) = self.peek() && c.is_ascii_hexdigit() {
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
        while let Some(c) = self.peek() && c.is_ascii_digit() {
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
        while let Some(c) = self.peek() && c.is_ascii_hexdigit() {
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
        while let Some(c) = self.peek() && c.is_ascii_hexdigit() {
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
