use crate::common::{AddrSpace, Ir51, Operand, Register};
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
        match reg {
            Register::R0 => Operand::Address(0 + (current_bank << 3), AddrSpace::iData),
            Register::R1 => Operand::Address(1 + (current_bank << 3), AddrSpace::iData),
            Register::R2 => Operand::Address(2 + (current_bank << 3), AddrSpace::iData),
            Register::R3 => Operand::Address(3 + (current_bank << 3), AddrSpace::iData),
            Register::R4 => Operand::Address(4 + (current_bank << 3), AddrSpace::iData),
            Register::R5 => Operand::Address(5 + (current_bank << 3), AddrSpace::iData),
            Register::R6 => Operand::Address(6 + (current_bank << 3), AddrSpace::iData),
            Register::R7 => Operand::Address(7 + (current_bank << 3), AddrSpace::iData),

            Register::A => Operand::Address(0xE0, AddrSpace::iData),
            Register::B => Operand::Address(0xF0, AddrSpace::iData),
            Register::PSW => Operand::Address(0xD0, AddrSpace::iData),
            Register::SP => Operand::Address(0x81, AddrSpace::iData),
            Register::DPL => Operand::Address(0x82, AddrSpace::iData),
            Register::DPH => Operand::Address(0x83, AddrSpace::iData),

            Register::DPTR => Operand::Address(0x82, AddrSpace::iData),

            Register::P0 => Operand::Address(0x80, AddrSpace::iData),
            Register::P1 => Operand::Address(0x90, AddrSpace::iData),
            Register::P2 => Operand::Address(0xA0, AddrSpace::iData),
            Register::P3 => Operand::Address(0xB0, AddrSpace::iData),

            Register::PCON => Operand::Address(0x87, AddrSpace::iData),
            Register::TCON => Operand::Address(0x88, AddrSpace::iData),
            Register::TMOD => Operand::Address(0x89, AddrSpace::iData),
            Register::TL0 => Operand::Address(0x8A, AddrSpace::iData),
            Register::TL1 => Operand::Address(0x8B, AddrSpace::iData),
            Register::TH0 => Operand::Address(0x8C, AddrSpace::iData),
            Register::TH1 => Operand::Address(0x8D, AddrSpace::iData),

            Register::SCON => Operand::Address(0x98, AddrSpace::iData),
            Register::SBUF => Operand::Address(0x99, AddrSpace::iData),

            Register::IE => Operand::Address(0xA8, AddrSpace::iData),
            Register::IP => Operand::Address(0xB8, AddrSpace::iData),

            Register::C => Operand::Address(0xD0, AddrSpace::iData),
        }
    }
}
