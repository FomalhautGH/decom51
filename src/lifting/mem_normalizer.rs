use crate::common::{Addr, AddrSpace, Ir51, Operand, Register};
pub struct MemNormalizer;

impl MemNormalizer {
    pub fn normalize(ir_code: &mut [Ir51]) {
        for ir in ir_code {
            let newir = match *ir {
                Ir51::Move { dest, src } => Ir51::Move {
                    dest: MemNormalizer::opreg_to_opaddress(dest),
                    src: MemNormalizer::opreg_to_opaddress(src),
                },

                Ir51::Store {
                    space,
                    dest_ptr,
                    value,
                } => Ir51::Store {
                    space,
                    dest_ptr: MemNormalizer::opreg_to_opaddress(dest_ptr),
                    value: MemNormalizer::opreg_to_opaddress(value),
                },

                Ir51::Load {
                    space,
                    dest,
                    src_ptr,
                } => Ir51::Load {
                    space,
                    dest: MemNormalizer::opreg_to_opaddress(dest),
                    src_ptr: MemNormalizer::opreg_to_opaddress(src_ptr),
                },

                Ir51::BranchIf {
                    cond,
                    true_jmp,
                    false_jmp,
                } => Ir51::BranchIf {
                    cond: MemNormalizer::opreg_to_opaddress(cond),
                    true_jmp,
                    false_jmp,
                },

                Ir51::BinOp { op, dest, lhs, rhs } => Ir51::BinOp {
                    op,
                    dest: MemNormalizer::opreg_to_opaddress(dest),
                    lhs: MemNormalizer::opreg_to_opaddress(lhs),
                    rhs: MemNormalizer::opreg_to_opaddress(rhs),
                },

                _ => *ir,
            };

            *ir = newir
        }
    }

    fn opreg_to_opaddress(op: Operand) -> Operand {
        if let Operand::Register(reg) = op {
            MemNormalizer::resolve_register(&reg, 0) // TODO: 0 as current_bank is fine for now
        // since it's a rare occurrence in sdcc
        // binaries, maybe support it in the future?
        } else {
            op
        }
    }

    #[allow(clippy::identity_op)]
    fn resolve_register(reg: &Register, current_bank: u16) -> Operand {
        let make_var = |addr: Addr| Operand::Var {
            addr,
            space: AddrSpace::iData,
            size: 1,
        };

        match reg {
            Register::R0 => make_var(0 + (current_bank << 3)),
            Register::R1 => make_var(1 + (current_bank << 3)),
            Register::R2 => make_var(2 + (current_bank << 3)),
            Register::R3 => make_var(3 + (current_bank << 3)),
            Register::R4 => make_var(4 + (current_bank << 3)),
            Register::R5 => make_var(5 + (current_bank << 3)),
            Register::R6 => make_var(6 + (current_bank << 3)),
            Register::R7 => make_var(7 + (current_bank << 3)),

            Register::A => make_var(0xE0),
            Register::B => make_var(0xF0),
            Register::SP => make_var(0x81),
            Register::DPL => make_var(0x82),
            Register::DPH => make_var(0x83),

            Register::DPTR => make_var(0x82),

            Register::P0 => make_var(0x80),
            Register::P1 => make_var(0x90),
            Register::P2 => make_var(0xA0),
            Register::P3 => make_var(0xB0),

            Register::PCON => make_var(0x87),
            Register::TCON => make_var(0x88),
            Register::TMOD => make_var(0x89),
            Register::TL0 => make_var(0x8A),
            Register::TL1 => make_var(0x8B),
            Register::TH0 => make_var(0x8C),
            Register::TH1 => make_var(0x8D),

            Register::SCON => make_var(0x98),
            Register::SBUF => make_var(0x99),

            Register::IE => make_var(0xA8),
            Register::IP => make_var(0xB8),

            Register::C => make_var(0xD0),

            Register::PSW => todo!(
                "Since PSW contains flags every single bit (like the Carry) we do not support its presence yet"
            ),
        }
    }
}
