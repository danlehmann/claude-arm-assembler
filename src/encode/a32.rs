use std::collections::HashMap;

use arbitrary_int::prelude::*;
use bitbybit::bitfield;

use crate::ast::*;
use crate::error::AsmError;

use super::{resolve_expr_u32, EncodedInst};

// ---------------------------------------------------------------------------
// Bitfield structs for A32 instruction formats
// ---------------------------------------------------------------------------

/// Data processing - immediate operand
/// cond 001 opcode S Rn Rd rotate imm8
#[bitfield(u32)]
struct DpImm {
    #[bits(0..=7, rw)]
    imm8: u8,
    #[bits(8..=11, rw)]
    rotate: u4,
    #[bits(12..=15, rw)]
    rd: u4,
    #[bits(16..=19, rw)]
    rn: u4,
    #[bit(20, rw)]
    s: bool,
    #[bits(21..=24, rw)]
    opcode: u4,
    #[bit(25, rw)]
    imm_flag: bool, // 1 for immediate operand
    #[bits(26..=27, rw)]
    class: u2, // 00 for data processing
    #[bits(28..=31, rw)]
    cond: Condition,
}

impl DpImm {
    fn new() -> Self {
        Self::ZERO.with_imm_flag(true)
    }
}

/// Data processing - register operand (immediate shift)
/// cond 000 opcode S Rn Rd shift_imm shift_type 0 Rm
#[bitfield(u32)]
struct DpReg {
    #[bits(0..=3, rw)]
    rm: u4,
    // bit 4: 0 (register shift indicator)
    #[bits(5..=6, rw)]
    shift_type: u2,
    #[bits(7..=11, rw)]
    shift_imm: u5,
    #[bits(12..=15, rw)]
    rd: u4,
    #[bits(16..=19, rw)]
    rn: u4,
    #[bit(20, rw)]
    s: bool,
    #[bits(21..=24, rw)]
    opcode: u4,
    // bits 25-27: 000 (all zero for register form)
    #[bits(28..=31, rw)]
    cond: Condition,
}

impl DpReg {
    fn new() -> Self {
        Self::ZERO
    }
}

/// Data processing - register operand (register-controlled shift)
/// cond 000 opcode S Rn Rd Rs 0 shift_type 1 Rm
#[bitfield(u32)]
struct DpRegShift {
    #[bits(0..=3, rw)]
    rm: u4,
    #[bit(4, rw)]
    fixed4: bool, // always 1 (register shift indicator)
    #[bits(5..=6, rw)]
    shift_type: u2,
    // bit 7: 0
    #[bits(8..=11, rw)]
    rs: u4,
    #[bits(12..=15, rw)]
    rd: u4,
    #[bits(16..=19, rw)]
    rn: u4,
    #[bit(20, rw)]
    s: bool,
    #[bits(21..=24, rw)]
    opcode: u4,
    // bits 25-27: 000
    #[bits(28..=31, rw)]
    cond: Condition,
}

impl DpRegShift {
    fn new() -> Self {
        Self::ZERO.with_fixed4(true)
    }
}

/// Load/Store immediate offset
/// cond 01 I P U B W L Rn Rt offset12
#[bitfield(u32)]
struct LdrStrImm {
    #[bits(0..=11, rw)]
    offset12: u12,
    #[bits(12..=15, rw)]
    rt: u4,
    #[bits(16..=19, rw)]
    rn: u4,
    #[bit(20, rw)]
    load: bool,
    #[bit(21, rw)]
    writeback: bool,
    #[bit(22, rw)]
    byte: bool,
    #[bit(23, rw)]
    add: bool,
    #[bit(24, rw)]
    pre: bool,
    // bit 25: I (0=immediate)
    #[bit(26, rw)]
    class_bit: bool, // 1 for load/store
    // bit 27: 0
    #[bits(28..=31, rw)]
    cond: Condition,
}

impl LdrStrImm {
    fn new() -> Self {
        Self::ZERO.with_class_bit(true)
    }
}

/// Branch (B/BL)
/// cond 101 L offset24
#[bitfield(u32)]
struct Branch {
    #[bits(0..=23, rw)]
    offset: u24,
    #[bit(24, rw)]
    link: bool,
    #[bits(25..=27, rw)]
    class: u3, // 101 for branch
    #[bits(28..=31, rw)]
    cond: Condition,
}

impl Branch {
    fn new() -> Self {
        Self::ZERO.with_class(u3::new(0b101))
    }
}

/// Halfword/signed load/store - immediate offset
/// cond 000P U1WL Rn Rt imm4H 1SH1 imm4L
#[bitfield(u32)]
struct HalfWordImm {
    #[bits(0..=3, rw)]
    imm4l: u4,
    #[bit(4, rw)]
    fixed4: bool, // always 1
    #[bit(5, rw)]
    h: bool,
    #[bit(6, rw)]
    s: bool,
    #[bit(7, rw)]
    fixed7: bool, // always 1
    #[bits(8..=11, rw)]
    imm4h: u4,
    #[bits(12..=15, rw)]
    rt: u4,
    #[bits(16..=19, rw)]
    rn: u4,
    #[bit(20, rw)]
    load: bool,
    #[bit(21, rw)]
    writeback: bool,
    #[bit(22, rw)]
    imm_flag: bool, // 1 for immediate offset
    #[bit(23, rw)]
    add: bool,
    #[bit(24, rw)]
    pre: bool,
    // bits 25-27: 000
    #[bits(28..=31, rw)]
    cond: Condition,
}

impl HalfWordImm {
    fn new() -> Self {
        Self::ZERO
            .with_fixed4(true)
            .with_fixed7(true)
            .with_imm_flag(true)
    }
}

/// Halfword/signed load/store - register offset
/// cond 000P U0WL Rn Rt 0000 1SH1 Rm
#[bitfield(u32)]
struct HalfWordReg {
    #[bits(0..=3, rw)]
    rm: u4,
    #[bit(4, rw)]
    fixed4: bool, // always 1
    #[bit(5, rw)]
    h: bool,
    #[bit(6, rw)]
    s: bool,
    #[bit(7, rw)]
    fixed7: bool, // always 1
    // bits 8-11: 0000
    #[bits(12..=15, rw)]
    rt: u4,
    #[bits(16..=19, rw)]
    rn: u4,
    #[bit(20, rw)]
    load: bool,
    #[bit(21, rw)]
    writeback: bool,
    // bit 22: 0 (register offset)
    #[bit(23, rw)]
    add: bool,
    #[bit(24, rw)]
    pre: bool,
    // bits 25-27: 000
    #[bits(28..=31, rw)]
    cond: Condition,
}

impl HalfWordReg {
    fn new() -> Self {
        Self::ZERO.with_fixed4(true).with_fixed7(true)
    }
}

/// Load/store register offset (with optional shift)
/// cond 011P UBWL Rn Rt shift_imm type 0 Rm
#[bitfield(u32)]
struct LdrStrReg {
    #[bits(0..=3, rw)]
    rm: u4,
    // bit 4: 0
    #[bits(5..=6, rw)]
    shift_type: u2,
    #[bits(7..=11, rw)]
    shift_imm: u5,
    #[bits(12..=15, rw)]
    rt: u4,
    #[bits(16..=19, rw)]
    rn: u4,
    #[bit(20, rw)]
    load: bool,
    #[bit(21, rw)]
    writeback: bool,
    #[bit(22, rw)]
    byte: bool,
    #[bit(23, rw)]
    add: bool,
    #[bit(24, rw)]
    pre: bool,
    #[bits(25..=27, rw)]
    class: u3, // 011 for register offset load/store
    #[bits(28..=31, rw)]
    cond: Condition,
}

impl LdrStrReg {
    fn new() -> Self {
        Self::ZERO.with_class(u3::new(0b011))
    }
}

/// Load/store multiple
/// cond 100P U0WL Rn reglist
#[bitfield(u32)]
struct LdmStm {
    #[bits(0..=15, rw)]
    reglist: u16,
    #[bits(16..=19, rw)]
    rn: u4,
    #[bit(20, rw)]
    load: bool,
    #[bit(21, rw)]
    writeback: bool,
    // bit 22: 0
    #[bit(23, rw)]
    add: bool,
    #[bit(24, rw)]
    pre: bool,
    #[bits(25..=27, rw)]
    class: u3, // 100 for load/store multiple
    #[bits(28..=31, rw)]
    cond: Condition,
}

impl LdmStm {
    fn new() -> Self {
        Self::ZERO.with_class(u3::new(0b100))
    }
}

/// Multiply accumulate: MUL/MLA/MLS/long multiply
/// cond 0000 opcode S RdHi RdLo Rm 1001 Rn
#[bitfield(u32)]
struct MulLong {
    #[bits(0..=3, rw)]
    rn: u4,
    #[bit(4, rw)]
    fixed4: bool, // always 1
    // bits 5-6: 00
    #[bit(7, rw)]
    fixed7: bool, // always 1
    #[bits(8..=11, rw)]
    rm: u4,
    #[bits(12..=15, rw)]
    rdlo: u4,
    #[bits(16..=19, rw)]
    rdhi: u4,
    #[bit(20, rw)]
    s: bool,
    #[bits(21..=27, rw)]
    op: u7,
    #[bits(28..=31, rw)]
    cond: Condition,
}

impl MulLong {
    fn new() -> Self {
        Self::ZERO.with_fixed4(true).with_fixed7(true)
    }
}

/// Media multiply instructions (SMMUL, SMMLA, SMMLS, SMUAD, SMUSD, SMLAD, SMLSD, USAD8, etc.)
/// cond op1[27:20] Rd Ra Rm op2[7:4] Rn
#[bitfield(u32)]
struct MediaMul {
    #[bits(0..=3, rw)]
    rn: u4,
    #[bits(4..=7, rw)]
    op2: u4,
    #[bits(8..=11, rw)]
    rm: u4,
    #[bits(12..=15, rw)]
    ra: u4,
    #[bits(16..=19, rw)]
    rd: u4,
    #[bits(20..=27, rw)]
    op1: u8,
    #[bits(28..=31, rw)]
    cond: Condition,
}

/// Halfword multiply (SMULxy, SMLAxy, SMULWy, SMLAWy)
/// cond op[27:20] Rd Rn/Ra Rs/Rm 1 y x 0 Rm/Rn
#[bitfield(u32)]
struct HalfMul {
    #[bits(0..=3, rw)]
    rm: u4,
    // bit 4: 0
    #[bit(5, rw)]
    x: bool,
    #[bit(6, rw)]
    y: bool,
    #[bit(7, rw)]
    fixed7: bool, // always 1
    #[bits(8..=11, rw)]
    rs: u4,
    #[bits(12..=15, rw)]
    ra: u4,
    #[bits(16..=19, rw)]
    rd: u4,
    #[bits(20..=27, rw)]
    op: u8,
    #[bits(28..=31, rw)]
    cond: Condition,
}

impl HalfMul {
    fn new() -> Self {
        Self::ZERO.with_fixed7(true)
    }
}

/// SWP / SWPB
/// cond 0001 0B00 Rn Rt 0000 1001 Rm
#[bitfield(u32)]
struct Swp {
    #[bits(0..=3, rw)]
    rm: u4,
    #[bits(4..=7, rw)]
    fixed_low: u4, // 1001
    // bits 8-11: 0000
    #[bits(12..=15, rw)]
    rt: u4,
    #[bits(16..=19, rw)]
    rn: u4,
    // bit 20: 0
    // bit 21: 0
    #[bit(22, rw)]
    byte: bool,
    #[bits(23..=27, rw)]
    fixed_hi: u5, // 00010
    #[bits(28..=31, rw)]
    cond: Condition,
}

impl Swp {
    fn new() -> Self {
        Self::ZERO
            .with_fixed_low(u4::new(0b1001))
            .with_fixed_hi(u5::new(0b00010))
    }
}

/// Preload data/instruction (PLD, PLDW, PLI)
/// 1111 01 I D U R 01 Rn 1111 offset12
#[bitfield(u32)]
struct Preload {
    #[bits(0..=11, rw)]
    offset12: u12,
    #[bits(12..=15, rw)]
    fixed_f: u4, // 1111
    #[bits(16..=19, rw)]
    rn: u4,
    #[bit(20, rw)]
    fixed_1: bool, // 1
    #[bit(21, rw)]
    is_write: bool, // 0=read/PLD, 1=write/PLDW... actually inverted
    #[bit(22, rw)]
    is_data: bool, // 1=data (PLD/PLDW), 0=instruction (PLI)
    #[bit(23, rw)]
    add: bool,
    #[bit(24, rw)]
    is_pld: bool, // 1 for PLD/PLDW, 0 for PLI ... combined with bit 22
    #[bit(25, rw)]
    reg_offset: bool, // I bit: 0=immediate, 1=register
    #[bits(26..=31, rw)]
    fixed_hi: u6, // 111101
}

impl Preload {
    fn pld() -> Self {
        Self::ZERO
            .with_fixed_f(u4::new(0xF))
            .with_fixed_1(true)
            .with_is_data(true)
            .with_is_pld(true)
            .with_fixed_hi(u6::new(0b111101))
    }

    fn pldw() -> Self {
        Self::ZERO
            .with_fixed_f(u4::new(0xF))
            .with_fixed_1(true)
            .with_is_pld(true)
            .with_fixed_hi(u6::new(0b111101))
    }

    fn pli() -> Self {
        Self::ZERO
            .with_fixed_f(u4::new(0xF))
            .with_fixed_1(true)
            .with_is_pld(false)
            .with_is_data(true)
            .with_fixed_hi(u6::new(0b111101))
    }
}

/// Saturation 16-bit (SSAT16, USAT16)
/// cond 0110 1x10 sat_imm Rd 1111 0011 Rn
#[bitfield(u32)]
struct Sat16 {
    #[bits(0..=3, rw)]
    rn: u4,
    #[bits(4..=7, rw)]
    fixed_low: u4, // 0011
    #[bits(8..=11, rw)]
    fixed_f: u4, // 1111
    #[bits(12..=15, rw)]
    rd: u4,
    #[bits(16..=19, rw)]
    sat_imm: u4,
    #[bits(20..=27, rw)]
    op: u8,
    #[bits(28..=31, rw)]
    cond: Condition,
}

impl Sat16 {
    fn ssat16() -> Self {
        Self::ZERO
            .with_fixed_low(u4::new(0b0011))
            .with_fixed_f(u4::new(0xF))
            .with_op(0x6A) // 0110 1010
    }

    fn usat16() -> Self {
        Self::ZERO
            .with_fixed_low(u4::new(0b0011))
            .with_fixed_f(u4::new(0xF))
            .with_op(0x6E) // 0110 1110
    }
}

/// BFI / BFC / UBFX / SBFX bit field operations
/// cond 0111 1x0 msb Rd lsb 001 Rn  (BFI/BFC)
/// cond 0111 1x1 widthm1 Rd lsb 101 Rn  (UBFX/SBFX)
#[bitfield(u32)]
struct BitField {
    #[bits(0..=3, rw)]
    rn: u4,
    #[bits(4..=6, rw)]
    op2: u3,
    #[bits(7..=11, rw)]
    lsb: u5,
    #[bits(12..=15, rw)]
    rd: u4,
    #[bits(16..=20, rw)]
    msb_or_width: u5,
    #[bits(21..=27, rw)]
    op1: u7,
    #[bits(28..=31, rw)]
    cond: Condition,
}

/// SSAT / USAT
/// cond 0110 1x1 sat_imm Rd shift_imm sh 01 Rn
#[bitfield(u32)]
struct SatShift {
    #[bits(0..=3, rw)]
    rn: u4,
    // bits 4-5: 01
    #[bit(4, rw)]
    fixed4: bool,
    #[bit(5, rw)]
    fixed5: bool,
    #[bit(6, rw)]
    sh: bool,
    #[bits(7..=11, rw)]
    shift_imm: u5,
    #[bits(12..=15, rw)]
    rd: u4,
    #[bits(16..=20, rw)]
    sat_imm: u5,
    #[bits(21..=27, rw)]
    op: u7,
    #[bits(28..=31, rw)]
    cond: Condition,
}

impl SatShift {
    fn ssat() -> Self {
        Self::ZERO.with_fixed4(true).with_op(u7::new(0x35)) // 0110 101 -> bits 27:21
    }
    fn usat() -> Self {
        Self::ZERO.with_fixed4(true).with_op(u7::new(0x37)) // 0110 111 -> bits 27:21
    }
}

/// PKHBT / PKHTB
/// cond 0110 1000 Rn Rd shift_imm tb 01 Rm
#[bitfield(u32)]
struct Pkh {
    #[bits(0..=3, rw)]
    rm: u4,
    // bits 4-5: 01
    #[bit(4, rw)]
    fixed4: bool,
    #[bit(5, rw)]
    fixed5: bool,
    #[bit(6, rw)]
    tb: bool,
    #[bits(7..=11, rw)]
    shift_imm: u5,
    #[bits(12..=15, rw)]
    rd: u4,
    #[bits(16..=19, rw)]
    rn: u4,
    #[bits(20..=27, rw)]
    op: u8,
    #[bits(28..=31, rw)]
    cond: Condition,
}

impl Pkh {
    fn new() -> Self {
        Self::ZERO.with_fixed4(true).with_op(0x68) // 0110 1000
    }
}

// ---------------------------------------------------------------------------
// Encoding entry point
// ---------------------------------------------------------------------------

pub fn encode_a32(
    inst: &Instruction,
    offset: u32,
    symbols: &HashMap<String, (usize, u32)>,
    equs: &HashMap<String, i64>,
    local_labels: &HashMap<u32, Vec<(usize, u32)>>,
    section: usize,
) -> Result<EncodedInst, AsmError> {
    use Mnemonic::*;
    match inst.mnemonic {
        Mov | Mvn | Add | Adc | Sub | Sbc | Rsb | Rsc | And | Orr | Eor | Bic | Cmp | Cmn | Tst
        | Teq => encode_data_proc(inst),
        Movw | Movt => encode_movw_movt_a32(inst),
        Ldr | Ldrb => encode_ldr_a32(inst, offset, symbols, equs, local_labels, section),
        Str | Strb => encode_str_a32(inst),
        Ldrh | Ldrsh | Ldrsb => encode_ldr_half_a32(inst, offset, symbols, equs, local_labels, section),
        Strh => encode_strh_a32(inst),
        Ldrd => encode_ldrd_a32(inst),
        Strd => encode_strd_a32(inst),
        Ldm | Ldmia | Ldmfd | Ldmib | Ldmed | Ldmda | Ldmfa | Ldmdb | Ldmea => encode_ldm_a32(inst),
        Stm | Stmia | Stmea | Stmib | Stmfa | Stmda | Stmed | Stmdb | Stmfd => encode_stm_a32(inst),
        B | Bl => encode_branch_a32(inst, offset, symbols, equs, local_labels, section),
        Bx => encode_bx_a32(inst),
        Blx => encode_blx_a32(inst, offset, symbols, equs, local_labels, section),
        Push => encode_push_a32(inst),
        Pop => encode_pop_a32(inst),
        Mul => encode_mul_a32(inst),
        Mla => encode_mla_a32(inst),
        Mls => encode_mls_a32(inst),
        Smull | Umull | Smlal | Umlal => encode_long_mul_a32(inst),
        Umaal => encode_umaal_a32(inst),
        Sdiv | Udiv => encode_div_a32(inst),
        Clz => encode_clz_a32(inst),
        Rbit => encode_rbit_a32(inst),
        Bfi => encode_bfi_a32(inst),
        Bfc => encode_bfc_a32(inst),
        Ubfx | Sbfx => encode_bfx_a32(inst),
        Sxth | Sxtb | Uxth | Uxtb | Sxtb16 | Uxtb16 => encode_extend_a32(inst),
        Sxtah | Sxtab | Uxtah | Uxtab | Sxtab16 | Uxtab16 => encode_extend_add_a32(inst),
        Adr => encode_adr_a32(inst, offset, symbols, equs, local_labels, section),
        Rev | Rev16 | Revsh => encode_rev_a32(inst),
        Ldrex => encode_ldrex_a32(inst),
        Strex => encode_strex_a32(inst),
        Ldrexb | Ldrexh | Ldrexd => encode_ldrex_bhd_a32(inst),
        Strexb | Strexh | Strexd => encode_strex_bhd_a32(inst),
        Clrex => Ok(emit32(0xF57FF01F)),
        Mrs => encode_mrs_a32(inst),
        Msr => encode_msr_a32(inst),
        Ssat => encode_ssat_a32(inst),
        Usat => encode_usat_a32(inst),
        Ssat16 => encode_ssat16_a32(inst),
        Usat16 => encode_usat16_a32(inst),
        Qadd | Qdadd | Qsub | Qdsub => encode_sat_arith_a32(inst),
        Pkhbt | Pkhtb => encode_pkh_a32(inst),
        Sel => encode_sel_a32(inst),
        Smulbb | Smulbt | Smultb | Smultt => encode_smulxy_a32(inst),
        Smulwb | Smulwt => encode_smulwy_a32(inst),
        Smlabb | Smlabt | Smlatb | Smlatt => encode_smlaxy_a32(inst),
        Smlawb | Smlawt => encode_smlawy_a32(inst),
        Smlalbb | Smlalbt | Smlaltb | Smlaltt => encode_smlalxy_a32(inst),
        Smmul | Smmulr | Smmla | Smmlar | Smmls | Smmlsr => encode_smmul_a32(inst),
        Smuad | Smuadx | Smusd | Smusdx | Smlad | Smladx | Smlsd | Smlsdx => {
            encode_smuad_a32(inst)
        }
        Smlald | Smlaldx | Smlsld | Smlsldx => encode_smlald_a32(inst),
        Usad8 | Usada8 => encode_usad8_a32(inst),
        Sadd16 | Sadd8 | Ssub16 | Ssub8 | Uadd16 | Uadd8 | Usub16 | Usub8 | Qadd16 | Qadd8
        | Qsub16 | Qsub8 | Shadd16 | Shadd8 | Shsub16 | Shsub8 | Uhadd16 | Uhadd8 | Uhsub16
        | Uhsub8 | Uqadd16 | Uqadd8 | Uqsub16 | Uqsub8 | Sasx | Ssax | Uasx | Usax | Qasx
        | Qsax | Shasx | Shsax | Uhasx | Uhsax | Uqasx | Uqsax => encode_parallel_a32(inst),
        Nop => {
            // NOP hint: cond 0011 0010 0000 1111 0000 0000 0000 0000
            // opcode=1001 (TEQ), S=0, Rn=0, Rd=15
            let enc = DpImm::new()
                .with_cond(cond_val(inst))
                .with_opcode(dp_opcode(Mnemonic::Teq))
                .with_rn(u4::new(0))
                .with_rd(u4::new(0xF));
            Ok(emit32(enc.raw_value()))
        }
        Svc => encode_svc_a32(inst),
        Lsl | Lsr | Asr | Ror | Rrx => encode_shift_a32(inst),
        Bkpt => encode_bkpt_a32(inst),
        Neg => {
            // NEG Rd, Rm is RSB Rd, Rm, #0
            let line = inst.line;
            match inst.operands.as_slice() {
                [Operand::Reg(rd), Operand::Reg(rm)] => {
                    let enc = DpImm::new()
                        .with_cond(cond_val(inst))
                        .with_opcode(dp_opcode(Mnemonic::Rsb))
                        .with_s(inst.set_flags)
                        .with_rn(*rm)
                        .with_rd(*rd);
                    Ok(emit32(enc.raw_value()))
                }
                _ => Err(AsmError::new(line, "NEG: need Rd, Rm")),
            }
        }
        Ldrt => encode_ldr_unpriv_a32(inst, true, false),
        Strt => encode_ldr_unpriv_a32(inst, false, false),
        Ldrbt => encode_ldr_unpriv_a32(inst, true, true),
        Strbt => encode_ldr_unpriv_a32(inst, false, true),
        Ldrht => encode_ldrh_unpriv_a32(inst, false, true),
        Strht => encode_ldrh_unpriv_a32(inst, false, true),
        Ldrsbt => encode_ldrh_unpriv_a32(inst, true, false),
        Ldrsht => encode_ldrh_unpriv_a32(inst, true, true),
        Pld => encode_pld_a32(inst),
        Pldw => encode_pldw_a32(inst),
        Pli => encode_pli_a32(inst),
        Swp | Swpb => encode_swp_a32(inst),
        Cpsie | Cpsid => encode_cps_a32(inst),
        Setend => encode_setend_a32(inst),
        Dbg => encode_dbg_a32(inst),
        Dmb => encode_barrier_a32(inst, 0xF57FF050),
        Dsb => encode_barrier_a32(inst, 0xF57FF040),
        Isb => encode_barrier_a32(inst, 0xF57FF060),
        Wfi => Ok(emit32(0x0320_F003 | (cond_bits(inst) << 28))),
        Wfe => Ok(emit32(0x0320_F002 | (cond_bits(inst) << 28))),
        Sev => Ok(emit32(0x0320_F004 | (cond_bits(inst) << 28))),
        Yield => Ok(emit32(0x0320_F001 | (cond_bits(inst) << 28))),
        _ => Err(AsmError::new(
            inst.line,
            format!("{:?} not yet supported in A32 mode", inst.mnemonic),
        )),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn emit32(val: u32) -> EncodedInst {
    EncodedInst::W32(val)
}

fn cond_val(inst: &Instruction) -> Condition {
    inst.condition.unwrap_or(Condition::Al)
}

fn cond_bits(inst: &Instruction) -> u32 {
    cond_val(inst).raw_value().value() as u32
}

fn dp_opcode(m: Mnemonic) -> u4 {
    u4::new(match m {
        Mnemonic::And => 0b0000,
        Mnemonic::Eor => 0b0001,
        Mnemonic::Sub => 0b0010,
        Mnemonic::Rsb => 0b0011,
        Mnemonic::Add => 0b0100,
        Mnemonic::Adc => 0b0101,
        Mnemonic::Sbc => 0b0110,
        Mnemonic::Rsc => 0b0111,
        Mnemonic::Tst => 0b1000,
        Mnemonic::Teq => 0b1001,
        Mnemonic::Cmp => 0b1010,
        Mnemonic::Cmn => 0b1011,
        Mnemonic::Orr => 0b1100,
        Mnemonic::Mov => 0b1101,
        Mnemonic::Bic => 0b1110,
        Mnemonic::Mvn => 0b1111,
        _ => panic!("not a data processing mnemonic"),
    })
}

/// Encode a 32-bit immediate as (imm8, rotate) or return None if not representable.
fn encode_imm12(value: u32) -> Option<(u8, u8)> {
    for rot in 0..16u8 {
        let shift = (rot as u32) * 2;
        let candidate = value.rotate_left(shift);
        if candidate <= 255 {
            return Some((candidate as u8, rot));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Data processing
// ---------------------------------------------------------------------------

fn encode_data_proc(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    let opcode = dp_opcode(inst.mnemonic);
    let s = inst.set_flags || inst.mnemonic.implicit_s();

    // Determine Rd, Rn, Operand2 based on mnemonic type
    let is_test = inst.mnemonic.implicit_s(); // CMP/CMN/TST/TEQ: no Rd
    let is_move = !inst.mnemonic.uses_rn(); // MOV/MVN: no Rn

    match inst.operands.as_slice() {
        // MOV/MVN Rd, #imm
        [Operand::Reg(rd), Operand::Imm(imm)] if is_move => {
            let (imm8, rot) = encode_imm12(*imm as u32)
                .ok_or_else(|| AsmError::new(line, format!("immediate {imm} not representable")))?;
            let enc = DpImm::new()
                .with_cond(cond_val(inst))
                .with_opcode(opcode)
                .with_s(s)
                .with_rn(u4::new(0))
                .with_rd(*rd)
                .with_rotate(u4::new(rot))
                .with_imm8(imm8);
            Ok(emit32(enc.raw_value()))
        }
        // MOV/MVN Rd, Rm [, shift]
        [Operand::Reg(rd), Operand::Reg(rm)] if is_move => {
            let enc = DpReg::new()
                .with_cond(cond_val(inst))
                .with_opcode(opcode)
                .with_s(s)
                .with_rn(u4::new(0))
                .with_rd(*rd)
                .with_rm(*rm);
            Ok(emit32(enc.raw_value()))
        }
        [Operand::Reg(rd), Operand::Shifted(rm, st, amount)] if is_move => match amount.as_ref() {
            Operand::Imm(n) => {
                let enc = DpReg::new()
                    .with_cond(cond_val(inst))
                    .with_opcode(opcode)
                    .with_s(s)
                    .with_rn(u4::new(0))
                    .with_rd(*rd)
                    .with_rm(*rm)
                    .with_shift_type(u2::new(st.encoding() as u8))
                    .with_shift_imm(u5::new((*n as u8) & 0x1F));
                Ok(emit32(enc.raw_value()))
            }
            Operand::Reg(rs) => {
                let enc = DpRegShift::new()
                    .with_cond(cond_val(inst))
                    .with_opcode(opcode)
                    .with_s(s)
                    .with_rn(u4::new(0))
                    .with_rd(*rd)
                    .with_rs(*rs)
                    .with_shift_type(u2::new(st.encoding() as u8))
                    .with_rm(*rm);
                Ok(emit32(enc.raw_value()))
            }
            _ => Err(AsmError::new(
                line,
                "expected immediate or register shift amount",
            )),
        },
        // CMP/CMN/TST/TEQ Rn, #imm
        [Operand::Reg(rn), Operand::Imm(imm)] if is_test => {
            let (imm8, rot) = encode_imm12(*imm as u32)
                .ok_or_else(|| AsmError::new(line, format!("immediate {imm} not representable")))?;
            let enc = DpImm::new()
                .with_cond(cond_val(inst))
                .with_opcode(opcode)
                .with_s(true)
                .with_rn(*rn)
                .with_rd(u4::new(0))
                .with_rotate(u4::new(rot))
                .with_imm8(imm8);
            Ok(emit32(enc.raw_value()))
        }
        // CMP/CMN/TST/TEQ Rn, Rm
        [Operand::Reg(rn), Operand::Reg(rm)] if is_test => {
            let enc = DpReg::new()
                .with_cond(cond_val(inst))
                .with_opcode(opcode)
                .with_s(true)
                .with_rn(*rn)
                .with_rd(u4::new(0))
                .with_rm(*rm);
            Ok(emit32(enc.raw_value()))
        }
        // CMP/CMN/TST/TEQ Rn, Rm, shift
        [Operand::Reg(rn), Operand::Shifted(rm, st, amount)] if is_test => match amount.as_ref() {
            Operand::Imm(n) => {
                let enc = DpReg::new()
                    .with_cond(cond_val(inst))
                    .with_opcode(opcode)
                    .with_s(true)
                    .with_rn(*rn)
                    .with_rd(u4::new(0))
                    .with_rm(*rm)
                    .with_shift_type(u2::new(st.encoding() as u8))
                    .with_shift_imm(u5::new((*n as u8) & 0x1F));
                Ok(emit32(enc.raw_value()))
            }
            Operand::Reg(rs) => {
                let enc = DpRegShift::new()
                    .with_cond(cond_val(inst))
                    .with_opcode(opcode)
                    .with_s(true)
                    .with_rn(*rn)
                    .with_rs(*rs)
                    .with_shift_type(u2::new(st.encoding() as u8))
                    .with_rm(*rm);
                Ok(emit32(enc.raw_value()))
            }
            _ => Err(AsmError::new(line, "expected imm or reg shift")),
        },
        // Normal: ADD/SUB/etc Rd, Rn, #imm
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Imm(imm)] => {
            let (imm8, rot) = encode_imm12(*imm as u32)
                .ok_or_else(|| AsmError::new(line, format!("immediate {imm} not representable")))?;
            let enc = DpImm::new()
                .with_cond(cond_val(inst))
                .with_opcode(opcode)
                .with_s(s)
                .with_rn(*rn)
                .with_rd(*rd)
                .with_rotate(u4::new(rot))
                .with_imm8(imm8);
            Ok(emit32(enc.raw_value()))
        }
        // Normal: ADD/SUB/etc Rd, Rn, Rm
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm)] => {
            let enc = DpReg::new()
                .with_cond(cond_val(inst))
                .with_opcode(opcode)
                .with_s(s)
                .with_rn(*rn)
                .with_rd(*rd)
                .with_rm(*rm);
            Ok(emit32(enc.raw_value()))
        }
        // Normal: ADD/SUB/etc Rd, Rn, Rm, shift #amount
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Shifted(rm, st, amount)] => {
            match amount.as_ref() {
                Operand::Imm(n) => {
                    let enc = DpReg::new()
                        .with_cond(cond_val(inst))
                        .with_opcode(opcode)
                        .with_s(s)
                        .with_rn(*rn)
                        .with_rd(*rd)
                        .with_rm(*rm)
                        .with_shift_type(u2::new(st.encoding() as u8))
                        .with_shift_imm(u5::new((*n as u8) & 0x1F));
                    Ok(emit32(enc.raw_value()))
                }
                Operand::Reg(rs) => {
                    let enc = DpRegShift::new()
                        .with_cond(cond_val(inst))
                        .with_opcode(opcode)
                        .with_s(s)
                        .with_rn(*rn)
                        .with_rd(*rd)
                        .with_rs(*rs)
                        .with_shift_type(u2::new(st.encoding() as u8))
                        .with_rm(*rm);
                    Ok(emit32(enc.raw_value()))
                }
                _ => Err(AsmError::new(
                    line,
                    "expected immediate or register shift amount",
                )),
            }
        }
        // Two-operand form: ADD Rd, #imm (Rd is both dest and source)
        [Operand::Reg(rd), Operand::Imm(imm)] if !is_test && !is_move => {
            let (imm8, rot) = encode_imm12(*imm as u32)
                .ok_or_else(|| AsmError::new(line, format!("immediate {imm} not representable")))?;
            let enc = DpImm::new()
                .with_cond(cond_val(inst))
                .with_opcode(opcode)
                .with_s(s)
                .with_rn(*rd)
                .with_rd(*rd)
                .with_rotate(u4::new(rot))
                .with_imm8(imm8);
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(
            line,
            format!("invalid operands for {:?}", inst.mnemonic),
        )),
    }
}

// ---------------------------------------------------------------------------
// Load/Store
// ---------------------------------------------------------------------------

fn encode_ldr_a32(
    inst: &Instruction,
    offset: u32,
    symbols: &HashMap<String, (usize, u32)>,
    equs: &HashMap<String, i64>,
    local_labels: &HashMap<u32, Vec<(usize, u32)>>,
    section: usize,
) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    let byte = inst.mnemonic == Mnemonic::Ldrb;

    match inst.operands.as_slice() {
        [Operand::Reg(rt), Operand::Memory {
            base,
            offset: MemOffset::Imm(imm),
            pre_index,
            writeback,
        }] => {
            let (add, abs_imm) = if *imm >= 0 {
                (true, *imm as u32)
            } else {
                (false, (-*imm) as u32)
            };
            if abs_imm > 4095 {
                return Err(AsmError::new(line, "LDR: offset out of range (max 4095)"));
            }
            // bits 26-27: 01 = 0x0400_0000
            let enc = LdrStrImm::new()
                .with_cond(cond_val(inst))
                .with_load(true)
                .with_byte(byte)
                .with_add(add)
                .with_pre(*pre_index)
                .with_writeback(*pre_index && *writeback) // W=1 only for pre-index writeback
                .with_rn(*base)
                .with_rt(*rt)
                .with_offset12(u12::new(abs_imm as u16));
            Ok(emit32(enc.raw_value()))
        }
        // LDR Rt, [Rn, Rm] / [Rn, -Rm] / [Rn], Rm
        [Operand::Reg(rt), Operand::Memory {
            base,
            offset: MemOffset::Reg(rm, sub),
            pre_index,
            writeback,
        }] => {
            let enc = LdrStrReg::new()
                .with_cond(cond_val(inst))
                .with_pre(*pre_index)
                .with_add(!*sub)
                .with_byte(byte)
                .with_writeback(*pre_index && *writeback)
                .with_load(true)
                .with_rn(*base)
                .with_rt(*rt)
                .with_rm(*rm);
            Ok(emit32(enc.raw_value()))
        }
        // LDR Rt, [Rn, Rm, shift #amt] / [Rn, -Rm, shift #amt]
        [Operand::Reg(rt), Operand::Memory {
            base,
            offset: MemOffset::RegShift(rm, st, amt, sub),
            pre_index,
            writeback,
        }] => {
            let enc = LdrStrReg::new()
                .with_cond(cond_val(inst))
                .with_pre(*pre_index)
                .with_add(!*sub)
                .with_byte(byte)
                .with_writeback(*pre_index && *writeback)
                .with_load(true)
                .with_rn(*base)
                .with_rt(*rt)
                .with_shift_imm(u5::masked_new(*amt))
                .with_shift_type(u2::new(st.encoding() as u8))
                .with_rm(*rm);
            Ok(emit32(enc.raw_value()))
        }
        // LDR Rt, label (PC-relative)
        [Operand::Reg(rt), Operand::Expr(expr)] => {
            let target = resolve_expr_u32(expr, symbols, equs, local_labels, section, offset, line)?;
            let pc = offset + 8; // ARM PC = current + 8
            let disp = target as i32 - pc as i32;
            let (add, abs_disp) = if disp > 0 {
                (true, disp as u32)
            } else {
                (false, (-disp) as u32)
            };
            if abs_disp > 4095 {
                return Err(AsmError::new(line, "LDR PC-relative: offset out of range"));
            }
            let enc = LdrStrImm::new()
                .with_cond(cond_val(inst))
                .with_load(true)
                .with_byte(byte)
                .with_add(add)
                .with_pre(true)
                .with_rn(u4::new(15)) // PC
                .with_rt(*rt)
                .with_offset12(u12::new(abs_disp as u16));
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "invalid operands for LDR")),
    }
}

fn encode_str_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    let byte = inst.mnemonic == Mnemonic::Strb;

    match inst.operands.as_slice() {
        [Operand::Reg(rt), Operand::Memory {
            base,
            offset: MemOffset::Imm(imm),
            pre_index,
            writeback,
        }] => {
            let (add, abs_imm) = if *imm >= 0 {
                (true, *imm as u32)
            } else {
                (false, (-*imm) as u32)
            };
            if abs_imm > 4095 {
                return Err(AsmError::new(line, "STR: offset out of range (max 4095)"));
            }
            let enc = LdrStrImm::new()
                .with_cond(cond_val(inst))
                .with_load(false)
                .with_byte(byte)
                .with_add(add)
                .with_pre(*pre_index)
                .with_writeback(*pre_index && *writeback)
                .with_rn(*base)
                .with_rt(*rt)
                .with_offset12(u12::new(abs_imm as u16));
            Ok(emit32(enc.raw_value()))
        }
        // STR Rt, [Rn, Rm] / [Rn, -Rm] / [Rn], Rm
        [Operand::Reg(rt), Operand::Memory {
            base,
            offset: MemOffset::Reg(rm, sub),
            pre_index,
            writeback,
        }] => {
            let enc = LdrStrReg::new()
                .with_cond(cond_val(inst))
                .with_pre(*pre_index)
                .with_add(!*sub)
                .with_byte(byte)
                .with_writeback(*pre_index && *writeback)
                .with_load(false)
                .with_rn(*base)
                .with_rt(*rt)
                .with_rm(*rm);
            Ok(emit32(enc.raw_value()))
        }
        // STR Rt, [Rn, Rm, shift #amt] / [Rn, -Rm, shift #amt]
        [Operand::Reg(rt), Operand::Memory {
            base,
            offset: MemOffset::RegShift(rm, st, amt, sub),
            pre_index,
            writeback,
        }] => {
            let enc = LdrStrReg::new()
                .with_cond(cond_val(inst))
                .with_pre(*pre_index)
                .with_add(!*sub)
                .with_byte(byte)
                .with_writeback(*pre_index && *writeback)
                .with_load(false)
                .with_rn(*base)
                .with_rt(*rt)
                .with_shift_imm(u5::masked_new(*amt))
                .with_shift_type(u2::new(st.encoding() as u8))
                .with_rm(*rm);
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "invalid operands for STR")),
    }
}

// ---------------------------------------------------------------------------
// Branch
// ---------------------------------------------------------------------------

fn encode_branch_a32(
    inst: &Instruction,
    offset: u32,
    symbols: &HashMap<String, (usize, u32)>,
    equs: &HashMap<String, i64>,
    local_labels: &HashMap<u32, Vec<(usize, u32)>>,
    section: usize,
) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    let expr = match inst.operands.as_slice() {
        [Operand::Expr(expr)] => expr,
        _ => return Err(AsmError::new(line, "B/BL requires a label")),
    };

    let target = resolve_expr_u32(expr, symbols, equs, local_labels, section, offset, line)?;
    let pc = offset + 8; // ARM PC = current + 8
    let disp = target as i32 - pc as i32;

    if disp % 4 != 0 {
        return Err(AsmError::new(line, "branch target not word-aligned"));
    }

    let imm = disp >> 2;
    if imm < -(1 << 23) || imm >= (1 << 23) {
        return Err(AsmError::new(line, "branch target out of range"));
    }

    let link = inst.mnemonic == Mnemonic::Bl;
    let enc = Branch::new()
        .with_cond(cond_val(inst))
        .with_link(link)
        .with_offset(u24::new(imm as u32 & 0x00FF_FFFF));
    Ok(emit32(enc.raw_value()))
}

// ---------------------------------------------------------------------------
// BX
// ---------------------------------------------------------------------------

fn encode_bx_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rm)] => {
            let c = cond_bits(inst);
            let enc: u32 = (c << 28) | 0x012F_FF10 | (rm.value() as u32);
            Ok(emit32(enc))
        }
        _ => Err(AsmError::new(line, "BX requires one register")),
    }
}

// ---------------------------------------------------------------------------
// PUSH/POP (STM/LDM aliases)
// ---------------------------------------------------------------------------

fn encode_push_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::RegList(mask)] => {
            if mask.count_ones() == 1 {
                // Single register: PUSH {Rt} = STR Rt, [SP, #-4]!
                let rt = mask.trailing_zeros() as u8;
                let enc = LdrStrImm::new()
                    .with_cond(cond_val(inst))
                    .with_pre(true)
                    .with_writeback(true)
                    .with_rn(u4::new(13))
                    .with_rt(u4::new(rt))
                    .with_offset12(u12::new(4));
                Ok(emit32(enc.raw_value()))
            } else {
                // Multiple registers: PUSH = STMDB SP!, {reglist}
                let enc = LdmStm::new()
                    .with_cond(cond_val(inst))
                    .with_pre(true)
                    .with_writeback(true)
                    .with_rn(u4::new(13))
                    .with_reglist(*mask as u16);
                Ok(emit32(enc.raw_value()))
            }
        }
        _ => Err(AsmError::new(line, "PUSH requires register list")),
    }
}

fn encode_pop_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::RegList(mask)] => {
            if mask.count_ones() == 1 {
                // Single register: POP {Rt} = LDR Rt, [SP], #4
                let rt = mask.trailing_zeros() as u8;
                let enc = LdrStrImm::new()
                    .with_cond(cond_val(inst))
                    .with_add(true)
                    .with_load(true)
                    .with_rn(u4::new(13))
                    .with_rt(u4::new(rt))
                    .with_offset12(u12::new(4));
                Ok(emit32(enc.raw_value()))
            } else {
                // Multiple registers: POP = LDMIA SP!, {reglist}
                let enc = LdmStm::new()
                    .with_cond(cond_val(inst))
                    .with_add(true)
                    .with_writeback(true)
                    .with_load(true)
                    .with_rn(u4::new(13))
                    .with_reglist(*mask as u16);
                Ok(emit32(enc.raw_value()))
            }
        }
        _ => Err(AsmError::new(line, "POP requires register list")),
    }
}

// ---------------------------------------------------------------------------
// MUL
// ---------------------------------------------------------------------------

fn encode_mul_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rm), Operand::Reg(rs)] => {
            // MUL: op=0000000, rdhi=Rd, rdlo=0, rm=Rs, rn=Rm
            let enc = MulLong::new()
                .with_cond(cond_val(inst))
                .with_s(inst.set_flags)
                .with_rdhi(*rd)
                .with_rm(*rs)
                .with_rn(*rm);
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "MUL requires three registers")),
    }
}

// ---------------------------------------------------------------------------
// SVC
// ---------------------------------------------------------------------------

fn encode_svc_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Imm(imm)] => {
            let c = cond_bits(inst);
            let val = *imm as u32 & 0x00FF_FFFF;
            let enc: u32 = (c << 28) | 0x0F00_0000 | val;
            Ok(emit32(enc))
        }
        _ => Err(AsmError::new(line, "SVC requires immediate")),
    }
}

// ---------------------------------------------------------------------------
// Shifts (LSL/LSR/ASR/ROR Rd, Rm, #imm / Rm, Rs)
// ---------------------------------------------------------------------------

fn encode_shift_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;

    // RRX Rd, Rm -> MOV Rd, Rm, ROR #0
    if inst.mnemonic == Mnemonic::Rrx {
        return match inst.operands.as_slice() {
            [Operand::Reg(rd), Operand::Reg(rm)] => {
                let enc = DpReg::new()
                    .with_cond(cond_val(inst))
                    .with_opcode(dp_opcode(Mnemonic::Mov))
                    .with_s(inst.set_flags)
                    .with_rn(u4::new(0))
                    .with_rd(*rd)
                    .with_rm(*rm)
                    .with_shift_type(u2::new(ShiftType::Ror.encoding() as u8))
                    .with_shift_imm(u5::new(0));
                Ok(emit32(enc.raw_value()))
            }
            _ => Err(AsmError::new(line, "RRX: need Rd, Rm")),
        };
    }

    let shift_type = match inst.mnemonic {
        Mnemonic::Lsl => ShiftType::Lsl,
        Mnemonic::Lsr => ShiftType::Lsr,
        Mnemonic::Asr => ShiftType::Asr,
        Mnemonic::Ror => ShiftType::Ror,
        _ => unreachable!(),
    };

    match inst.operands.as_slice() {
        // LSL Rd, Rm, #imm -> MOV Rd, Rm, LSL #imm
        [Operand::Reg(rd), Operand::Reg(rm), Operand::Imm(amount)] => {
            let s = inst.set_flags;
            // ARM encodes LSR #32 and ASR #32 as imm5=0 (special case)
            let imm5 = (*amount as u8) & 0x1F;
            let enc = DpReg::new()
                .with_cond(cond_val(inst))
                .with_opcode(dp_opcode(Mnemonic::Mov))
                .with_s(s)
                .with_rn(u4::new(0))
                .with_rd(*rd)
                .with_rm(*rm)
                .with_shift_type(u2::new(shift_type.encoding() as u8))
                .with_shift_imm(u5::new(imm5));
            Ok(emit32(enc.raw_value()))
        }
        // LSL Rd, Rm, Rs -> MOV Rd, Rm, LSL Rs  (register shift)
        [Operand::Reg(rd), Operand::Reg(rm), Operand::Reg(rs)] => {
            let s = inst.set_flags;
            let enc = DpRegShift::new()
                .with_cond(cond_val(inst))
                .with_opcode(dp_opcode(Mnemonic::Mov))
                .with_s(s)
                .with_rn(u4::new(0))
                .with_rd(*rd)
                .with_rs(*rs)
                .with_shift_type(u2::new(shift_type.encoding() as u8))
                .with_rm(*rm);
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "invalid operands for shift")),
    }
}

// ---------------------------------------------------------------------------
// MOVW / MOVT (16-bit immediate)
// ---------------------------------------------------------------------------

fn encode_movw_movt_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    let (rd, imm16) = match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Imm(imm)] => (*rd, *imm as u32),
        _ => return Err(AsmError::new(line, "MOVW/MOVT: need Rd, #imm16")),
    };
    if imm16 > 0xFFFF {
        return Err(AsmError::new(line, "MOVW/MOVT: immediate out of range"));
    }
    let c = cond_bits(inst);
    let top = if inst.mnemonic == Mnemonic::Movt {
        1u32 << 22
    } else {
        0
    };
    // cond 0011 0x00 imm4 Rd imm12
    let imm4 = (imm16 >> 12) & 0xF;
    let imm12 = imm16 & 0xFFF;
    let enc: u32 =
        (c << 28) | 0x0300_0000 | top | (imm4 << 16) | ((rd.value() as u32) << 12) | imm12;
    Ok(emit32(enc))
}

// ---------------------------------------------------------------------------
// Halfword / signed load/store
// ---------------------------------------------------------------------------

fn encode_ldr_half_a32(
    inst: &Instruction,
    offset: u32,
    symbols: &HashMap<String, (usize, u32)>,
    equs: &HashMap<String, i64>,
    local_labels: &HashMap<u32, Vec<(usize, u32)>>,
    section: usize,
) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    let (s_bit, h_bit) = match inst.mnemonic {
        Mnemonic::Ldrh => (false, true),
        Mnemonic::Ldrsh => (true, true),
        Mnemonic::Ldrsb => (true, false),
        _ => unreachable!(),
    };

    match inst.operands.as_slice() {
        [Operand::Reg(rt), Operand::Memory {
            base,
            offset: MemOffset::Imm(imm),
            pre_index,
            writeback,
        }] => {
            let (add, abs) = if *imm >= 0 {
                (true, *imm as u32)
            } else {
                (false, (-*imm) as u32)
            };
            if abs > 255 {
                return Err(AsmError::new(
                    line,
                    "halfword offset out of range (max 255)",
                ));
            }
            let enc = HalfWordImm::new()
                .with_cond(cond_val(inst))
                .with_pre(*pre_index)
                .with_add(add)
                .with_writeback(*pre_index && *writeback)
                .with_load(true)
                .with_rn(*base)
                .with_rt(*rt)
                .with_s(s_bit)
                .with_h(h_bit)
                .with_imm4h(u4::new((abs >> 4) as u8))
                .with_imm4l(u4::new((abs & 0xF) as u8));
            Ok(emit32(enc.raw_value()))
        }
        [Operand::Reg(rt), Operand::Memory {
            base,
            offset: MemOffset::Reg(rm, sub),
            pre_index,
            writeback,
        }] => {
            let enc = HalfWordReg::new()
                .with_cond(cond_val(inst))
                .with_pre(*pre_index)
                .with_add(!*sub)
                .with_writeback(*pre_index && *writeback)
                .with_load(true)
                .with_rn(*base)
                .with_rt(*rt)
                .with_s(s_bit)
                .with_h(h_bit)
                .with_rm(*rm);
            Ok(emit32(enc.raw_value()))
        }
        [Operand::Reg(rt), Operand::Expr(expr)] => {
            let target = resolve_expr_u32(expr, symbols, equs, local_labels, section, offset, line)?;
            let pc = offset + 8;
            let disp = target as i32 - pc as i32;
            let (add, abs) = if disp >= 0 {
                (true, disp as u32)
            } else {
                (false, (-disp) as u32)
            };
            if abs > 255 {
                return Err(AsmError::new(
                    line,
                    "halfword PC-relative offset out of range",
                ));
            }
            let enc = HalfWordImm::new()
                .with_cond(cond_val(inst))
                .with_pre(true)
                .with_add(add)
                .with_load(true)
                .with_rn(u4::new(15)) // PC
                .with_rt(*rt)
                .with_s(s_bit)
                .with_h(h_bit)
                .with_imm4h(u4::new((abs >> 4) as u8))
                .with_imm4l(u4::new((abs & 0xF) as u8));
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "invalid operands for halfword load")),
    }
}

fn encode_strh_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rt), Operand::Memory {
            base,
            offset: MemOffset::Imm(imm),
            pre_index,
            writeback,
        }] => {
            let (add, abs) = if *imm >= 0 {
                (true, *imm as u32)
            } else {
                (false, (-*imm) as u32)
            };
            if abs > 255 {
                return Err(AsmError::new(line, "STRH offset out of range"));
            }
            let enc = HalfWordImm::new()
                .with_cond(cond_val(inst))
                .with_pre(*pre_index)
                .with_add(add)
                .with_writeback(*pre_index && *writeback)
                .with_load(false)
                .with_rn(*base)
                .with_rt(*rt)
                .with_s(false)
                .with_h(true)
                .with_imm4h(u4::new((abs >> 4) as u8))
                .with_imm4l(u4::new((abs & 0xF) as u8));
            Ok(emit32(enc.raw_value()))
        }
        [Operand::Reg(rt), Operand::Memory {
            base,
            offset: MemOffset::Reg(rm, sub),
            pre_index,
            writeback,
        }] => {
            let enc = HalfWordReg::new()
                .with_cond(cond_val(inst))
                .with_pre(*pre_index)
                .with_add(!*sub)
                .with_writeback(*pre_index && *writeback)
                .with_load(false)
                .with_rn(*base)
                .with_rt(*rt)
                .with_s(false)
                .with_h(true)
                .with_rm(*rm);
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "invalid operands for STRH")),
    }
}

// ---------------------------------------------------------------------------
// LDRD / STRD
// ---------------------------------------------------------------------------

fn encode_ldrd_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rt), Operand::Reg(_rt2), Operand::Memory {
            base,
            offset: MemOffset::Imm(imm),
            pre_index,
            writeback,
        }] => {
            let (add, abs) = if *imm >= 0 {
                (true, *imm as u32)
            } else {
                (false, (-*imm) as u32)
            };
            if abs > 255 {
                return Err(AsmError::new(line, "LDRD offset out of range"));
            }
            // LDRD: S=1 H=0 (0xD0 = 1101_0000)
            let enc = HalfWordImm::new()
                .with_cond(cond_val(inst))
                .with_pre(*pre_index)
                .with_add(add)
                .with_writeback(*pre_index && *writeback)
                .with_load(false) // LDRD uses L=0 in misc encoding
                .with_rn(*base)
                .with_rt(*rt)
                .with_s(true)
                .with_h(false)
                .with_imm4h(u4::new((abs >> 4) as u8))
                .with_imm4l(u4::new((abs & 0xF) as u8));
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "LDRD: need Rt, Rt2, [Rn, #imm]")),
    }
}

fn encode_strd_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rt), Operand::Reg(_rt2), Operand::Memory {
            base,
            offset: MemOffset::Imm(imm),
            pre_index,
            writeback,
        }] => {
            let (add, abs) = if *imm >= 0 {
                (true, *imm as u32)
            } else {
                (false, (-*imm) as u32)
            };
            if abs > 255 {
                return Err(AsmError::new(line, "STRD offset out of range"));
            }
            // STRD: S=1 H=1 (0xF0 = 1111_0000)
            let enc = HalfWordImm::new()
                .with_cond(cond_val(inst))
                .with_pre(*pre_index)
                .with_add(add)
                .with_writeback(*pre_index && *writeback)
                .with_load(false)
                .with_rn(*base)
                .with_rt(*rt)
                .with_s(true)
                .with_h(true)
                .with_imm4h(u4::new((abs >> 4) as u8))
                .with_imm4l(u4::new((abs & 0xF) as u8));
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "STRD: need Rt, Rt2, [Rn, #imm]")),
    }
}

// ---------------------------------------------------------------------------
// LDM / STM
// ---------------------------------------------------------------------------

fn encode_ldm_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    let (rn, mask) = match inst.operands.as_slice() {
        [Operand::Reg(rn), Operand::RegList(mask)] => (*rn, *mask),
        _ => return Err(AsmError::new(line, "LDM: need Rn, {reglist}")),
    };
    // IA(FD): P=0 U=1, IB(ED): P=1 U=1, DA(FA): P=0 U=0, DB(EA): P=1 U=0
    let (pre, add) = match inst.mnemonic {
        Mnemonic::Ldm | Mnemonic::Ldmia | Mnemonic::Ldmfd => (false, true),
        Mnemonic::Ldmib | Mnemonic::Ldmed => (true, true),
        Mnemonic::Ldmda | Mnemonic::Ldmfa => (false, false),
        Mnemonic::Ldmdb | Mnemonic::Ldmea => (true, false),
        _ => unreachable!(),
    };
    let enc = LdmStm::new()
        .with_cond(cond_val(inst))
        .with_pre(pre)
        .with_add(add)
        .with_writeback(inst.writeback)
        .with_load(true)
        .with_rn(rn)
        .with_reglist(mask);
    Ok(emit32(enc.raw_value()))
}

fn encode_stm_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    let (rn, mask) = match inst.operands.as_slice() {
        [Operand::Reg(rn), Operand::RegList(mask)] => (*rn, *mask),
        _ => return Err(AsmError::new(line, "STM: need Rn, {reglist}")),
    };
    let (pre, add) = match inst.mnemonic {
        Mnemonic::Stm | Mnemonic::Stmia | Mnemonic::Stmea => (false, true),
        Mnemonic::Stmib | Mnemonic::Stmfa => (true, true),
        Mnemonic::Stmda | Mnemonic::Stmed => (false, false),
        Mnemonic::Stmdb | Mnemonic::Stmfd => (true, false),
        _ => unreachable!(),
    };
    let enc = LdmStm::new()
        .with_cond(cond_val(inst))
        .with_pre(pre)
        .with_add(add)
        .with_writeback(inst.writeback)
        .with_load(false)
        .with_rn(rn)
        .with_reglist(mask);
    Ok(emit32(enc.raw_value()))
}

// ---------------------------------------------------------------------------
// BLX (register)
// ---------------------------------------------------------------------------

fn encode_blx_a32(
    inst: &Instruction,
    offset: u32,
    symbols: &HashMap<String, (usize, u32)>,
    equs: &HashMap<String, i64>,
    local_labels: &HashMap<u32, Vec<(usize, u32)>>,
    section: usize,
) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rm)] => {
            let c = cond_bits(inst);
            let enc: u32 = (c << 28) | 0x012F_FF30 | (rm.value() as u32);
            Ok(emit32(enc))
        }
        // BLX label (unconditional, switches to Thumb)
        // 1111 101 H imm24
        [Operand::Expr(expr)] => {
            let target = resolve_expr_u32(expr, symbols, equs, local_labels, section, offset, line)?;
            let pc = offset + 8;
            let disp = target as i32 - pc as i32;
            // H bit is bit 1 of the displacement (half-word alignment for Thumb target)
            let h = ((disp >> 1) & 1) as u32;
            let imm24 = ((disp >> 2) & 0x00FF_FFFF) as u32;
            let enc: u32 = 0xFA00_0000 | (h << 24) | imm24;
            Ok(emit32(enc))
        }
        _ => Err(AsmError::new(line, "BLX requires register or label")),
    }
}

// ---------------------------------------------------------------------------
// MLA, MLS
// ---------------------------------------------------------------------------

fn encode_mla_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm), Operand::Reg(ra)] => {
            // MLA: op=0000001
            let enc = MulLong::new()
                .with_cond(cond_val(inst))
                .with_op(u7::new(0b0000001))
                .with_s(inst.set_flags)
                .with_rdhi(*rd)
                .with_rdlo(*ra)
                .with_rm(*rm)
                .with_rn(*rn);
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "MLA: need Rd, Rn, Rm, Ra")),
    }
}

fn encode_mls_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm), Operand::Reg(ra)] => {
            // MLS: op=0000011
            let enc = MulLong::new()
                .with_cond(cond_val(inst))
                .with_op(u7::new(0b0000011))
                .with_rdhi(*rd)
                .with_rdlo(*ra)
                .with_rm(*rm)
                .with_rn(*rn);
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "MLS: need Rd, Rn, Rm, Ra")),
    }
}

// ---------------------------------------------------------------------------
// Long multiply: SMULL, UMULL, SMLAL, UMLAL
// ---------------------------------------------------------------------------

fn encode_long_mul_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rdlo), Operand::Reg(rdhi), Operand::Reg(rn), Operand::Reg(rm)] => {
            let op = match inst.mnemonic {
                Mnemonic::Umull => 0b0000100u8,
                Mnemonic::Umlal => 0b0000101,
                Mnemonic::Smull => 0b0000110,
                Mnemonic::Smlal => 0b0000111,
                _ => unreachable!(),
            };
            let enc = MulLong::new()
                .with_cond(cond_val(inst))
                .with_op(u7::new(op))
                .with_s(inst.set_flags)
                .with_rdhi(*rdhi)
                .with_rdlo(*rdlo)
                .with_rm(*rm)
                .with_rn(*rn);
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(
            line,
            "long multiply: need RdLo, RdHi, Rn, Rm",
        )),
    }
}

// ---------------------------------------------------------------------------
// SDIV / UDIV
// ---------------------------------------------------------------------------

fn encode_div_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm)] => {
            let c = cond_bits(inst);
            let op = if inst.mnemonic == Mnemonic::Sdiv {
                0x0710_0010u32
            } else {
                0x0730_0010
            };
            let enc: u32 = (c << 28)
                | op
                | ((rd.value() as u32) << 16)
                | (0xF << 12)
                | ((rm.value() as u32) << 8)
                | (rn.value() as u32);
            Ok(emit32(enc))
        }
        _ => Err(AsmError::new(line, "DIV: need Rd, Rn, Rm")),
    }
}

// ---------------------------------------------------------------------------
// CLZ, RBIT
// ---------------------------------------------------------------------------

fn encode_clz_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rm)] => {
            let c = cond_bits(inst);
            // cond 0001 0110 1111 Rd 1111 0001 Rm
            let enc: u32 =
                (c << 28) | 0x016F_0F10 | ((rd.value() as u32) << 12) | (rm.value() as u32);
            Ok(emit32(enc))
        }
        _ => Err(AsmError::new(line, "CLZ: need Rd, Rm")),
    }
}

fn encode_rbit_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rm)] => {
            let c = cond_bits(inst);
            // cond 0110 1111 1111 Rd 1111 0011 Rm
            let enc: u32 =
                (c << 28) | 0x06FF_0F30 | ((rd.value() as u32) << 12) | (rm.value() as u32);
            Ok(emit32(enc))
        }
        _ => Err(AsmError::new(line, "RBIT: need Rd, Rm")),
    }
}

// ---------------------------------------------------------------------------
// BFI, BFC, UBFX, SBFX
// ---------------------------------------------------------------------------

fn encode_bfi_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Imm(lsb), Operand::Imm(width)] => {
            let msb = *lsb as u8 + *width as u8 - 1;
            let enc = BitField::ZERO
                .with_cond(inst.condition.unwrap_or(Condition::Al))
                .with_op1(u7::new(0b0111_110))
                .with_msb_or_width(u5::new(msb))
                .with_rd(*rd)
                .with_lsb(u5::new(*lsb as u8))
                .with_op2(u3::new(0b001))
                .with_rn(*rn);
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "BFI: need Rd, Rn, #lsb, #width")),
    }
}

fn encode_bfc_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Imm(lsb), Operand::Imm(width)] => {
            let msb = *lsb as u8 + *width as u8 - 1;
            let enc = BitField::ZERO
                .with_cond(inst.condition.unwrap_or(Condition::Al))
                .with_op1(u7::new(0b0111_110))
                .with_msb_or_width(u5::new(msb))
                .with_rd(*rd)
                .with_lsb(u5::new(*lsb as u8))
                .with_op2(u3::new(0b001))
                .with_rn(u4::new(0xF));
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "BFC: need Rd, #lsb, #width")),
    }
}

fn encode_bfx_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Imm(lsb), Operand::Imm(width)] => {
            let widthm1 = *width as u8 - 1;
            let op1 = if inst.mnemonic == Mnemonic::Sbfx {
                0b0111_101u8
            } else {
                0b0111_111
            };
            let enc = BitField::ZERO
                .with_cond(inst.condition.unwrap_or(Condition::Al))
                .with_op1(u7::new(op1))
                .with_msb_or_width(u5::new(widthm1))
                .with_rd(*rd)
                .with_lsb(u5::new(*lsb as u8))
                .with_op2(u3::new(0b101))
                .with_rn(*rn);
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "UBFX/SBFX: need Rd, Rn, #lsb, #width")),
    }
}

// ---------------------------------------------------------------------------
// Extend: SXTH, SXTB, UXTH, UXTB
// ---------------------------------------------------------------------------

fn encode_extend_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    let (rd, rm, rot) = match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rm)] => (*rd, *rm, 0u32),
        [Operand::Reg(rd), Operand::Shifted(rm, ShiftType::Ror, amount)] => {
            let rot = match amount.as_ref() {
                Operand::Imm(n) => (*n as u32 / 8) & 3,
                _ => return Err(AsmError::new(line, "extend: rotation must be immediate")),
            };
            (*rd, *rm, rot)
        }
        _ => return Err(AsmError::new(line, "extend: need Rd, Rm{, ROR #n}")),
    };
    let c = cond_bits(inst);
    // cond 0110 1UBx 1111 Rd rot 0111 Rm
    let op = match inst.mnemonic {
        Mnemonic::Sxth => 0x06BF_0070u32,
        Mnemonic::Sxtb => 0x06AF_0070,
        Mnemonic::Uxth => 0x06FF_0070,
        Mnemonic::Uxtb => 0x06EF_0070,
        Mnemonic::Sxtb16 => 0x068F_0070,
        Mnemonic::Uxtb16 => 0x06CF_0070,
        _ => unreachable!(),
    };
    let enc: u32 = (c << 28) | op | ((rd.value() as u32) << 12) | (rot << 10) | (rm.value() as u32);
    Ok(emit32(enc))
}

// ---------------------------------------------------------------------------
// Extend-add: SXTAH, SXTAB, UXTAH, UXTAB
// ---------------------------------------------------------------------------

fn encode_extend_add_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    let (rd, rn, rm, rot) = match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm)] => (*rd, *rn, *rm, 0u32),
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Shifted(rm, ShiftType::Ror, amount)] => {
            let rot = match amount.as_ref() {
                Operand::Imm(n) => (*n as u32 / 8) & 3,
                _ => {
                    return Err(AsmError::new(
                        line,
                        "extend-add: rotation must be immediate",
                    ))
                }
            };
            (*rd, *rn, *rm, rot)
        }
        _ => return Err(AsmError::new(line, "extend-add: need Rd, Rn, Rm{, ROR #n}")),
    };
    let c = cond_bits(inst);
    let op = match inst.mnemonic {
        Mnemonic::Sxtah => 0x06B0_0070u32,
        Mnemonic::Sxtab => 0x06A0_0070,
        Mnemonic::Uxtah => 0x06F0_0070,
        Mnemonic::Uxtab => 0x06E0_0070,
        Mnemonic::Sxtab16 => 0x0680_0070,
        Mnemonic::Uxtab16 => 0x06C0_0070,
        _ => unreachable!(),
    };
    let enc: u32 = (c << 28)
        | op
        | ((rn.value() as u32) << 16)
        | ((rd.value() as u32) << 12)
        | (rot << 10)
        | (rm.value() as u32);
    Ok(emit32(enc))
}

// ---------------------------------------------------------------------------
// REV, REV16, REVSH
// ---------------------------------------------------------------------------

fn encode_rev_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rm)] => {
            let c = cond_bits(inst);
            let op = match inst.mnemonic {
                Mnemonic::Rev => 0x06BF_0F30u32,
                Mnemonic::Rev16 => 0x06BF_0FB0,
                Mnemonic::Revsh => 0x06FF_0FB0,
                _ => unreachable!(),
            };
            let enc: u32 = (c << 28) | op | ((rd.value() as u32) << 12) | (rm.value() as u32);
            Ok(emit32(enc))
        }
        _ => Err(AsmError::new(line, "REV: need Rd, Rm")),
    }
}

// ---------------------------------------------------------------------------
// LDREX / STREX family
// ---------------------------------------------------------------------------

fn encode_ldrex_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rt), Operand::Memory {
            base,
            offset: MemOffset::Imm(0),
            ..
        }] => {
            let c = cond_bits(inst);
            // cond 0001 1001 Rn Rt 1111 1001 1111
            let enc: u32 = (c << 28)
                | 0x0190_0F9F
                | ((base.value() as u32) << 16)
                | ((rt.value() as u32) << 12);
            Ok(emit32(enc))
        }
        _ => Err(AsmError::new(line, "LDREX: need Rt, [Rn]")),
    }
}

fn encode_strex_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rt), Operand::Memory {
            base,
            offset: MemOffset::Imm(0),
            ..
        }] => {
            let c = cond_bits(inst);
            // cond 0001 1000 Rn Rd 1111 1001 Rt
            let enc: u32 = (c << 28)
                | 0x0180_0F90
                | ((base.value() as u32) << 16)
                | ((rd.value() as u32) << 12)
                | (rt.value() as u32);
            Ok(emit32(enc))
        }
        _ => Err(AsmError::new(line, "STREX: need Rd, Rt, [Rn]")),
    }
}

fn encode_ldrex_bhd_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    // LDREXD takes Rt, Rt2, [Rn] but Rt2 is implicit (Rt+1) in A32
    let (rt, base) = match inst.operands.as_slice() {
        [Operand::Reg(rt), Operand::Memory {
            base,
            offset: MemOffset::Imm(0),
            ..
        }] => (*rt, *base),
        [Operand::Reg(rt), Operand::Reg(_rt2), Operand::Memory {
            base,
            offset: MemOffset::Imm(0),
            ..
        }] => (*rt, *base),
        _ => return Err(AsmError::new(line, "LDREXB/H/D: need Rt, [Rn]")),
    };
    let c = cond_bits(inst);
    let op = match inst.mnemonic {
        Mnemonic::Ldrexb => 0x01D0_0F9Fu32,
        Mnemonic::Ldrexh => 0x01F0_0F9F,
        Mnemonic::Ldrexd => 0x01B0_0F9F,
        _ => unreachable!(),
    };
    let enc: u32 =
        (c << 28) | op | ((base.value() as u32) << 16) | ((rt.value() as u32) << 12);
    Ok(emit32(enc))
}

fn encode_strex_bhd_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    // STREXD takes Rd, Rt, Rt2, [Rn] but Rt2 is implicit in encoding
    let (rd, rt, base) = match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rt), Operand::Memory {
            base,
            offset: MemOffset::Imm(0),
            ..
        }] => (*rd, *rt, *base),
        [Operand::Reg(rd), Operand::Reg(rt), Operand::Reg(_rt2), Operand::Memory {
            base,
            offset: MemOffset::Imm(0),
            ..
        }] => (*rd, *rt, *base),
        _ => return Err(AsmError::new(line, "STREXB/H/D: need Rd, Rt, [Rn]")),
    };
    let c = cond_bits(inst);
    let op = match inst.mnemonic {
        Mnemonic::Strexb => 0x01C0_0F90u32,
        Mnemonic::Strexh => 0x01E0_0F90,
        Mnemonic::Strexd => 0x01A0_0F90,
        _ => unreachable!(),
    };
    let enc: u32 = (c << 28)
        | op
        | ((base.value() as u32) << 16)
        | ((rd.value() as u32) << 12)
        | (rt.value() as u32);
    Ok(emit32(enc))
}

// ---------------------------------------------------------------------------
// MRS / MSR
// ---------------------------------------------------------------------------

fn encode_mrs_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    let (rd, sysm) = match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::SysReg(s)] => (*rd, *s),
        _ => return Err(AsmError::new(line, "MRS: need Rd, sysreg")),
    };
    let c = cond_bits(inst);
    // cond 0001 0R00 1111 Rd 0000 0000 0000
    let mut enc: u32 = (c << 28) | 0x010F_0000 | ((rd.value() as u32) << 12);
    // A-profile CPSR/SPSR: bit 7 set, bit 4 = R (SPSR)
    if sysm & 0x80 != 0 && sysm & 0x10 != 0 {
        enc |= 1 << 22; // R=1 → read SPSR
    }
    Ok(emit32(enc))
}

fn encode_msr_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    let (sysm, rn) = match inst.operands.as_slice() {
        [Operand::SysReg(s), Operand::Reg(rn)] => (*s, *rn),
        _ => return Err(AsmError::new(line, "MSR: need sysreg, Rn")),
    };
    let c = cond_bits(inst);
    // MSR CPSR_<fields>, Rn: cond 0001 0R10 mask 1111 0000 0000 Rn
    let (r_bit, mask): (u32, u32) = if sysm & 0x80 != 0 {
        // A-profile: bit 4 = R, bits 3:0 = field mask
        let r = if sysm & 0x10 != 0 { 1u32 } else { 0u32 };
        let m = (sysm & 0x0F) as u32;
        (r, m)
    } else {
        // M-profile: APSR → flags only
        (0, if sysm == 0 { 0x8 } else { 0xF })
    };
    let enc: u32 =
        (c << 28) | 0x0120_F000 | (r_bit << 22) | (mask << 16) | (rn.value() as u32);
    Ok(emit32(enc))
}

// ---------------------------------------------------------------------------
// SSAT / USAT
// ---------------------------------------------------------------------------

fn encode_ssat_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let (rd, sat, rn, sh, sh_amt) = parse_sat_ops_a32(inst)?;
    let enc = SatShift::ssat()
        .with_cond(inst.condition.unwrap_or(Condition::Al))
        .with_sat_imm(u5::new(sat - 1))
        .with_rd(rd)
        .with_shift_imm(u5::masked_new(sh_amt))
        .with_sh(sh != 0)
        .with_rn(rn);
    Ok(emit32(enc.raw_value()))
}

fn encode_usat_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let (rd, sat, rn, sh, sh_amt) = parse_sat_ops_a32(inst)?;
    let enc = SatShift::usat()
        .with_cond(inst.condition.unwrap_or(Condition::Al))
        .with_sat_imm(u5::new(sat))
        .with_rd(rd)
        .with_shift_imm(u5::masked_new(sh_amt))
        .with_sh(sh != 0)
        .with_rn(rn);
    Ok(emit32(enc.raw_value()))
}

fn parse_sat_ops_a32(inst: &Instruction) -> Result<(u4, u8, u4, u8, u8), AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Imm(sat), Operand::Reg(rn)] => Ok((*rd, *sat as u8, *rn, 0, 0)),
        [Operand::Reg(rd), Operand::Imm(sat), Operand::Shifted(rn, st, amount)] => {
            let sh = match st {
                ShiftType::Lsl => 0u8,
                ShiftType::Asr => 1,
                _ => return Err(AsmError::new(line, "SSAT/USAT: only LSL/ASR shifts")),
            };
            let amt = match amount.as_ref() {
                Operand::Imm(n) => *n as u8,
                _ => return Err(AsmError::new(line, "SSAT/USAT: expected imm shift")),
            };
            Ok((*rd, *sat as u8, *rn, sh, amt))
        }
        _ => Err(AsmError::new(line, "SSAT/USAT: need Rd, #sat, Rn")),
    }
}

// ---------------------------------------------------------------------------
// Saturating arithmetic: QADD, QDADD, QSUB, QDSUB
// ---------------------------------------------------------------------------

fn encode_sat_arith_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rm), Operand::Reg(rn)] => {
            let c = cond_bits(inst);
            let op = match inst.mnemonic {
                Mnemonic::Qadd => 0x0100_0050u32,
                Mnemonic::Qsub => 0x0120_0050,
                Mnemonic::Qdadd => 0x0140_0050,
                Mnemonic::Qdsub => 0x0160_0050,
                _ => unreachable!(),
            };
            let enc: u32 = (c << 28)
                | op
                | ((rn.value() as u32) << 16)
                | ((rd.value() as u32) << 12)
                | (rm.value() as u32);
            Ok(emit32(enc))
        }
        _ => Err(AsmError::new(line, "QADD/etc: need Rd, Rm, Rn")),
    }
}

// ---------------------------------------------------------------------------
// PKHBT / PKHTB
// ---------------------------------------------------------------------------

fn encode_pkh_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    let (rd, rn, rm, sh_amt) = match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm)] => (*rd, *rn, *rm, 0u8),
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Shifted(rm, _st, amt)] => {
            let a = match amt.as_ref() {
                Operand::Imm(n) => *n as u8,
                _ => return Err(AsmError::new(line, "PKH: need imm shift")),
            };
            (*rd, *rn, *rm, a)
        }
        _ => return Err(AsmError::new(line, "PKHBT/PKHTB: need Rd, Rn, Rm")),
    };
    let enc = Pkh::new()
        .with_cond(inst.condition.unwrap_or(Condition::Al))
        .with_rn(rn)
        .with_rd(rd)
        .with_shift_imm(u5::masked_new(sh_amt))
        .with_tb(inst.mnemonic == Mnemonic::Pkhtb)
        .with_rm(rm);
    Ok(emit32(enc.raw_value()))
}

// ---------------------------------------------------------------------------
// SEL
// ---------------------------------------------------------------------------

fn encode_sel_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm)] => {
            let c = cond_bits(inst);
            // cond 0110 1000 Rn Rd 1111 1011 Rm
            let enc: u32 = (c << 28)
                | 0x0680_0FB0
                | ((rn.value() as u32) << 16)
                | ((rd.value() as u32) << 12)
                | (rm.value() as u32);
            Ok(emit32(enc))
        }
        _ => Err(AsmError::new(line, "SEL: need Rd, Rn, Rm")),
    }
}

// ---------------------------------------------------------------------------
// DSP multiply: SMULxy, SMLAxy
// ---------------------------------------------------------------------------

fn encode_smulxy_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm)] => {
            let (x, y) = match inst.mnemonic {
                Mnemonic::Smulbb => (false, false),
                Mnemonic::Smulbt => (false, true),
                Mnemonic::Smultb => (true, false),
                Mnemonic::Smultt => (true, true),
                _ => unreachable!(),
            };
            // cond 0001 0110 Rd 0000 Rm 1xy0 Rn
            let enc = HalfMul::new()
                .with_cond(cond_val(inst))
                .with_op(0x16)
                .with_rd(*rd)
                .with_ra(u4::new(0))
                .with_rs(*rm)
                .with_x(x)
                .with_y(y)
                .with_rm(*rn);
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "SMULxy: need Rd, Rn, Rm")),
    }
}

fn encode_smlaxy_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm), Operand::Reg(ra)] => {
            let (x, y) = match inst.mnemonic {
                Mnemonic::Smlabb => (false, false),
                Mnemonic::Smlabt => (false, true),
                Mnemonic::Smlatb => (true, false),
                Mnemonic::Smlatt => (true, true),
                _ => unreachable!(),
            };
            // cond 0001 0000 Rd Ra Rm 1xy0 Rn
            let enc = HalfMul::new()
                .with_cond(cond_val(inst))
                .with_op(0x10)
                .with_rd(*rd)
                .with_ra(*ra)
                .with_rs(*rm)
                .with_x(x)
                .with_y(y)
                .with_rm(*rn);
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "SMLAxy: need Rd, Rn, Rm, Ra")),
    }
}

fn encode_smlalxy_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rdlo), Operand::Reg(rdhi), Operand::Reg(rn), Operand::Reg(rm)] => {
            let (x, y) = match inst.mnemonic {
                Mnemonic::Smlalbb => (false, false),
                Mnemonic::Smlalbt => (false, true),
                Mnemonic::Smlaltb => (true, false),
                Mnemonic::Smlaltt => (true, true),
                _ => unreachable!(),
            };
            // cond 0001 0100 RdHi RdLo Rm 1xy0 Rn
            let enc = HalfMul::new()
                .with_cond(cond_val(inst))
                .with_op(0x14)
                .with_rd(*rdhi)
                .with_ra(*rdlo)
                .with_rs(*rm)
                .with_x(x)
                .with_y(y)
                .with_rm(*rn);
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "SMLALxy: need RdLo, RdHi, Rn, Rm")),
    }
}

// ---------------------------------------------------------------------------
// SMMUL, SMMLA, SMMLS
// ---------------------------------------------------------------------------

fn encode_smmul_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    // R bit (bit 5 of op2) selects rounding variant
    let round = matches!(
        inst.mnemonic,
        Mnemonic::Smmulr | Mnemonic::Smmlar | Mnemonic::Smmlsr
    );
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm)]
            if matches!(inst.mnemonic, Mnemonic::Smmul | Mnemonic::Smmulr) =>
        {
            // cond 0111 0101 Rd 1111 Rm 00R1 Rn
            let op2 = if round { 0x3u8 } else { 0x1 };
            let enc = MediaMul::ZERO
                .with_cond(cond_val(inst))
                .with_op1(0x75)
                .with_rd(*rd)
                .with_ra(u4::new(0xF))
                .with_rm(*rm)
                .with_op2(u4::new(op2))
                .with_rn(*rn);
            Ok(emit32(enc.raw_value()))
        }
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm), Operand::Reg(ra)] => {
            let op2 = match inst.mnemonic {
                Mnemonic::Smmla => u4::new(0x1),
                Mnemonic::Smmlar => u4::new(0x3),
                Mnemonic::Smmls => u4::new(0xD),
                Mnemonic::Smmlsr => u4::new(0xF),
                _ => unreachable!(),
            };
            let enc = MediaMul::ZERO
                .with_cond(cond_val(inst))
                .with_op1(0x75)
                .with_rd(*rd)
                .with_ra(*ra)
                .with_rm(*rm)
                .with_op2(op2)
                .with_rn(*rn);
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "SMMUL/SMMLA/SMMLS: invalid operands")),
    }
}

// ---------------------------------------------------------------------------
// SMUAD, SMUSD, SMLAD, SMLSD
// ---------------------------------------------------------------------------

fn encode_smuad_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    // M bit (bit 5 of op2) selects exchange variant
    let exchange = matches!(
        inst.mnemonic,
        Mnemonic::Smuadx | Mnemonic::Smusdx | Mnemonic::Smladx | Mnemonic::Smlsdx
    );
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm)] => {
            let base_op2: u8 = match inst.mnemonic {
                Mnemonic::Smuad | Mnemonic::Smuadx => 0x1,
                Mnemonic::Smusd | Mnemonic::Smusdx => 0x5,
                _ => return Err(AsmError::new(line, "expected 3-operand DSP mul")),
            };
            let op2 = base_op2 | ((exchange as u8) << 1);
            let enc = MediaMul::ZERO
                .with_cond(cond_val(inst))
                .with_op1(0x70)
                .with_rd(*rd)
                .with_ra(u4::new(0xF))
                .with_rm(*rm)
                .with_op2(u4::new(op2))
                .with_rn(*rn);
            Ok(emit32(enc.raw_value()))
        }
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm), Operand::Reg(ra)] => {
            let base_op2: u8 = match inst.mnemonic {
                Mnemonic::Smlad | Mnemonic::Smladx => 0x1,
                Mnemonic::Smlsd | Mnemonic::Smlsdx => 0x5,
                _ => return Err(AsmError::new(line, "expected 4-operand DSP mul")),
            };
            let op2 = base_op2 | ((exchange as u8) << 1);
            let enc = MediaMul::ZERO
                .with_cond(cond_val(inst))
                .with_op1(0x70)
                .with_rd(*rd)
                .with_ra(*ra)
                .with_rm(*rm)
                .with_op2(u4::new(op2))
                .with_rn(*rn);
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "SMUAD/etc: invalid operands")),
    }
}

// ---------------------------------------------------------------------------
// SMLALD, SMLSLD
// ---------------------------------------------------------------------------

fn encode_smlald_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    let exchange = matches!(
        inst.mnemonic,
        Mnemonic::Smlaldx | Mnemonic::Smlsldx
    );
    match inst.operands.as_slice() {
        [Operand::Reg(rdlo), Operand::Reg(rdhi), Operand::Reg(rn), Operand::Reg(rm)] => {
            let base_op2: u8 = match inst.mnemonic {
                Mnemonic::Smlald | Mnemonic::Smlaldx => 0x1,
                Mnemonic::Smlsld | Mnemonic::Smlsldx => 0x5,
                _ => unreachable!(),
            };
            let op2 = base_op2 | ((exchange as u8) << 1);
            let enc = MediaMul::ZERO
                .with_cond(cond_val(inst))
                .with_op1(0x74)
                .with_rd(*rdhi)
                .with_ra(*rdlo)
                .with_rm(*rm)
                .with_op2(u4::new(op2))
                .with_rn(*rn);
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(
            line,
            "SMLALD/SMLSLD: need RdLo, RdHi, Rn, Rm",
        )),
    }
}

// ---------------------------------------------------------------------------
// USAD8 / USADA8
// ---------------------------------------------------------------------------

fn encode_usad8_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm)]
            if inst.mnemonic == Mnemonic::Usad8 =>
        {
            // cond 0111 1000 Rd 1111 Rm 0001 Rn
            let enc = MediaMul::ZERO
                .with_cond(cond_val(inst))
                .with_op1(0x78)
                .with_rd(*rd)
                .with_ra(u4::new(0xF))
                .with_rm(*rm)
                .with_op2(u4::new(0x1))
                .with_rn(*rn);
            Ok(emit32(enc.raw_value()))
        }
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm), Operand::Reg(ra)] => {
            // cond 0111 1000 Rd Ra Rm 0001 Rn
            let enc = MediaMul::ZERO
                .with_cond(cond_val(inst))
                .with_op1(0x78)
                .with_rd(*rd)
                .with_ra(*ra)
                .with_rm(*rm)
                .with_op2(u4::new(0x1))
                .with_rn(*rn);
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "USAD8/USADA8: invalid operands")),
    }
}

// ---------------------------------------------------------------------------
// Parallel arithmetic (~48 variants)
// ---------------------------------------------------------------------------

fn encode_parallel_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm)] => {
            let c = cond_bits(inst);
            let op = parallel_opcode_a32(inst.mnemonic);
            let enc: u32 = (c << 28)
                | op
                | ((rn.value() as u32) << 16)
                | ((rd.value() as u32) << 12)
                | (rm.value() as u32);
            Ok(emit32(enc))
        }
        _ => Err(AsmError::new(line, "parallel arith: need Rd, Rn, Rm")),
    }
}

fn parallel_opcode_a32(m: Mnemonic) -> u32 {
    use Mnemonic::*;
    match m {
        // Signed: 0110 0001 Rn Rd 1111 opc1 Rm
        Sadd16 => 0x0610_0F10,
        Sasx => 0x0610_0F30,
        Ssax => 0x0610_0F50,
        Ssub16 => 0x0610_0F70,
        Sadd8 => 0x0610_0F90,
        Ssub8 => 0x0610_0FF0,
        // Saturating: 0110 0010
        Qadd16 => 0x0620_0F10,
        Qasx => 0x0620_0F30,
        Qsax => 0x0620_0F50,
        Qsub16 => 0x0620_0F70,
        Qadd8 => 0x0620_0F90,
        Qsub8 => 0x0620_0FF0,
        // Signed halving: 0110 0011
        Shadd16 => 0x0630_0F10,
        Shasx => 0x0630_0F30,
        Shsax => 0x0630_0F50,
        Shsub16 => 0x0630_0F70,
        Shadd8 => 0x0630_0F90,
        Shsub8 => 0x0630_0FF0,
        // Unsigned: 0110 0101
        Uadd16 => 0x0650_0F10,
        Uasx => 0x0650_0F30,
        Usax => 0x0650_0F50,
        Usub16 => 0x0650_0F70,
        Uadd8 => 0x0650_0F90,
        Usub8 => 0x0650_0FF0,
        // Unsigned saturating: 0110 0110
        Uqadd16 => 0x0660_0F10,
        Uqasx => 0x0660_0F30,
        Uqsax => 0x0660_0F50,
        Uqsub16 => 0x0660_0F70,
        Uqadd8 => 0x0660_0F90,
        Uqsub8 => 0x0660_0FF0,
        // Unsigned halving: 0110 0111
        Uhadd16 => 0x0670_0F10,
        Uhasx => 0x0670_0F30,
        Uhsax => 0x0670_0F50,
        Uhsub16 => 0x0670_0F70,
        Uhadd8 => 0x0670_0F90,
        Uhsub8 => 0x0670_0FF0,
        _ => panic!("not a parallel arithmetic mnemonic"),
    }
}

// ---------------------------------------------------------------------------
// BKPT
// ---------------------------------------------------------------------------

fn encode_bkpt_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Imm(imm)] => {
            let val = *imm as u32;
            // 1110 0001 0010 imm12 0111 imm4
            let enc: u32 = 0xE120_0070 | ((val >> 4) << 8) | (val & 0xF);
            Ok(emit32(enc))
        }
        _ => Err(AsmError::new(line, "BKPT requires immediate")),
    }
}

// ---------------------------------------------------------------------------
// ADR (PC-relative address)
// ---------------------------------------------------------------------------

fn encode_adr_a32(
    inst: &Instruction,
    offset: u32,
    symbols: &HashMap<String, (usize, u32)>,
    equs: &HashMap<String, i64>,
    local_labels: &HashMap<u32, Vec<(usize, u32)>>,
    section: usize,
) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    let (rd, expr) = match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Expr(expr)] => (*rd, expr),
        _ => return Err(AsmError::new(line, "ADR: need Rd, label")),
    };
    let target = resolve_expr_u32(expr, symbols, equs, local_labels, section, offset, line)?;
    let pc = offset + 8; // ARM PC = current + 8
    let disp = target as i32 - pc as i32;
    let c = cond_bits(inst);
    // ADR is ADD Rd, PC, #imm or SUB Rd, PC, #imm
    let (opcode, abs_disp) = if disp >= 0 {
        (0x4u32, disp as u32) // ADD
    } else {
        (0x2u32, (-disp) as u32) // SUB
    };
    let (imm8, rot) = encode_imm12(abs_disp)
        .ok_or_else(|| AsmError::new(line, "ADR: offset not representable as immediate"))?;
    let enc: u32 = (c << 28) | 0x0200_0000 | (opcode << 21)
        | (15 << 16) // Rn = PC
        | ((rd.value() as u32) << 12)
        | ((rot as u32) << 8)
        | (imm8 as u32);
    Ok(emit32(enc))
}

// ---------------------------------------------------------------------------
// Unprivileged loads/stores: LDRT, STRT, LDRBT, STRBT
// ---------------------------------------------------------------------------

fn encode_ldr_unpriv_a32(
    inst: &Instruction,
    load: bool,
    byte: bool,
) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    // These use post-index addressing: [Rn], #offset
    // Encoding: cond 01 I 0 U B 1 L Rn Rt offset12
    // P=0, W=1 for unprivileged (W=1 + P=0 = unprivileged)
    match inst.operands.as_slice() {
        [Operand::Reg(rt), Operand::Memory {
            base,
            offset: MemOffset::Imm(imm),
            pre_index: false,
            ..
        }] => {
            let (u, abs) = if *imm >= 0 {
                (1u32, *imm as u32)
            } else {
                (0, (-*imm) as u32)
            };
            if abs > 4095 {
                return Err(AsmError::new(
                    line,
                    "unprivileged load/store: offset out of range",
                ));
            }
            let c = cond_bits(inst);
            // P=0 (bit24), W=1 (bit21), I=0 (bit25=0 for immediate)
            let enc: u32 = (c << 28) | (0x04 << 24) // bits 27:26 = 01
                | (u << 23) | ((byte as u32) << 22) | (1 << 21) // W=1
                | ((load as u32) << 20)
                | ((base.value() as u32) << 16) | ((rt.value() as u32) << 12)
                | abs;
            Ok(emit32(enc))
        }
        // [Rn] with no offset = [Rn], #0
        [Operand::Reg(rt), Operand::Memory {
            base,
            offset: MemOffset::Imm(0),
            pre_index: true,
            writeback: false,
        }] => {
            let c = cond_bits(inst);
            let enc: u32 = (c << 28) | (0x04 << 24)
                | (1 << 23) // U=1 for +0
                | ((byte as u32) << 22) | (1 << 21)
                | ((load as u32) << 20)
                | ((base.value() as u32) << 16) | ((rt.value() as u32) << 12);
            Ok(emit32(enc))
        }
        // [Rn], Rm  (post-index register)
        [Operand::Reg(rt), Operand::Memory {
            base,
            offset: MemOffset::Reg(rm, sub),
            pre_index: false,
            ..
        }] => {
            let c = cond_bits(inst);
            let u = (!*sub) as u32;
            // I=1 (bit25), P=0, W=1 for register post-index unprivileged
            let enc: u32 = (c << 28) | (0x06 << 24) // bits 27:25 = 011
                | (u << 23) | ((byte as u32) << 22) | (1 << 21)
                | ((load as u32) << 20)
                | ((base.value() as u32) << 16) | ((rt.value() as u32) << 12)
                | (rm.value() as u32);
            Ok(emit32(enc))
        }
        // [Rn], Rm, shift #amt  (post-index shifted register)
        [Operand::Reg(rt), Operand::Memory {
            base,
            offset: MemOffset::RegShift(rm, st, amt, sub),
            pre_index: false,
            ..
        }] => {
            // P=0, W=1 for unprivileged; I=1 (bit25) via class=011
            let enc = LdrStrReg::new()
                .with_cond(inst.condition.unwrap_or(Condition::Al))
                .with_add(!*sub)
                .with_byte(byte)
                .with_writeback(true)
                .with_load(load)
                .with_rn(*base)
                .with_rt(*rt)
                .with_shift_imm(u5::masked_new(*amt))
                .with_shift_type(u2::new(st.encoding() as u8))
                .with_rm(*rm);
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(
            line,
            "unprivileged load/store: need Rt, [Rn]{, #offset}",
        )),
    }
}

// ---------------------------------------------------------------------------
// Unprivileged halfword: LDRHT, STRHT, LDRSBT, LDRSHT
// ---------------------------------------------------------------------------

fn encode_ldrh_unpriv_a32(
    inst: &Instruction,
    signed: bool,
    half: bool,
) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    // cond 000 0 U 1 1 L Rn Rt imm4H 1SH1 imm4L  (P=0, W=1 for T variant)
    // Actually for A32 unprivileged halfword: P=0, W=1
    // bit layout: cond 0000 U110 Rn Rt imm4H 1SH1 imm4L  (load)
    //             cond 0000 U100 Rn Rt imm4H 1SH1 imm4L  (store)
    let load = match inst.mnemonic {
        Mnemonic::Ldrht | Mnemonic::Ldrsbt | Mnemonic::Ldrsht => true,
        Mnemonic::Strht => false,
        _ => unreachable!(),
    };
    match inst.operands.as_slice() {
        [Operand::Reg(rt), Operand::Memory {
            base,
            offset: MemOffset::Imm(imm),
            pre_index: false,
            ..
        }] => {
            let (u, abs) = if *imm >= 0 {
                (1u32, *imm as u32)
            } else {
                (0, (-*imm) as u32)
            };
            if abs > 255 {
                return Err(AsmError::new(
                    line,
                    "halfword unpriv: offset out of range (max 255)",
                ));
            }
            let c = cond_bits(inst);
            // P=0 (bit24=0), W=1 (bit21=1 for T), I=1 (bit22=1 for imm8)
            let enc: u32 = (c << 28)
                | (u << 23)
                | (1 << 22)
                | (1 << 21)
                | ((load as u32) << 20)
                | ((base.value() as u32) << 16)
                | ((rt.value() as u32) << 12)
                | ((abs >> 4) << 8)
                | (1 << 7)
                | ((signed as u32) << 6)
                | ((half as u32) << 5)
                | (1 << 4)
                | (abs & 0xF);
            Ok(emit32(enc))
        }
        [Operand::Reg(rt), Operand::Memory {
            base,
            offset: MemOffset::Imm(0),
            pre_index: true,
            writeback: false,
        }] => {
            let c = cond_bits(inst);
            let enc: u32 = (c << 28)
                | (1 << 23)
                | (1 << 22)
                | (1 << 21)
                | ((load as u32) << 20)
                | ((base.value() as u32) << 16)
                | ((rt.value() as u32) << 12)
                | (1 << 7)
                | ((signed as u32) << 6)
                | ((half as u32) << 5)
                | (1 << 4);
            Ok(emit32(enc))
        }
        _ => Err(AsmError::new(
            line,
            "halfword unpriv: need Rt, [Rn], #offset",
        )),
    }
}

// ---------------------------------------------------------------------------
// PLD (Preload Data)
// ---------------------------------------------------------------------------

fn encode_pld_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Memory {
            base,
            offset: MemOffset::Imm(imm),
            ..
        }] => {
            let (add, abs) = if *imm >= 0 {
                (true, *imm as u32)
            } else {
                (false, (-*imm) as u32)
            };
            if abs > 4095 {
                return Err(AsmError::new(line, "PLD: offset out of range"));
            }
            let enc = Preload::pld()
                .with_add(add)
                .with_rn(*base)
                .with_offset12(u12::new(abs as u16));
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "PLD: need [Rn, #offset]")),
    }
}

// ---------------------------------------------------------------------------
// PLI (Preload Instruction)
// ---------------------------------------------------------------------------

fn encode_pli_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Memory {
            base,
            offset: MemOffset::Imm(imm),
            ..
        }] => {
            let (add, abs) = if *imm >= 0 {
                (true, *imm as u32)
            } else {
                (false, (-*imm) as u32)
            };
            if abs > 4095 {
                return Err(AsmError::new(line, "PLI: offset out of range"));
            }
            let enc = Preload::pli()
                .with_add(add)
                .with_rn(*base)
                .with_offset12(u12::new(abs as u16));
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "PLI: need [Rn, #offset]")),
    }
}

// ---------------------------------------------------------------------------
// CPSIE / CPSID
// ---------------------------------------------------------------------------

fn encode_cps_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    // CPSIE/CPSID takes interrupt flags as identifiers (i, f, a, or combinations)
    // CPSIE if -> 1111 0001 0000 1000 0000 0000 1100 0000 = F1080 0C0
    // CPSID if -> 1111 0001 0000 1100 0000 0000 1100 0000 = F10C00C0
    let imod = if inst.mnemonic == Mnemonic::Cpsie {
        0b10u32
    } else {
        0b11u32
    };

    // Parse flag operand - it's parsed as an identifier like "if", "i", "f", "a", "aif"
    let flags_val = match inst.operands.as_slice() {
        [Operand::Expr(Expr::Symbol(s))] => {
            let mut f = 0u32;
            for ch in s.to_ascii_lowercase().chars() {
                match ch {
                    'a' => f |= 4,
                    'i' => f |= 2,
                    'f' => f |= 1,
                    _ => return Err(AsmError::new(line, format!("CPS: unknown flag '{ch}'"))),
                }
            }
            f
        }
        _ => return Err(AsmError::new(line, "CPSIE/CPSID: need flags (a, i, f)")),
    };

    // 1111 0001 0000 imod 0 0000 000 0 A I F 0 00000
    let enc: u32 = 0xF100_0000 | (imod << 18) | (flags_val << 6);
    Ok(emit32(enc))
}

// ---------------------------------------------------------------------------
// DBG
// ---------------------------------------------------------------------------

fn encode_barrier_a32(inst: &Instruction, base: u32) -> Result<EncodedInst, AsmError> {
    let option = match inst.operands.as_slice() {
        [] => 0xF, // default SY
        [Operand::Expr(Expr::Symbol(s))] => match s.to_ascii_uppercase().as_str() {
            "SY" => 0xF,
            "ST" => 0xE,
            "LD" => 0xD,
            "ISH" => 0xB,
            "ISHST" => 0xA,
            "ISHLD" => 0x9,
            "NSH" => 0x7,
            "NSHST" => 0x6,
            "NSHLD" => 0x5,
            "OSH" => 0x3,
            "OSHST" => 0x2,
            "OSHLD" => 0x1,
            _ => {
                return Err(AsmError::new(
                    inst.line,
                    format!("unknown barrier option: {s}"),
                ))
            }
        },
        [Operand::Imm(n)] => (*n as u32) & 0xF,
        _ => return Err(AsmError::new(inst.line, "barrier: need option")),
    };
    Ok(emit32(base | option))
}

fn encode_dbg_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Imm(opt)] => {
            let c = cond_bits(inst);
            // cond 0011 0010 0000 1111 0000 1111 opt
            let enc: u32 = (c << 28) | 0x0320_F0F0 | (*opt as u32 & 0xF);
            Ok(emit32(enc))
        }
        _ => Err(AsmError::new(line, "DBG: need #option")),
    }
}

// ---------------------------------------------------------------------------
// UMAAL
// ---------------------------------------------------------------------------

fn encode_umaal_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rdlo), Operand::Reg(rdhi), Operand::Reg(rn), Operand::Reg(rm)] => {
            // cond 0000 0100 RdHi RdLo Rm 1001 Rn
            let enc = MulLong::new()
                .with_cond(cond_val(inst))
                .with_op(u7::new(0b0000010))
                .with_rdhi(*rdhi)
                .with_rdlo(*rdlo)
                .with_rm(*rm)
                .with_rn(*rn);
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "UMAAL: need RdLo, RdHi, Rn, Rm")),
    }
}

// ---------------------------------------------------------------------------
// SMULWy / SMLAWy (signed multiply word by halfword)
// ---------------------------------------------------------------------------

fn encode_smulwy_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm)] => {
            let y = inst.mnemonic == Mnemonic::Smulwt;
            // cond 0001 0010 Rd 0000 Rm 1y10 Rn
            let enc = HalfMul::new()
                .with_cond(cond_val(inst))
                .with_op(0x12)
                .with_rd(*rd)
                .with_ra(u4::new(0))
                .with_rs(*rm)
                .with_x(true) // bit 5 = 1 for SMULWy
                .with_y(y)
                .with_rm(*rn);
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "SMULWy: need Rd, Rn, Rm")),
    }
}

fn encode_smlawy_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm), Operand::Reg(ra)] => {
            let y = inst.mnemonic == Mnemonic::Smlawt;
            // cond 0001 0010 Rd Ra Rm 1y00 Rn
            let enc = HalfMul::new()
                .with_cond(cond_val(inst))
                .with_op(0x12)
                .with_rd(*rd)
                .with_ra(*ra)
                .with_rs(*rm)
                .with_x(false) // bit 5 = 0 for SMLAWy
                .with_y(y)
                .with_rm(*rn);
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "SMLAWy: need Rd, Rn, Rm, Ra")),
    }
}

// ---------------------------------------------------------------------------
// SSAT16 / USAT16
// ---------------------------------------------------------------------------

fn encode_ssat16_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Imm(sat), Operand::Reg(rn)] => {
            let enc = Sat16::ssat16()
                .with_cond(cond_val(inst))
                .with_sat_imm(u4::new((*sat as u8) - 1))
                .with_rd(*rd)
                .with_rn(*rn);
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "SSAT16: need Rd, #sat, Rn")),
    }
}

fn encode_usat16_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Imm(sat), Operand::Reg(rn)] => {
            let enc = Sat16::usat16()
                .with_cond(cond_val(inst))
                .with_sat_imm(u4::new(*sat as u8))
                .with_rd(*rd)
                .with_rn(*rn);
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "USAT16: need Rd, #sat, Rn")),
    }
}

// ---------------------------------------------------------------------------
// PLDW (Preload Data for Write)
// ---------------------------------------------------------------------------

fn encode_pldw_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Memory {
            base,
            offset: MemOffset::Imm(imm),
            ..
        }] => {
            let (add, abs) = if *imm >= 0 {
                (true, *imm as u32)
            } else {
                (false, (-*imm) as u32)
            };
            if abs > 4095 {
                return Err(AsmError::new(line, "PLDW: offset out of range"));
            }
            let enc = Preload::pldw()
                .with_add(add)
                .with_rn(*base)
                .with_offset12(u12::new(abs as u16));
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "PLDW: need [Rn, #offset]")),
    }
}

// ---------------------------------------------------------------------------
// SWP / SWPB (deprecated but valid ARMv7)
// ---------------------------------------------------------------------------

fn encode_swp_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rt), Operand::Reg(rm), Operand::Memory {
            base,
            offset: MemOffset::Imm(0),
            ..
        }] => {
            let enc = Swp::new()
                .with_cond(cond_val(inst))
                .with_byte(inst.mnemonic == Mnemonic::Swpb)
                .with_rn(*base)
                .with_rt(*rt)
                .with_rm(*rm);
            Ok(emit32(enc.raw_value()))
        }
        _ => Err(AsmError::new(line, "SWP: need Rt, Rm, [Rn]")),
    }
}

// ---------------------------------------------------------------------------
// SETEND
// ---------------------------------------------------------------------------

fn encode_setend_a32(inst: &Instruction) -> Result<EncodedInst, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Expr(Expr::Symbol(s))] => {
            let e = match s.to_ascii_uppercase().as_str() {
                "BE" => 1u32,
                "LE" => 0,
                _ => return Err(AsmError::new(line, "SETEND: need BE or LE")),
            };
            // 1111 0001 0000 0001 0000 00E0 0000 0000
            let enc: u32 = 0xF101_0000 | (e << 9);
            Ok(emit32(enc))
        }
        _ => Err(AsmError::new(line, "SETEND: need BE or LE")),
    }
}
