use std::{fs::File, io::{self, Read}};

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

    // Registers
    Dptr,
    Carry,
    Acc,
    Dpl,
    Dph,
    R(u8),

    // Numbers
    Address(u16),
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
    line: usize
}

impl Lexer {
    pub fn new(file_path: &str) -> io::Result<Self> {
        let mut file = File::open(file_path)?;
        let mut contents = String::new();

        file.read_to_string(&mut contents)?;
        Ok(Self { file_content: contents, index: 0, line: 1 })
    }

    // Tecnically this is not the most correct thing for UTF-8 strings but it is
    // fine since the disassembler is part of the program and i control it
    fn consume(&mut self) -> Option<char> {
        self.index += 1;
        self.file_content.chars().nth(self.index - 1)
    }

    fn peek(&self) -> Option<char> {
        self.file_content.chars().nth(self.index)
    }

    pub fn parse(&mut self) -> TokenType {
        let mut c = self.consume();
        while let Some(ch) = c && (ch == ' ' || ch == '\t') {
            c = self.consume();
        }

        if let Some(c) = c {
            match c {
                ',' => TokenType::Comma,
                ':' => TokenType::SemiColon,
                '@' => TokenType::Escargot,
                '#' => self.immediate(),
                '\n' => {
                    self.line += 1;
                    self.parse()
                }
                c if c.is_digit(10) => self.address(), 
                c if c.is_alphabetic() => self.keyword(),
                _ => unreachable!()
            }
        } else {
            TokenType::End
        }
    }

    fn keyword(&mut self) -> TokenType {
        let kyw_start = self.index - 1;
        while let Some(c) = self.peek() && c.is_alphabetic() { self.consume(); }
        let kyw_end = self.index;

        let kyw = self.file_content.get(kyw_start..kyw_end)
            .expect("Malformed instruction");

        match kyw {
            "MOV" => TokenType::Mov,
            "A" => TokenType::Acc,
            "ADD" => TokenType::Add,
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
                const MAX_R_REGISTER: u8 = 7;
                let index = self.consume()
                    .expect("R registers used without specifying wich one");

                let index = index.to_digit(10)
                    .expect("Malformed use of R registers")
                    .try_into();

                match index {
                    Ok(value) => {
                        assert!(value <= MAX_R_REGISTER);
                        TokenType::R(value)
                    } 
                    Err(e) => unreachable!("{e}")
                }
            }
            "END" => TokenType::End,
            _ => unreachable!("Unknown keyword: {kyw}"),
        }
    }

    fn data(&mut self) -> TokenType {
        let _b = self.consume();

        let num_start = self.index; 
        while let Some(c) = self.peek() && c.is_digit(16) { self.consume(); }
        let num_end = self.index;

        let num = self.file_content.get(num_start..num_end)
            .expect("Non ASCII number");

        let num: u16 = u16::from_str_radix(num, 16)
            .expect("Malformed data in Lexer");

        let _h = self.consume();
        TokenType::Db(num)
    }

    fn label(&mut self) -> TokenType {
        let num_start = self.index;
        while let Some(c) = self.peek() && c.is_digit(10) { self.consume(); }
        let num_end = self.index;

        let number = self.file_content.get(num_start..num_end)
            .expect("Non ASCII number");

        let number: usize = usize::from_str_radix(number, 10)
            .expect("Malformed label number in Lexer");

        TokenType::Label(number)
    }

    fn immediate(&mut self) -> TokenType {
        let imm_start = self.index; 
        while let Some(c) = self.peek() && c.is_digit(16) { self.consume(); }
        let imm_end = self.index;

        let _h = self.consume(); // Consume the h at the end

        let imm = self.file_content.get(imm_start..imm_end)
            .expect("Non ASCII number");

        let imm: u16 = u16::from_str_radix(imm, 16)
            .expect("Malformed address number in Lexer");

        TokenType::Immediate(imm)
    }

    fn address(&mut self) -> TokenType {
        let addr_start = self.index - 1; // -1 because I already consumed the first digit
        while let Some(c) = self.peek() && c.is_digit(16) { self.consume(); }
        let addr_end = self.index;

        let _h = self.consume(); // Consume the h at the end

        let addr = self.file_content.get(addr_start..addr_end)
            .expect("Non ASCII number");

        let addr: u16 = u16::from_str_radix(addr, 16)
            .expect("Malformed address number in Lexer");

        TokenType::Address(addr)
    }
}
