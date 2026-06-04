#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Register {
    R0, R1, R2, R3, R4, R5, R6, R7,
    A, B, C, PSW, SP,
    DPTR, DPL, DPH,
    P0, P1, P2, P3,
    PCON, TCON, TMOD, TL0, TL1, TH0, TH1, SCON, SBUF, IE, IP,
}
