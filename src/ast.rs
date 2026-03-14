use arbitrary_int::prelude::*;
use bitbybit::bitenum;

/// Well-known register constants for pattern matching.
pub const SP: u4 = u4::new(13);
#[allow(dead_code)]
pub const LR: u4 = u4::new(14);
pub const PC: u4 = u4::new(15);

/// Instruction set to encode for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Isa {
    /// 16-bit Thumb + 32-bit Thumb-2 (Cortex-M, some Cortex-R)
    Thumb,
    /// 32-bit ARM (A32) instructions
    A32,
}

/// Target CPU, determines available instructions and default ISA.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cpu {
    /// ARM7TDMI — ARMv4T, Thumb-1 only
    Arm7Tdmi,
    /// Cortex-M4 — ARMv7E-M, Thumb-2 (no ARM mode)
    CortexM4,
    /// Cortex-A7 — ARMv7-A, Thumb-2 + ARM
    CortexA7,
    /// Cortex-R5 — ARMv7-R, Thumb-2 + ARM
    CortexR5,
}

impl Cpu {
    /// Name as expected by GNU as `-mcpu=`.
    pub fn gnu_name(self) -> &'static str {
        match self {
            Self::Arm7Tdmi => "arm7tdmi",
            Self::CortexM4 => "cortex-m4",
            Self::CortexA7 => "cortex-a7",
            Self::CortexR5 => "cortex-r5",
        }
    }

    /// Default ISA for this CPU.
    pub fn default_isa(self) -> Isa {
        match self {
            Self::CortexM4 => Isa::Thumb,
            _ => Isa::A32,
        }
    }

    /// Parse a CPU name string (as used by GNU as).
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "arm7tdmi" => Some(Self::Arm7Tdmi),
            "cortex-m4" => Some(Self::CortexM4),
            "cortex-a7" => Some(Self::CortexA7),
            "cortex-r5" => Some(Self::CortexR5),
            _ => None,
        }
    }
}

/// ARM condition codes, usable directly in `#[bitfield]` structs as a `u4` field.
#[bitenum(u4, exhaustive = true)]
#[derive(Debug, PartialEq, Eq)]
pub enum Condition {
    Eq = 0,
    Ne = 1,
    Cs = 2,
    Cc = 3,
    Mi = 4,
    Pl = 5,
    Vs = 6,
    Vc = 7,
    Hi = 8,
    Ls = 9,
    Ge = 10,
    Lt = 11,
    Gt = 12,
    Le = 13,
    Al = 14,
    /// Architecturally "never" / unconditional in some A32 instruction spaces.
    #[allow(dead_code)]
    Nv = 15,
}

impl Condition {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "EQ" => Some(Self::Eq),
            "NE" => Some(Self::Ne),
            "CS" | "HS" => Some(Self::Cs),
            "CC" | "LO" => Some(Self::Cc),
            "MI" => Some(Self::Mi),
            "PL" => Some(Self::Pl),
            "VS" => Some(Self::Vs),
            "VC" => Some(Self::Vc),
            "HI" => Some(Self::Hi),
            "LS" => Some(Self::Ls),
            "GE" => Some(Self::Ge),
            "LT" => Some(Self::Lt),
            "GT" => Some(Self::Gt),
            "LE" => Some(Self::Le),
            "AL" => Some(Self::Al),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShiftType {
    Lsl,
    Lsr,
    Asr,
    Ror,
    Rrx,
}

impl ShiftType {
    pub fn encoding(self) -> u32 {
        match self {
            Self::Lsl => 0,
            Self::Lsr => 1,
            Self::Asr => 2,
            Self::Ror => 3,
            Self::Rrx => 3, // RRX is ROR #0
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mnemonic {
    // --- Data processing ---
    Mov,
    Mvn,
    Add,
    Adc,
    Sub,
    Sbc,
    Rsb,
    Rsc,
    And,
    Orr,
    Orn,
    Eor,
    Bic,
    Cmp,
    Cmn,
    Tst,
    Teq,
    // --- Shifts ---
    Lsl,
    Lsr,
    Asr,
    Ror,
    Rrx,
    // --- Move wide ---
    Movw,
    Movt,
    // --- Multiply / divide ---
    Mul,
    Mla,
    Mls,
    Umull,
    Smull,
    Umlal,
    Smlal,
    Sdiv,
    Udiv,
    Umaal,
    // --- DSP multiply ---
    Smmul,
    Smmulr,
    Smmla,
    Smmlar,
    Smmls,
    Smmlsr,
    Smulbb,
    Smulbt,
    Smultb,
    Smultt,
    Smulwb,
    Smulwt,
    Smlabb,
    Smlabt,
    Smlatb,
    Smlatt,
    Smlawb,
    Smlawt,
    Smlalbb,
    Smlalbt,
    Smlaltb,
    Smlaltt,
    Smuad,
    Smuadx,
    Smusd,
    Smusdx,
    Smlad,
    Smladx,
    Smlsd,
    Smlsdx,
    Smlald,
    Smlaldx,
    Smlsld,
    Smlsldx,
    Usad8,
    Usada8,
    // --- Load / store ---
    Ldr,
    Ldrb,
    Ldrh,
    Ldrsb,
    Ldrsh,
    Str,
    Strb,
    Strh,
    Ldrd,
    Strd,
    // --- Load / store multiple ---
    Ldm,
    Ldmia,
    Ldmfd,
    Ldmib,
    Ldmed,
    Ldmda,
    Ldmfa,
    Ldmdb,
    Ldmea,
    Stm,
    Stmia,
    Stmea,
    Stmib,
    Stmfa,
    Stmda,
    Stmed,
    Stmdb,
    Stmfd,
    Push,
    Pop,
    // --- Exclusive load / store ---
    Ldrex,
    Strex,
    Ldrexb,
    Strexb,
    Ldrexh,
    Strexh,
    Ldrexd,
    Strexd,
    Clrex,
    // --- Unprivileged load / store ---
    Ldrt,
    Strt,
    Ldrbt,
    Strbt,
    Ldrht,
    Strht,
    Ldrsbt,
    Ldrsht,
    // --- Branch ---
    B,
    Bl,
    Bx,
    Blx,
    Cbz,
    Cbnz,
    Tbb,
    Tbh,
    // --- IT block ---
    It,
    // --- Bit manipulation ---
    Clz,
    Rbit,
    Bfi,
    Bfc,
    Ubfx,
    Sbfx,
    // --- Saturation ---
    Ssat,
    Usat,
    Ssat16,
    Usat16,
    // --- Byte reversal / extend ---
    Rev,
    Rev16,
    Revsh,
    Sxth,
    Sxtb,
    Uxth,
    Uxtb,
    Sxtb16,
    Uxtb16,
    // --- Extend and add ---
    Sxtab,
    Sxtah,
    Uxtab,
    Uxtah,
    Sxtab16,
    Uxtab16,
    // --- Parallel arithmetic (DSP) ---
    Sadd16,
    Sadd8,
    Ssub16,
    Ssub8,
    Uadd16,
    Uadd8,
    Usub16,
    Usub8,
    Qadd16,
    Qadd8,
    Qsub16,
    Qsub8,
    Shadd16,
    Shadd8,
    Shsub16,
    Shsub8,
    Uhadd16,
    Uhadd8,
    Uhsub16,
    Uhsub8,
    Uqadd16,
    Uqadd8,
    Uqsub16,
    Uqsub8,
    Sasx,
    Ssax,
    Uasx,
    Usax,
    Qasx,
    Qsax,
    Shasx,
    Shsax,
    Uhasx,
    Uhsax,
    Uqasx,
    Uqsax,
    // --- Saturating arithmetic ---
    Qadd,
    Qdadd,
    Qsub,
    Qdsub,
    // --- Packing ---
    Pkhbt,
    Pkhtb,
    Sel,
    // --- System ---
    Svc,
    Mrs,
    Msr,
    Cpsie,
    Cpsid,
    Nop,
    Yield,
    Bkpt,
    Wfi,
    Wfe,
    Sev,
    Dmb,
    Dsb,
    Isb,
    Dbg,
    Setend,
    Swp,
    Swpb,
    // --- Misc ---
    Adr,
    Neg,
    Pld,
    Pldw,
    Pli,
    // --- VFP / Floating-point ---
    Vadd,
    Vsub,
    Vmul,
    Vdiv,
    Vsqrt,
    Vabs,
    Vneg,
    Vmov,
    Vcmp,
    Vcmpe,
    Vcvt,
    Vcvtr,
    Vldr,
    Vstr,
    Vpush,
    Vpop,
    Vmrs,
    Vmsr,
}

impl Mnemonic {
    /// Whether this mnemonic is a test/compare that always sets flags.
    pub fn implicit_s(self) -> bool {
        matches!(self, Self::Cmp | Self::Cmn | Self::Tst | Self::Teq)
    }

    /// Whether this mnemonic uses Rn (vs ignoring it like MOV/MVN).
    pub fn uses_rn(self) -> bool {
        !matches!(self, Self::Mov | Self::Mvn)
    }

    /// Whether this mnemonic writes to Rd (vs test/compare instructions).
    #[allow(dead_code)]
    pub fn writes_rd(self) -> bool {
        !matches!(self, Self::Cmp | Self::Cmn | Self::Tst | Self::Teq)
    }
}

/// Mapping from mnemonic name to enum value. Sorted longest first for parsing.
pub const MNEMONICS: &[(&str, Mnemonic)] = &[
    // --- 7 characters ---
    ("SMLALBB", Mnemonic::Smlalbb),
    ("SMLALBT", Mnemonic::Smlalbt),
    ("SMLALTB", Mnemonic::Smlaltb),
    ("SMLALTT", Mnemonic::Smlaltt),
    ("SMLALDX", Mnemonic::Smlaldx),
    ("SMLSLDX", Mnemonic::Smlsldx),
    ("SXTAB16", Mnemonic::Sxtab16),
    ("UXTAB16", Mnemonic::Uxtab16),
    ("UQADD16", Mnemonic::Uqadd16),
    ("UQSUB16", Mnemonic::Uqsub16),
    ("UHADD16", Mnemonic::Uhadd16),
    ("UHSUB16", Mnemonic::Uhsub16),
    ("SHADD16", Mnemonic::Shadd16),
    ("SHSUB16", Mnemonic::Shsub16),
    // --- 6 characters ---
    ("LDREXB", Mnemonic::Ldrexb),
    ("STREXB", Mnemonic::Strexb),
    ("LDREXH", Mnemonic::Ldrexh),
    ("STREXH", Mnemonic::Strexh),
    ("LDREXD", Mnemonic::Ldrexd),
    ("STREXD", Mnemonic::Strexd),
    ("SMULBB", Mnemonic::Smulbb),
    ("SMULBT", Mnemonic::Smulbt),
    ("SMULTB", Mnemonic::Smultb),
    ("SMULTT", Mnemonic::Smultt),
    ("SMULWB", Mnemonic::Smulwb),
    ("SMULWT", Mnemonic::Smulwt),
    ("SMLABB", Mnemonic::Smlabb),
    ("SMLABT", Mnemonic::Smlabt),
    ("SMLATB", Mnemonic::Smlatb),
    ("SMLATT", Mnemonic::Smlatt),
    ("SMLAWB", Mnemonic::Smlawb),
    ("SMLAWT", Mnemonic::Smlawt),
    ("SMLALD", Mnemonic::Smlald),
    ("SMLSLD", Mnemonic::Smlsld),
    ("SMUADX", Mnemonic::Smuadx),
    ("SMUSDX", Mnemonic::Smusdx),
    ("SMLADX", Mnemonic::Smladx),
    ("SMLSDX", Mnemonic::Smlsdx),
    ("SMMULR", Mnemonic::Smmulr),
    ("SMMLAR", Mnemonic::Smmlar),
    ("SMMLSR", Mnemonic::Smmlsr),
    ("USADA8", Mnemonic::Usada8),
    ("SETEND", Mnemonic::Setend),
    ("SSAT16", Mnemonic::Ssat16),
    ("USAT16", Mnemonic::Usat16),
    ("SADD16", Mnemonic::Sadd16),
    ("SSUB16", Mnemonic::Ssub16),
    ("UADD16", Mnemonic::Uadd16),
    ("USUB16", Mnemonic::Usub16),
    ("QADD16", Mnemonic::Qadd16),
    ("QSUB16", Mnemonic::Qsub16),
    ("UQADD8", Mnemonic::Uqadd8),
    ("UQSUB8", Mnemonic::Uqsub8),
    ("UHADD8", Mnemonic::Uhadd8),
    ("UHSUB8", Mnemonic::Uhsub8),
    ("SHADD8", Mnemonic::Shadd8),
    ("SHSUB8", Mnemonic::Shsub8),
    ("SXTB16", Mnemonic::Sxtb16),
    ("UXTB16", Mnemonic::Uxtb16),
    ("LDRSBT", Mnemonic::Ldrsbt),
    ("LDRSHT", Mnemonic::Ldrsht),
    ("LDRSB", Mnemonic::Ldrsb),
    ("LDRSH", Mnemonic::Ldrsh),
    ("LDMIA", Mnemonic::Ldmia),
    ("LDMFD", Mnemonic::Ldmfd),
    ("LDMIB", Mnemonic::Ldmib),
    ("LDMED", Mnemonic::Ldmed),
    ("LDMDA", Mnemonic::Ldmda),
    ("LDMFA", Mnemonic::Ldmfa),
    ("LDMDB", Mnemonic::Ldmdb),
    ("LDMEA", Mnemonic::Ldmea),
    ("STMIA", Mnemonic::Stmia),
    ("STMEA", Mnemonic::Stmea),
    ("STMIB", Mnemonic::Stmib),
    ("STMFA", Mnemonic::Stmfa),
    ("STMDA", Mnemonic::Stmda),
    ("STMED", Mnemonic::Stmed),
    ("STMDB", Mnemonic::Stmdb),
    ("STMFD", Mnemonic::Stmfd),
    // --- 5 characters ---
    ("UMULL", Mnemonic::Umull),
    ("SMULL", Mnemonic::Smull),
    ("UMLAL", Mnemonic::Umlal),
    ("SMLAL", Mnemonic::Smlal),
    ("UMAAL", Mnemonic::Umaal),
    ("SMMUL", Mnemonic::Smmul),
    ("SMMLA", Mnemonic::Smmla),
    ("SMMLS", Mnemonic::Smmls),
    ("SMUAD", Mnemonic::Smuad),
    ("SMUSD", Mnemonic::Smusd),
    ("SMLAD", Mnemonic::Smlad),
    ("SMLSD", Mnemonic::Smlsd),
    ("USAD8", Mnemonic::Usad8),
    ("YIELD", Mnemonic::Yield),
    ("LDREX", Mnemonic::Ldrex),
    ("STREX", Mnemonic::Strex),
    ("CLREX", Mnemonic::Clrex),
    ("LDRBT", Mnemonic::Ldrbt),
    ("STRBT", Mnemonic::Strbt),
    ("LDRHT", Mnemonic::Ldrht),
    ("STRHT", Mnemonic::Strht),
    ("REV16", Mnemonic::Rev16),
    ("REVSH", Mnemonic::Revsh),
    ("SXTAB", Mnemonic::Sxtab),
    ("SXTAH", Mnemonic::Sxtah),
    ("UXTAB", Mnemonic::Uxtab),
    ("UXTAH", Mnemonic::Uxtah),
    ("PKHBT", Mnemonic::Pkhbt),
    ("PKHTB", Mnemonic::Pkhtb),
    ("QDADD", Mnemonic::Qdadd),
    ("QDSUB", Mnemonic::Qdsub),
    ("CPSIE", Mnemonic::Cpsie),
    ("CPSID", Mnemonic::Cpsid),
    ("SHASX", Mnemonic::Shasx),
    ("SHSAX", Mnemonic::Shsax),
    ("UHASX", Mnemonic::Uhasx),
    ("UHSAX", Mnemonic::Uhsax),
    ("UQASX", Mnemonic::Uqasx),
    ("UQSAX", Mnemonic::Uqsax),
    ("SADD8", Mnemonic::Sadd8),
    ("SSUB8", Mnemonic::Ssub8),
    ("UADD8", Mnemonic::Uadd8),
    ("USUB8", Mnemonic::Usub8),
    ("QADD8", Mnemonic::Qadd8),
    ("QSUB8", Mnemonic::Qsub8),
    ("LDRT", Mnemonic::Ldrt), // 4 char
    ("STRT", Mnemonic::Strt),
    // --- 4 characters ---
    ("PLDW", Mnemonic::Pldw),
    ("SWPB", Mnemonic::Swpb),
    ("MOVW", Mnemonic::Movw),
    ("MOVT", Mnemonic::Movt),
    ("LDRB", Mnemonic::Ldrb),
    ("LDRH", Mnemonic::Ldrh),
    ("LDRD", Mnemonic::Ldrd),
    ("STRB", Mnemonic::Strb),
    ("STRH", Mnemonic::Strh),
    ("STRD", Mnemonic::Strd),
    ("PUSH", Mnemonic::Push),
    ("BKPT", Mnemonic::Bkpt),
    ("CBNZ", Mnemonic::Cbnz),
    ("SXTH", Mnemonic::Sxth),
    ("SXTB", Mnemonic::Sxtb),
    ("UXTH", Mnemonic::Uxth),
    ("UXTB", Mnemonic::Uxtb),
    ("RBIT", Mnemonic::Rbit),
    ("UBFX", Mnemonic::Ubfx),
    ("SBFX", Mnemonic::Sbfx),
    ("SDIV", Mnemonic::Sdiv),
    ("UDIV", Mnemonic::Udiv),
    ("SSAT", Mnemonic::Ssat),
    ("USAT", Mnemonic::Usat),
    ("QADD", Mnemonic::Qadd),
    ("QSUB", Mnemonic::Qsub),
    ("SASX", Mnemonic::Sasx),
    ("SSAX", Mnemonic::Ssax),
    ("UASX", Mnemonic::Uasx),
    ("USAX", Mnemonic::Usax),
    ("QASX", Mnemonic::Qasx),
    ("QSAX", Mnemonic::Qsax),
    // --- 3 characters ---
    ("POP", Mnemonic::Pop),
    ("LDR", Mnemonic::Ldr),
    ("STR", Mnemonic::Str),
    ("LDM", Mnemonic::Ldm),
    ("STM", Mnemonic::Stm),
    ("MOV", Mnemonic::Mov),
    ("MVN", Mnemonic::Mvn),
    ("ADD", Mnemonic::Add),
    ("ADC", Mnemonic::Adc),
    ("SUB", Mnemonic::Sub),
    ("SBC", Mnemonic::Sbc),
    ("RSB", Mnemonic::Rsb),
    ("RSC", Mnemonic::Rsc),
    ("AND", Mnemonic::And),
    ("ORR", Mnemonic::Orr),
    ("ORN", Mnemonic::Orn),
    ("EOR", Mnemonic::Eor),
    ("BIC", Mnemonic::Bic),
    ("CMP", Mnemonic::Cmp),
    ("CMN", Mnemonic::Cmn),
    ("TST", Mnemonic::Tst),
    ("TEQ", Mnemonic::Teq),
    ("LSL", Mnemonic::Lsl),
    ("LSR", Mnemonic::Lsr),
    ("ASR", Mnemonic::Asr),
    ("ROR", Mnemonic::Ror),
    ("RRX", Mnemonic::Rrx),
    ("MUL", Mnemonic::Mul),
    ("MLA", Mnemonic::Mla),
    ("MLS", Mnemonic::Mls),
    ("BLX", Mnemonic::Blx),
    ("SVC", Mnemonic::Svc),
    ("NOP", Mnemonic::Nop),
    ("WFI", Mnemonic::Wfi),
    ("WFE", Mnemonic::Wfe),
    ("SEV", Mnemonic::Sev),
    ("DMB", Mnemonic::Dmb),
    ("DSB", Mnemonic::Dsb),
    ("ISB", Mnemonic::Isb),
    ("ADR", Mnemonic::Adr),
    ("CBZ", Mnemonic::Cbz),
    ("CLZ", Mnemonic::Clz),
    ("REV", Mnemonic::Rev),
    ("NEG", Mnemonic::Neg),
    ("BFI", Mnemonic::Bfi),
    ("BFC", Mnemonic::Bfc),
    ("MRS", Mnemonic::Mrs),
    ("MSR", Mnemonic::Msr),
    ("TBB", Mnemonic::Tbb),
    ("TBH", Mnemonic::Tbh),
    ("DBG", Mnemonic::Dbg),
    ("SEL", Mnemonic::Sel),
    ("SWP", Mnemonic::Swp),
    ("PLD", Mnemonic::Pld),
    ("PLI", Mnemonic::Pli),
    // --- VFP (parsed specially but listed here for mnemonic recognition) ---
    ("VCMPE", Mnemonic::Vcmpe),
    ("VPUSH", Mnemonic::Vpush),
    ("VPOP", Mnemonic::Vpop),
    ("VSQRT", Mnemonic::Vsqrt),
    ("VCVTR", Mnemonic::Vcvtr),
    ("VCVT", Mnemonic::Vcvt),
    ("VCMP", Mnemonic::Vcmp),
    ("VADD", Mnemonic::Vadd),
    ("VSUB", Mnemonic::Vsub),
    ("VMUL", Mnemonic::Vmul),
    ("VDIV", Mnemonic::Vdiv),
    ("VABS", Mnemonic::Vabs),
    ("VNEG", Mnemonic::Vneg),
    ("VMOV", Mnemonic::Vmov),
    ("VLDR", Mnemonic::Vldr),
    ("VSTR", Mnemonic::Vstr),
    ("VMRS", Mnemonic::Vmrs),
    ("VMSR", Mnemonic::Vmsr),
    // --- 2 characters ---
    ("BL", Mnemonic::Bl),
    ("BX", Mnemonic::Bx),
    ("IT", Mnemonic::It),
    // --- 1 character ---
    ("B", Mnemonic::B),
];

#[derive(Debug, Clone, PartialEq)]
pub enum MemOffset {
    Imm(i32),
    /// Register offset with subtract flag (true = subtract, false = add).
    Reg(u4, bool),
    /// Shifted register offset with subtract flag.
    RegShift(u4, ShiftType, u8, bool),
}

/// An expression that may reference labels, resolved at encode time.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(i64),
    Symbol(String),
    /// Local (numeric) label reference: (label_number, is_forward).
    /// `1f` = LocalLabel(1, true), `1b` = LocalLabel(1, false).
    LocalLabel(u32, bool),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
}

/// Floating-point precision suffix (.F32 / .F64).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpSize {
    F32,
    F64,
}

/// VCVT conversion kind, parsed from the double-suffix (e.g. .F32.S32).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VcvtKind {
    F32ToF64,
    F64ToF32,
    F32ToS32,
    F32ToU32,
    F64ToS32,
    F64ToU32,
    S32ToF32,
    U32ToF32,
    S32ToF64,
    U32ToF64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    Reg(u4),
    Imm(i64),
    Expr(Expr),
    RegList(u16),
    Memory {
        base: u4,
        offset: MemOffset,
        pre_index: bool,
        writeback: bool,
    },
    Shifted(u4, ShiftType, Box<Operand>),
    SysReg(u8),
    /// Single-precision FP register S0-S31
    SReg(u8),
    /// Double-precision FP register D0-D15 (D0-D31 for VFPv3-D32)
    DReg(u8),
    /// FP register list for VPUSH/VPOP: {Sx-Sy} or {Dx-Dy}
    FpRegList {
        start: u8,
        count: u8,
        double: bool,
    },
    /// APSR_nzcv (used in VMRS APSR_nzcv, FPSCR)
    ApsrNzcv,
    /// FPSCR system register (for VMRS/VMSR)
    Fpscr,
    /// FP immediate (8-bit encoded float)
    FpImm(f64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Instruction {
    pub mnemonic: Mnemonic,
    pub condition: Option<Condition>,
    pub set_flags: bool,
    pub wide: bool,
    pub writeback: bool,
    pub operands: Vec<Operand>,
    pub line: usize,
    /// FP precision suffix (.F32 / .F64) for VFP instructions.
    pub fp_size: Option<FpSize>,
    /// VCVT conversion kind (e.g. .F32.S32).
    pub vcvt_kind: Option<VcvtKind>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Directive {
    Section(String),
    Global(String),
    Align(u32, Option<u8>),
    Balign(u32, Option<u8>),
    Word(Vec<Expr>),
    Short(Vec<Expr>),
    Byte(Vec<Expr>),
    Space(u32, u8),
    Ascii(String),
    Asciz(String),
    Thumb,
    Arm,
    SyntaxUnified,
    Equ(String, Expr),
    Type(String, String),
    Fpu(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Label(String, usize),
    Instruction(Instruction),
    Directive(Directive, usize),
}
