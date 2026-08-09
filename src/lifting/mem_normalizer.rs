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
            Register::R0 => Operand::Var(0 + (current_bank << 3), AddrSpace::iData, 1),
            Register::R1 => Operand::Var(1 + (current_bank << 3), AddrSpace::iData, 1),
            Register::R2 => Operand::Var(2 + (current_bank << 3), AddrSpace::iData, 1),
            Register::R3 => Operand::Var(3 + (current_bank << 3), AddrSpace::iData, 1),
            Register::R4 => Operand::Var(4 + (current_bank << 3), AddrSpace::iData, 1),
            Register::R5 => Operand::Var(5 + (current_bank << 3), AddrSpace::iData, 1),
            Register::R6 => Operand::Var(6 + (current_bank << 3), AddrSpace::iData, 1),
            Register::R7 => Operand::Var(7 + (current_bank << 3), AddrSpace::iData, 1),

            Register::A => Operand::Var(0xE0, AddrSpace::iData, 1),
            Register::B => Operand::Var(0xF0, AddrSpace::iData, 1),
            Register::SP => Operand::Var(0x81, AddrSpace::iData, 1),
            Register::DPL => Operand::Var(0x82, AddrSpace::iData, 1),
            Register::DPH => Operand::Var(0x83, AddrSpace::iData, 1),

            Register::DPTR => Operand::Var(0x82, AddrSpace::iData, 1),

            Register::P0 => Operand::Var(0x80, AddrSpace::iData, 1),
            Register::P1 => Operand::Var(0x90, AddrSpace::iData, 1),
            Register::P2 => Operand::Var(0xA0, AddrSpace::iData, 1),
            Register::P3 => Operand::Var(0xB0, AddrSpace::iData, 1),

            Register::PCON => Operand::Var(0x87, AddrSpace::iData, 1),
            Register::TCON => Operand::Var(0x88, AddrSpace::iData, 1),
            Register::TMOD => Operand::Var(0x89, AddrSpace::iData, 1),
            Register::TL0 => Operand::Var(0x8A, AddrSpace::iData, 1),
            Register::TL1 => Operand::Var(0x8B, AddrSpace::iData, 1),
            Register::TH0 => Operand::Var(0x8C, AddrSpace::iData, 1),
            Register::TH1 => Operand::Var(0x8D, AddrSpace::iData, 1),

            Register::SCON => Operand::Var(0x98, AddrSpace::iData, 1),
            Register::SBUF => Operand::Var(0x99, AddrSpace::iData, 1),

            Register::IE => Operand::Var(0xA8, AddrSpace::iData, 1),
            Register::IP => Operand::Var(0xB8, AddrSpace::iData, 1),

            Register::C => Operand::Var(0xD0, AddrSpace::iData, 1),

            Register::PSW => todo!(
                "Since PSW contains flags every single bit (like the Carry) we do not support its presence yet"
            ),
        }
    }
}
