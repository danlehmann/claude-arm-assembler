use std::collections::HashMap;

use arbitrary_int::prelude::*;
use bitbybit::bitfield;

use crate::ast::*;
use crate::error::AsmError;

use super::resolve_label;

// ---------------------------------------------------------------------------
// Bitfield structs for Thumb 16-bit instruction formats
// ---------------------------------------------------------------------------

/// Format 1: Shift by immediate (LSL/LSR/ASR Rd, Rm, #imm5)
/// Bits [15:11]=000op [10:6]=imm5 [5:3]=Rm [2:0]=Rd
#[bitfield(u16)]
struct ShiftImm {
    #[bits(0..=2, rw)]
    rd: u3,
    #[bits(3..=5, rw)]
    rm: u3,
    #[bits(6..=10, rw)]
    imm5: u5,
    #[bits(11..=12, rw)]
    op: u2,
    // bits 13-15: 000 (implicit in initial value of 0)
}

/// Format 2: Add/Sub register/imm3
/// Bits [15:9]=0001_1Io [8:6]=Rm_imm3 [5:3]=Rn [2:0]=Rd
#[bitfield(u16)]
struct AddSubRegImm3 {
    #[bits(0..=2, rw)]
    rd: u3,
    #[bits(3..=5, rw)]
    rn: u3,
    #[bits(6..=8, rw)]
    rm_imm3: u3,
    #[bit(9, rw)]
    op: bool,
    #[bit(10, rw)]
    imm_flag: bool,
    // bits 11-15: 00011 (prefix 0x1800)
}

/// Format 3: Mov/Cmp/Add/Sub with imm8
/// Bits [15:13]=001 [12:11]=op [10:8]=Rd [7:0]=imm8
#[bitfield(u16)]
struct DataImm8 {
    #[bits(0..=7, rw)]
    imm8: u8,
    #[bits(8..=10, rw)]
    rd: u3,
    #[bits(11..=12, rw)]
    op: u2,
    // bits 13-15: 001 (prefix 0x2000)
}

/// Format 4: ALU register-register
/// Bits [15:6]=010000_op [5:3]=Rm [2:0]=Rd
#[bitfield(u16)]
struct AluOp {
    #[bits(0..=2, rw)]
    rd: u3,
    #[bits(3..=5, rw)]
    rm: u3,
    #[bits(6..=9, rw)]
    op: u4,
    // bits 10-15: 010000 (prefix 0x4000)
}

/// Format 5: Hi register operations / BX / BLX
/// Bits [15:8]=01000110+op [7]=D [6:3]=Rm [2:0]=Rd_lo
#[bitfield(u16)]
struct HiRegOp {
    #[bits([0..=2, 7], rw)]
    rd: u4,
    #[bits(3..=6, rw)]
    rm: u4,
    #[bits(8..=9, rw)]
    op: u2,
    // bits 10-15: 010001 (prefix 0x4400)
}

/// Format 7: Load/store register offset
/// Bits [15:9]=0101_ooo [8:6]=Rm [5:3]=Rn [2:0]=Rt
#[bitfield(u16)]
struct LdrStrRegOff {
    #[bits(0..=2, rw)]
    rt: u3,
    #[bits(3..=5, rw)]
    rn: u3,
    #[bits(6..=8, rw)]
    rm: u3,
    #[bits(9..=11, rw)]
    opcode: u3,
    // bits 12-15: 0101 (prefix 0x5000)
}

/// Format 9: Load/store word/byte immediate offset
/// Bits [15:13]=011 [12]=B [11]=L [10:6]=imm5 [5:3]=Rn [2:0]=Rt
#[bitfield(u16)]
struct LdrStrImm {
    #[bits(0..=2, rw)]
    rt: u3,
    #[bits(3..=5, rw)]
    rn: u3,
    #[bits(6..=10, rw)]
    imm5: u5,
    #[bit(11, rw)]
    load: bool,
    #[bit(12, rw)]
    byte: bool,
    // bits 13-15: 011 (prefix 0x6000)
}

/// Format 10: Load/store halfword immediate offset
/// Bits [15:12]=1000 [11]=L [10:6]=imm5 [5:3]=Rn [2:0]=Rt
#[bitfield(u16)]
struct LdrStrHalfImm {
    #[bits(0..=2, rw)]
    rt: u3,
    #[bits(3..=5, rw)]
    rn: u3,
    #[bits(6..=10, rw)]
    imm5: u5,
    #[bit(11, rw)]
    load: bool,
    // bits 12-15: 1000 (prefix 0x8000)
}

/// Format 11: SP-relative load/store
/// Bits [15:12]=1001 [11]=L [10:8]=Rt [7:0]=imm8
#[bitfield(u16)]
struct SpRelLdrStr {
    #[bits(0..=7, rw)]
    imm8: u8,
    #[bits(8..=10, rw)]
    rt: u3,
    #[bit(11, rw)]
    load: bool,
    // bits 12-15: 1001 (prefix 0x9000)
}

/// Format 12: Load address from PC (ADR) or SP
/// Bits [15:12]=1010 [11]=SP [10:8]=Rd [7:0]=imm8
#[bitfield(u16)]
struct LoadAddr {
    #[bits(0..=7, rw)]
    imm8: u8,
    #[bits(8..=10, rw)]
    rd: u3,
    #[bit(11, rw)]
    sp: bool,
    // bits 12-15: 1010 (prefix 0xA000)
}

/// Format 13: Adjust SP
/// Bits [15:8]=10110000 [7]=sub [6:0]=imm7
#[bitfield(u16)]
struct AdjustSp {
    #[bits(0..=6, rw)]
    imm7: u7,
    #[bit(7, rw)]
    sub: bool,
    // bits 8-15: 10110000 (prefix 0xB000)
}

/// Format 14: Push/Pop
/// Bits [15:9]=1011x10 [8]=R [7:0]=reglist
#[bitfield(u16)]
struct PushPop {
    #[bits(0..=7, rw)]
    reglist: u8,
    #[bit(8, rw)]
    extra_reg: bool,
    // bits 9-15 set via prefix: PUSH=0xB400, POP=0xBC00
}

/// Format 16: Conditional branch
/// Bits [15:12]=1101 [11:8]=cond [7:0]=imm8
#[bitfield(u16)]
struct CondBranch {
    #[bits(0..=7, rw)]
    imm8: u8,
    #[bits(8..=11, rw)]
    cond: Condition,
    // bits 12-15: 1101 (prefix 0xD000)
}

/// Format 17: SVC
/// Bits [15:8]=11011111 [7:0]=imm8
#[bitfield(u16)]
struct Svc {
    #[bits(0..=7, rw)]
    imm8: u8,
    // bits 8-15: 11011111 (prefix 0xDF00)
}

/// Format 18: Unconditional branch
/// Bits [15:11]=11100 [10:0]=imm11
#[bitfield(u16)]
struct UncondBranch {
    #[bits(0..=10, rw)]
    imm11: u11,
    // bits 11-15: 11100 (prefix 0xE000)
}

// ---------------------------------------------------------------------------
// Thumb-2 (32-bit) helpers
// ---------------------------------------------------------------------------

/// Emit a 32-bit Thumb-2 instruction as two little-endian halfwords.
fn emit32_thumb(hw1: u16, hw2: u16) -> Vec<u8> {
    let mut v = Vec::with_capacity(4);
    v.extend_from_slice(&hw1.to_le_bytes());
    v.extend_from_slice(&hw2.to_le_bytes());
    v
}

/// Encode a 32-bit value into the Thumb modified immediate constant (12-bit encoding).
/// Returns the 12-bit value (i:imm3:imm8) or None if not representable.
fn thumb_expand_imm_encoding(value: u32) -> Option<u16> {
    // Case 1: plain 8-bit
    if value <= 255 {
        return Some(value as u16);
    }
    let lo = value & 0xFF;
    // Case 2: 0x00XX00XX
    if lo != 0 && value == (lo | (lo << 16)) {
        return Some(0x100 | lo as u16);
    }
    // Case 3: 0xXX00XX00
    let hi = (value >> 8) & 0xFF;
    if hi != 0 && value == ((hi << 8) | (hi << 24)) {
        return Some(0x200 | hi as u16);
    }
    // Case 4: 0xXXXXXXXX
    if lo != 0 && value == (lo | (lo << 8) | (lo << 16) | (lo << 24)) {
        return Some(0x300 | lo as u16);
    }
    // Case 5: rotated 1:imm7
    for rot in 8u32..=31 {
        let unrotated = value.rotate_left(rot);
        if unrotated >= 0x80 && unrotated <= 0xFF {
            let imm12 = (rot << 7) | (unrotated & 0x7F);
            return Some(imm12 as u16);
        }
    }
    None
}

/// Map mnemonic to Thumb-2 data processing opcode (4-bit).
fn t2_dp_opcode(m: Mnemonic) -> u8 {
    match m {
        Mnemonic::And | Mnemonic::Tst => 0b0000,
        Mnemonic::Bic => 0b0001,
        Mnemonic::Orr | Mnemonic::Mov => 0b0010,
        Mnemonic::Orn | Mnemonic::Mvn => 0b0011,
        Mnemonic::Eor | Mnemonic::Teq => 0b0100,
        Mnemonic::Add | Mnemonic::Cmn => 0b1000,
        Mnemonic::Adc => 0b1010,
        Mnemonic::Sbc => 0b1011,
        Mnemonic::Sub | Mnemonic::Cmp => 0b1101,
        Mnemonic::Rsb | Mnemonic::Neg => 0b1110,
        _ => panic!("not a T2 DP opcode: {:?}", m),
    }
}

/// Predict whether this instruction will need a 32-bit (wide) encoding.
/// Called from instruction_size for instructions with both narrow and wide forms.
pub fn thumb_instruction_size(inst: &Instruction) -> u32 {
    use Mnemonic::*;
    match inst.mnemonic {
        Bl => 4,
        // LDM/STM always wide in Thumb-2 (16-bit only has PUSH/POP and LDMIA!/STMIA! for low regs)
        Ldm | Ldmia | Ldmfd | Ldmdb | Stm | Stmia | Stmea | Stmdb | Stmfd => {
            match inst.operands.as_slice() {
                // LDMIA SP!, {R0-R7, PC} → narrow POP
                [Operand::Reg(SP), Operand::RegList(mask)]
                    if matches!(inst.mnemonic, Ldm | Ldmia | Ldmfd)
                        && inst.writeback
                        && (*mask & 0x7F00) == 0 =>
                {
                    2
                }
                // Narrow LDMIA/STMIA only for low base reg with writeback
                [Operand::Reg(rn), Operand::RegList(mask)]
                    if rn.value() <= 7 && (*mask & 0xFF00) == 0 =>
                {
                    match inst.mnemonic {
                        Ldmia | Ldm | Ldmfd | Stmia | Stm | Stmea => 2,
                        _ => 4,
                    }
                }
                _ => 4,
            }
        }
        // No narrow form at all
        Rsb | Teq => 4,
        // Check operands for narrow feasibility
        Mov | Mvn | Add | Adc | Sub | Sbc | And | Orr | Eor | Bic
        | Cmp | Cmn | Tst => {
            match inst.operands.as_slice() {
                // High register or large immediate -> wide
                [Operand::Reg(rd), Operand::Imm(imm)]
                    if rd.value() > 7 || *imm < 0 || *imm > 255 =>
                {
                    4
                }
                // MOV Rd, #imm without S flag (outside IT): narrow sets flags, need wide
                [Operand::Reg(rd), Operand::Imm(_)]
                    if matches!(inst.mnemonic, Mov)
                        && !inst.set_flags && inst.condition.is_none()
                        && rd.value() <= 7 =>
                {
                    4
                }
                [Operand::Reg(rd), Operand::Reg(rn), Operand::Imm(imm)]
                    if rd.value() > 7 || rn.value() > 7 || *imm < 0 || *imm > 7 =>
                {
                    if matches!(inst.mnemonic, Add | Sub) {
                        let v = *imm as u32;
                        if rd.value() == 13 && rn.value() == 13 {
                            // ADD/SUB SP, SP, #imm7*4 (Format 13: 0..508)
                            if v % 4 == 0 && v <= 508 { 2 } else { 4 }
                        } else if rn.value() == 13 && rd.value() <= 7 && matches!(inst.mnemonic, Add) {
                            // ADD Rd, SP, #imm8*4 (Format 12: 0..1020)
                            if v % 4 == 0 && v <= 1020 { 2 } else { 4 }
                        } else if rd.value() <= 7 && rn == rd && *imm >= 0 && *imm <= 255 {
                            // Format 3: ADDS/SUBS Rd, #imm8 (Rd == Rn)
                            2
                        } else {
                            4
                        }
                    } else if rd.value() <= 7 && rn == rd && *imm >= 0 && *imm <= 255
                        && matches!(inst.mnemonic, Cmp)
                    {
                        // Format 3: CMP Rd, #imm8
                        2
                    } else {
                        4
                    }
                }
                // 2-reg form: only MOV, ADD, CMP have hi-reg narrow (Format 5)
                [Operand::Reg(rd), Operand::Reg(rm)]
                    if (rd.value() > 7 || rm.value() > 7)
                        && !matches!(inst.mnemonic, Mov | Add | Cmp) =>
                {
                    4
                }
                // 3-reg form: only ADD/SUB have dedicated 3-reg narrow;
                // ALU ops (AND, ORR, etc.) need Rd == Rn to collapse to 2-reg narrow
                [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm)]
                    if rd.value() > 7 || rn.value() > 7 || rm.value() > 7 =>
                {
                    4
                }
                [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(_)]
                    if rd != rn && !matches!(inst.mnemonic, Add | Sub) =>
                {
                    4
                }
                // Shifted register operand -> always wide
                [_, _, Operand::Shifted(..)] => 4,
                [_, Operand::Shifted(..)] => 4,
                _ => 2,
            }
        }
        Ldr | Str | Ldrb | Strb | Ldrh | Strh => {
            match inst.operands.as_slice() {
                [Operand::Reg(rt), Operand::Memory { base, offset: MemOffset::Imm(imm), .. }] => {
                    if rt.value() > 7 || (base.value() > 7 && base.value() != 13) {
                        return 4;
                    }
                    match inst.mnemonic {
                        Ldr | Str if base.value() == 13 => {
                            if *imm < 0 || *imm > 1020 || (*imm as u32) % 4 != 0 { 4 } else { 2 }
                        }
                        Ldr | Str => {
                            if *imm < 0 || *imm > 124 || (*imm as u32) % 4 != 0 { 4 } else { 2 }
                        }
                        Ldrb | Strb => {
                            if *imm < 0 || *imm > 31 { 4 } else { 2 }
                        }
                        Ldrh | Strh => {
                            if *imm < 0 || *imm > 62 || (*imm as u32) % 2 != 0 { 4 } else { 2 }
                        }
                        _ => 2,
                    }
                }
                [Operand::Reg(rt), Operand::Memory { base, offset: MemOffset::Reg(rm, _), .. }] => {
                    if rt.value() > 7 || base.value() > 7 || rm.value() > 7 { 4 } else { 2 }
                }
                [Operand::Reg(rt), Operand::Memory { offset: MemOffset::RegShift(..), .. }] => {
                    let _ = rt;
                    4
                }
                _ => 2,
            }
        }
        Ldrsb | Ldrsh => 4, // No narrow encoding for signed loads with immediate
        Lsl | Lsr | Asr | Ror => {
            match inst.operands.as_slice() {
                [Operand::Reg(rd), Operand::Reg(rm), ..] if rd.value() > 7 || rm.value() > 7 => 4,
                _ => 2,
            }
        }
        Mul => {
            match inst.operands.as_slice() {
                [Operand::Reg(rd), Operand::Reg(_), Operand::Reg(rd2)]
                    if rd.value() <= 7 && rd2.value() <= 7 && rd == rd2 => 2,
                [Operand::Reg(rd), Operand::Reg(rm)] if rd.value() <= 7 && rm.value() <= 7 => 2,
                _ => 4,
            }
        }
        Mla | Mls | Umull | Smull | Umlal | Smlal | Dmb | Dsb | Isb => 4,
        Push | Pop => {
            match inst.operands.as_slice() {
                [Operand::RegList(mask)] if (*mask & 0x1F00) != 0 => 4,
                _ => 2,
            }
        }
        Rev | Rev16 | Revsh | Sxth | Sxtb | Uxth | Uxtb => {
            match inst.operands.as_slice() {
                [Operand::Reg(rd), Operand::Reg(rm)] if rd.value() > 7 || rm.value() > 7 => 4,
                _ => 2,
            }
        }
        Neg => {
            match inst.operands.as_slice() {
                [Operand::Reg(rd), Operand::Reg(rm)] if rd.value() > 7 || rm.value() > 7 => 4,
                _ => 2,
            }
        }
        _ => 2,
    }
}

// ---------------------------------------------------------------------------
// Encoding entry point
// ---------------------------------------------------------------------------

pub fn encode_thumb(
    inst: &Instruction,
    offset: u32,
    symbols: &HashMap<String, (usize, u32)>,
    equs: &HashMap<String, i64>,
) -> Result<Vec<u8>, AsmError> {
    use Mnemonic::*;

    // Always-wide Thumb-2 instructions dispatch directly
    if is_always_t2(inst.mnemonic) {
        return encode_thumb_wide(inst, offset, symbols, equs);
    }

    // If .W suffix is set, go directly to wide encoding
    if inst.wide {
        return encode_thumb_wide(inst, offset, symbols, equs);
    }

    // Try narrow (16-bit) encoding first
    let narrow = match inst.mnemonic {
        Mov => encode_mov(inst),
        Mvn => match inst.operands.as_slice() {
            [Operand::Reg(_), Operand::Reg(_)] => encode_alu(inst),
            _ => encode_mov(inst),
        },
        Add => encode_add(inst, offset, symbols, equs),
        Sub => encode_sub(inst, offset),
        Cmp | Cmn => encode_cmp(inst),
        And | Orr | Eor | Bic | Adc | Sbc | Tst | Neg | Mul => encode_alu(inst),
        Lsl | Lsr | Asr | Ror => encode_shift(inst),
        Ldr => encode_ldr(inst, offset, symbols, equs),
        Str => encode_str(inst),
        Ldrb => encode_ldrb(inst),
        Strb => encode_strb(inst),
        Ldrh => encode_ldrh(inst),
        Strh => encode_strh(inst),
        Push => encode_push(inst),
        Pop => encode_pop(inst),
        B => encode_branch(inst, offset, symbols, equs),
        Bl => encode_bl(inst, offset, symbols, equs),
        Bx => encode_bx(inst),
        Blx => encode_blx(inst),
        Nop => Ok(0xBF00u16.to_le_bytes().to_vec()),
        Svc => encode_svc(inst),
        Adr => encode_adr(inst, offset, symbols, equs),
        Rev | Rev16 | Revsh | Sxth | Sxtb | Uxth | Uxtb => encode_misc_thumb(inst),
        Wfi => Ok(0xBF30u16.to_le_bytes().to_vec()),
        Wfe => Ok(0xBF20u16.to_le_bytes().to_vec()),
        Sev => Ok(0xBF40u16.to_le_bytes().to_vec()),
        Bkpt => encode_bkpt(inst),
        Cbz => encode_cbz_cbnz(inst, offset, symbols, equs),
        Cbnz => encode_cbz_cbnz(inst, offset, symbols, equs),
        It => encode_it(inst),
        Dmb => encode_barrier_thumb(inst, 0x5F),
        Dsb => encode_barrier_thumb(inst, 0x4F),
        Isb => encode_barrier_thumb(inst, 0x6F),
        Ldm | Ldmia | Ldmfd => encode_ldm_narrow(inst),
        Stm | Stmia | Stmea => encode_stm_narrow(inst),
        Rsb => Err(AsmError::new(inst.line, "RSB requires wide encoding")),
        _ => Err(AsmError::new(
            inst.line,
            format!("{:?} not supported in narrow Thumb", inst.mnemonic),
        )),
    };

    match narrow {
        Ok(bytes) => Ok(bytes),
        Err(e) => {
            // Instructions with no wide form: propagate error directly
            if matches!(inst.mnemonic, Cbz | Cbnz | It | Bkpt) {
                return Err(e);
            }
            encode_thumb_wide(inst, offset, symbols, equs)
        }
    }
}

/// Returns true for mnemonics that are always 32-bit in Thumb-2.
fn is_always_t2(m: Mnemonic) -> bool {
    use Mnemonic::*;
    matches!(
        m,
        Movw | Movt | Orn | Sdiv | Udiv | Mls | Mla
        | Smlal | Umlal | Umull | Smull
        | Clz | Rbit | Bfi | Bfc | Ubfx | Sbfx
        | Ldrd | Strd | Ldrex | Strex
        | Ldrexb | Strexb | Ldrexh | Strexh | Clrex
        | Mrs | Msr | Tbb | Tbh | Ssat | Usat
        | Ldrt | Strt | Ldrbt | Strbt | Ldrht | Strht | Ldrsbt | Ldrsht
        | Ldrsb | Ldrsh
        | Pld | Pli
        | Smmul | Smmla | Smmls
        | Smulbb | Smulbt | Smultb | Smultt
        | Smlabb | Smlabt | Smlatb | Smlatt
        | Smlalbb | Smlalbt | Smlaltb | Smlaltt
        | Smuad | Smusd | Smlad | Smlsd | Smlald | Smlsld
        | Usad8 | Usada8
        | Sadd16 | Sadd8 | Ssub16 | Ssub8
        | Uadd16 | Uadd8 | Usub16 | Usub8
        | Qadd16 | Qadd8 | Qsub16 | Qsub8
        | Shadd16 | Shadd8 | Shsub16 | Shsub8
        | Uhadd16 | Uhadd8 | Uhsub16 | Uhsub8
        | Uqadd16 | Uqadd8 | Uqsub16 | Uqsub8
        | Sasx | Ssax | Uasx | Usax
        | Qasx | Qsax | Shasx | Shsax | Uhasx | Uhsax | Uqasx | Uqsax
        | Qadd | Qdadd | Qsub | Qdsub
        | Pkhbt | Pkhtb | Sel
        | Sxtab | Sxtah | Uxtab | Uxtah
        | Sxtab16 | Uxtab16 | Sxtb16 | Uxtb16
        | Cpsie | Cpsid
        | Ldmdb | Stmdb | Stmfd
        | Dbg | Rrx
    )
}

/// Wide (32-bit Thumb-2) encoding dispatch.
fn encode_thumb_wide(
    inst: &Instruction,
    offset: u32,
    symbols: &HashMap<String, (usize, u32)>,
    equs: &HashMap<String, i64>,
) -> Result<Vec<u8>, AsmError> {
    use Mnemonic::*;
    match inst.mnemonic {
        // Data processing (wide)
        Mov | Mvn | Add | Adc | Sub | Sbc | Rsb | Neg | And | Orr | Orn | Eor | Bic | Cmp
        | Cmn | Tst | Teq => encode_t2_data_proc(inst),
        // Move wide immediate
        Movw | Movt => encode_t2_movw_movt(inst),
        // Shifts (wide)
        Lsl | Lsr | Asr | Ror | Rrx => encode_t2_shift(inst),
        // Multiply / divide
        Mul | Mla | Mls => encode_t2_mul(inst),
        Umull | Smull | Umlal | Smlal => encode_t2_long_mul(inst),
        Sdiv | Udiv => encode_t2_div(inst),
        // Load/store (wide)
        Ldr | Str | Ldrb | Strb | Ldrh | Strh | Ldrsb | Ldrsh => {
            encode_t2_ldr_str(inst, offset, symbols, equs)
        }
        Ldrd | Strd => encode_t2_ldrd_strd(inst, offset, symbols, equs),
        // Load/store multiple (wide)
        Ldm | Ldmia | Ldmfd | Ldmdb => encode_t2_ldm(inst),
        Stm | Stmia | Stmea | Stmdb | Stmfd => encode_t2_stm(inst),
        Push => encode_t2_push(inst),
        Pop => encode_t2_pop(inst),
        // Branch (wide)
        B => encode_t2_branch(inst, offset, symbols, equs),
        Bl => encode_bl(inst, offset, symbols, equs),
        // Bit manipulation
        Clz => encode_t2_clz_rbit(inst, 0b1011, 0b1000),
        Rbit => encode_t2_clz_rbit(inst, 0b1001, 0b1010),
        Bfi => encode_t2_bfi(inst),
        Bfc => encode_t2_bfc(inst),
        Ubfx => encode_t2_bfx(inst, false),
        Sbfx => encode_t2_bfx(inst, true),
        // Exclusive load/store
        Ldrex => encode_t2_ldrex(inst),
        Strex => encode_t2_strex(inst),
        Ldrexb | Ldrexh => encode_t2_ldrex_bh(inst),
        Strexb | Strexh => encode_t2_strex_bh(inst),
        Clrex => Ok(emit32_thumb(0xF3BF, 0x8F2F)),
        // System
        Mrs => encode_t2_mrs(inst),
        Msr => encode_t2_msr(inst),
        // Table branch
        Tbb => encode_t2_tb(inst, false),
        Tbh => encode_t2_tb(inst, true),
        // Saturation
        Ssat => encode_t2_ssat(inst),
        Usat => encode_t2_usat(inst),
        // Byte reversal / extend (wide forms)
        Rev | Rev16 | Revsh | Sxth | Sxtb | Uxth | Uxtb | Sxtb16 | Uxtb16 => {
            encode_t2_extend(inst)
        }
        // Extend and add
        Sxtab | Sxtah | Uxtab | Uxtah | Sxtab16 | Uxtab16 => encode_t2_extend_add(inst),
        // DSP multiply
        Smmul | Smmla | Smmls | Smulbb | Smulbt | Smultb | Smultt | Smlabb | Smlabt
        | Smlatb | Smlatt | Smuad | Smusd | Smlad | Smlsd | Usad8 | Usada8 => {
            encode_t2_dsp_mul(inst)
        }
        Smlalbb | Smlalbt | Smlaltb | Smlaltt | Smlald | Smlsld => encode_t2_dsp_long_mul(inst),
        // Parallel arithmetic
        Sadd16 | Sadd8 | Ssub16 | Ssub8 | Uadd16 | Uadd8 | Usub16 | Usub8 | Qadd16
        | Qadd8 | Qsub16 | Qsub8 | Shadd16 | Shadd8 | Shsub16 | Shsub8 | Uhadd16 | Uhadd8
        | Uhsub16 | Uhsub8 | Uqadd16 | Uqadd8 | Uqsub16 | Uqsub8 | Sasx | Ssax | Uasx
        | Usax | Qasx | Qsax | Shasx | Shsax | Uhasx | Uhsax | Uqasx | Uqsax => {
            encode_t2_parallel(inst)
        }
        // Saturating arithmetic
        Qadd | Qdadd | Qsub | Qdsub => encode_t2_sat_arith(inst),
        // Packing
        Pkhbt | Pkhtb => encode_t2_pkhbt(inst),
        Sel => encode_t2_sel(inst),
        // Hints / system (wide)
        Nop => Ok(emit32_thumb(0xF3AF, 0x8000)),
        Dmb => encode_barrier_thumb(inst, 0x5F),
        Dsb => encode_barrier_thumb(inst, 0x4F),
        Isb => encode_barrier_thumb(inst, 0x6F),
        Dbg => encode_t2_dbg(inst),
        // Unprivileged load/store
        Ldrt | Ldrbt | Ldrht | Ldrsbt | Ldrsht | Strt | Strbt | Strht => {
            encode_t2_ldr_str_unpriv(inst)
        }
        Cpsie | Cpsid => encode_t2_cps(inst),
        Pld | Pli => encode_t2_preload(inst),
        _ => Err(AsmError::new(
            inst.line,
            format!("{:?} not supported in Thumb mode", inst.mnemonic),
        )),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn emit16(hw: u16) -> Vec<u8> {
    hw.to_le_bytes().to_vec()
}

/// Convert a low register (R0-R7) to u3 for narrow Thumb encodings.
/// Callers must ensure r <= 7 via match guards; panics in debug if not.
fn lo(r: u4) -> u3 {
    u3::new(r.value())
}

fn require_lo(r: u4, line: usize, ctx: &str) -> Result<u3, AsmError> {
    if r.value() > 7 {
        Err(AsmError::new(line, format!("{ctx}: register R{} must be R0-R7", r.value())))
    } else {
        Ok(u3::new(r.value()))
    }
}

fn imm_u8(val: i64, line: usize) -> Result<u8, AsmError> {
    if val < 0 || val > 255 {
        Err(AsmError::new(line, format!("immediate {val} out of range 0..255")))
    } else {
        Ok(val as u8)
    }
}

// ---------------------------------------------------------------------------
// Individual encoders
// ---------------------------------------------------------------------------

fn encode_mov(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        // MOVS Rd, #imm8 (Rd low) -- narrow only when S flag set or inside IT block
        [Operand::Reg(rd), Operand::Imm(imm)]
            if rd.value() <= 7 && (inst.set_flags || inst.condition.is_some()) =>
        {
            let hw = DataImm8::new_with_raw_value(0x2000)
                .with_op(u2::new(0b00))
                .with_rd(require_lo(*rd, line, "MOVS")?)
                .with_imm8(imm_u8(*imm, line)?);
            Ok(emit16(hw.raw_value()))
        }
        // MOV Rd, Rm (any registers, format 5)
        [Operand::Reg(rd), Operand::Reg(rm)] if !inst.set_flags || rd.value() > 7 || rm.value() > 7 => {
            let hw = HiRegOp::new_with_raw_value(0x4600)
                .with_rd(*rd)
                .with_rm(*rm);
            Ok(emit16(hw.raw_value()))
        }
        // MOVS Rd, Rm (low registers only, encoded as LSL Rd, Rm, #0)
        [Operand::Reg(rd), Operand::Reg(rm)] => {
            let hw = ShiftImm::new_with_raw_value(0x0000)
                .with_op(u2::new(0b00))
                .with_imm5(u5::new(0))
                .with_rm(require_lo(*rm, line, "MOVS")?)
                .with_rd(require_lo(*rd, line, "MOVS")?);
            Ok(emit16(hw.raw_value()))
        }
        _ => Err(AsmError::new(line, "invalid operands for MOV")),
    }
}

fn encode_add(
    inst: &Instruction,
    offset: u32,
    symbols: &HashMap<String, (usize, u32)>,
    equs: &HashMap<String, i64>,
) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        // ADD Rd, SP, #imm8 (Format 12)
        [Operand::Reg(rd), Operand::Reg(SP), Operand::Imm(imm)] if rd.value() <= 7 => {
            let scaled = *imm as u32;
            if scaled % 4 != 0 || scaled > 1020 {
                return Err(AsmError::new(line, "ADD Rd, SP, #imm: immediate must be 0..1020, word-aligned"));
            }
            let hw = LoadAddr::new_with_raw_value(0xA800)
                .with_rd(require_lo(*rd, line, "ADD")?)
                .with_imm8((scaled / 4) as u8);
            Ok(emit16(hw.raw_value()))
        }
        // ADD SP, SP, #imm7 (Format 13)
        [Operand::Reg(SP), Operand::Reg(SP), Operand::Imm(imm)]
        | [Operand::Reg(SP), Operand::Imm(imm)] => {
            let scaled = *imm as u32;
            if scaled % 4 != 0 || scaled > 508 {
                return Err(AsmError::new(line, "ADD SP, #imm: immediate must be 0..508, word-aligned"));
            }
            let hw = AdjustSp::new_with_raw_value(0xB000)
                .with_sub(false)
                .with_imm7(u7::new((scaled / 4) as u8));
            Ok(emit16(hw.raw_value()))
        }
        // ADDS Rd, Rn, #imm8 (Format 3) when Rd == Rn -- preferred by GNU as
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Imm(imm)]
            if rd.value() <= 7 && rd == rn && *imm >= 0 && *imm <= 255 =>
        {
            let hw = DataImm8::new_with_raw_value(0x3000)
                .with_rd(require_lo(*rd, line, "ADDS")?)
                .with_imm8(*imm as u8);
            Ok(emit16(hw.raw_value()))
        }
        // ADDS Rd, Rn, #imm3 (Format 2)
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Imm(imm)]
            if rd.value() <= 7 && rn.value() <= 7 && *imm >= 0 && *imm <= 7 =>
        {
            let hw = AddSubRegImm3::new_with_raw_value(0x1C00)
                .with_rd(require_lo(*rd, line, "ADD")?)
                .with_rn(require_lo(*rn, line, "ADD")?)
                .with_rm_imm3(u3::new(*imm as u8))
                .with_imm_flag(true)
                .with_op(false);
            Ok(emit16(hw.raw_value()))
        }
        // ADDS Rd, Rn, Rm (Format 2)
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm)]
            if rd.value() <= 7 && rn.value() <= 7 && rm.value() <= 7 =>
        {
            let hw = AddSubRegImm3::new_with_raw_value(0x1800)
                .with_rd(require_lo(*rd, line, "ADD")?)
                .with_rn(require_lo(*rn, line, "ADD")?)
                .with_rm_imm3(require_lo(*rm, line, "ADD")?)
                .with_imm_flag(false)
                .with_op(false);
            Ok(emit16(hw.raw_value()))
        }
        // ADDS Rd, #imm8 (Format 3)
        [Operand::Reg(rd), Operand::Imm(imm)] if rd.value() <= 7 => {
            let hw = DataImm8::new_with_raw_value(0x3000)
                .with_rd(require_lo(*rd, line, "ADDS")?)
                .with_imm8(imm_u8(*imm, line)?);
            Ok(emit16(hw.raw_value()))
        }
        // ADD Rd, Rm (high registers, Format 5)
        [Operand::Reg(rd), Operand::Reg(rm)] if rd.value() > 7 || rm.value() > 7 => {
            let hw = HiRegOp::new_with_raw_value(0x4400)
                .with_op(u2::new(0b00))
                .with_rd(*rd)
                .with_rm(*rm);
            Ok(emit16(hw.raw_value()))
        }
        // ADD Rd, PC, #imm (label, Format 12 = ADR)
        [Operand::Reg(rd), Operand::Label(name)] if rd.value() <= 7 => {
            let target = resolve_label(name, symbols, equs, line)?;
            let pc = (offset + 4) & !3; // Thumb PC is aligned
            let disp = target.wrapping_sub(pc);
            if disp % 4 != 0 || disp > 1020 {
                return Err(AsmError::new(line, "ADD PC-relative: offset out of range"));
            }
            let hw = LoadAddr::new_with_raw_value(0xA000)
                .with_rd(require_lo(*rd, line, "ADD")?)
                .with_imm8((disp / 4) as u8);
            Ok(emit16(hw.raw_value()))
        }
        _ => Err(AsmError::new(line, "invalid operands for ADD in Thumb mode")),
    }
}

fn encode_sub(inst: &Instruction, _offset: u32) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        // SUB SP, SP, #imm7 (Format 13)
        [Operand::Reg(SP), Operand::Reg(SP), Operand::Imm(imm)]
        | [Operand::Reg(SP), Operand::Imm(imm)] => {
            let scaled = *imm as u32;
            if scaled % 4 != 0 || scaled > 508 {
                return Err(AsmError::new(line, "SUB SP, #imm: immediate must be 0..508, word-aligned"));
            }
            let hw = AdjustSp::new_with_raw_value(0xB080)
                .with_imm7(u7::new((scaled / 4) as u8));
            Ok(emit16(hw.raw_value()))
        }
        // SUBS Rd, Rn, #imm8 (Format 3) when Rd == Rn -- preferred by GNU as
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Imm(imm)]
            if rd.value() <= 7 && rd == rn && *imm >= 0 && *imm <= 255 =>
        {
            let hw = DataImm8::new_with_raw_value(0x3800)
                .with_rd(require_lo(*rd, line, "SUBS")?)
                .with_imm8(*imm as u8);
            Ok(emit16(hw.raw_value()))
        }
        // SUBS Rd, Rn, #imm3
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Imm(imm)]
            if rd.value() <= 7 && rn.value() <= 7 && *imm >= 0 && *imm <= 7 =>
        {
            let hw = AddSubRegImm3::new_with_raw_value(0x1E00)
                .with_rd(require_lo(*rd, line, "SUB")?)
                .with_rn(require_lo(*rn, line, "SUB")?)
                .with_rm_imm3(u3::new(*imm as u8))
                .with_imm_flag(true)
                .with_op(true);
            Ok(emit16(hw.raw_value()))
        }
        // SUBS Rd, Rn, Rm
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm)]
            if rd.value() <= 7 && rn.value() <= 7 && rm.value() <= 7 =>
        {
            let hw = AddSubRegImm3::new_with_raw_value(0x1A00)
                .with_rd(require_lo(*rd, line, "SUB")?)
                .with_rn(require_lo(*rn, line, "SUB")?)
                .with_rm_imm3(require_lo(*rm, line, "SUB")?)
                .with_imm_flag(false)
                .with_op(true);
            Ok(emit16(hw.raw_value()))
        }
        // SUBS Rd, #imm8
        [Operand::Reg(rd), Operand::Imm(imm)] if rd.value() <= 7 => {
            let hw = DataImm8::new_with_raw_value(0x3800)
                .with_rd(require_lo(*rd, line, "SUBS")?)
                .with_imm8(imm_u8(*imm, line)?);
            Ok(emit16(hw.raw_value()))
        }
        _ => Err(AsmError::new(line, "invalid operands for SUB in Thumb mode")),
    }
}

fn encode_cmp(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        // CMP Rn, #imm8 (Format 3)
        [Operand::Reg(rn), Operand::Imm(imm)] if rn.value() <= 7 => {
            let hw = DataImm8::new_with_raw_value(0x2800)
                .with_rd(require_lo(*rn, line, "CMP")?)
                .with_imm8(imm_u8(*imm, line)?);
            Ok(emit16(hw.raw_value()))
        }
        // CMP Rn, Rm (low regs, Format 4)
        [Operand::Reg(rn), Operand::Reg(rm)] if rn.value() <= 7 && rm.value() <= 7 => {
            let hw = AluOp::new_with_raw_value(0x4000)
                .with_op(u4::new(0b1010))
                .with_rm(require_lo(*rm, line, "CMP")?)
                .with_rd(require_lo(*rn, line, "CMP")?);
            Ok(emit16(hw.raw_value()))
        }
        // CMP Rn, Rm (high regs, Format 5)
        [Operand::Reg(rn), Operand::Reg(rm)] => {
            let hw = HiRegOp::new_with_raw_value(0x4500)
                .with_rd(*rn)
                .with_rm(*rm);
            Ok(emit16(hw.raw_value()))
        }
        _ => Err(AsmError::new(line, "invalid operands for CMP in Thumb mode")),
    }
}

fn encode_alu(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let (rd, rm) = match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rm)] => (*rd, *rm),
        // 3-operand form where Rd == Rn: collapse to 2-reg (AND Rd, Rd, Rm -> ANDS Rd, Rm)
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm)] if rd == rn => (*rd, *rm),
        // MUL Rd, Rn, Rd - three-operand form (Rd must match first and third)
        [Operand::Reg(rd), Operand::Reg(rm), Operand::Reg(rd2)]
            if inst.mnemonic == Mnemonic::Mul && rd == rd2 =>
        {
            (*rd, *rm)
        }
        _ => return Err(AsmError::new(line, "ALU ops require two low registers")),
    };

    let opcode = match inst.mnemonic {
        Mnemonic::And => 0b0000,
        Mnemonic::Eor => 0b0001,
        Mnemonic::Adc => 0b0101,
        Mnemonic::Sbc => 0b0110,
        Mnemonic::Tst => 0b1000,
        Mnemonic::Neg => 0b1001, // RSB Rd, Rm, #0
        Mnemonic::Cmn => 0b1011,
        Mnemonic::Orr => 0b1100,
        Mnemonic::Mul => 0b1101,
        Mnemonic::Bic => 0b1110,
        Mnemonic::Mvn => 0b1111,
        _ => return Err(AsmError::new(line, "unsupported ALU operation")),
    };

    let hw = AluOp::new_with_raw_value(0x4000)
        .with_op(u4::new(opcode))
        .with_rm(require_lo(rm, line, "ALU")?)
        .with_rd(require_lo(rd, line, "ALU")?);
    Ok(emit16(hw.raw_value()))
}

fn encode_shift(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let op = match inst.mnemonic {
        Mnemonic::Lsl => 0b00u8,
        Mnemonic::Lsr => 0b01,
        Mnemonic::Asr => 0b10,
        Mnemonic::Ror => {
            // ROR is only register-register in Thumb (Format 4)
            return match inst.operands.as_slice() {
                [Operand::Reg(rd), Operand::Reg(rm)] => {
                    let hw = AluOp::new_with_raw_value(0x4000)
                        .with_op(u4::new(0b0111))
                        .with_rm(require_lo(*rm, line, "ROR")?)
                        .with_rd(require_lo(*rd, line, "ROR")?);
                    Ok(emit16(hw.raw_value()))
                }
                _ => Err(AsmError::new(line, "ROR in Thumb only supports register form")),
            };
        }
        _ => unreachable!(),
    };

    match inst.operands.as_slice() {
        // LSL/LSR/ASR Rd, Rm, #imm5 (Format 1)
        [Operand::Reg(rd), Operand::Reg(rm), Operand::Imm(imm)] => {
            // ARM encodes LSR #32 and ASR #32 as imm5=0
            let imm5 = (*imm as u8) & 0x1F;
            let hw = ShiftImm::new_with_raw_value(0x0000)
                .with_op(u2::new(op))
                .with_imm5(u5::new(imm5))
                .with_rm(require_lo(*rm, line, "shift")?)
                .with_rd(require_lo(*rd, line, "shift")?);
            Ok(emit16(hw.raw_value()))
        }
        // LSL/LSR/ASR Rd, Rm (Format 4, register shift)
        [Operand::Reg(rd), Operand::Reg(rm)] => {
            let alu_op = match inst.mnemonic {
                Mnemonic::Lsl => 0b0010,
                Mnemonic::Lsr => 0b0011,
                Mnemonic::Asr => 0b0100,
                _ => unreachable!(),
            };
            let hw = AluOp::new_with_raw_value(0x4000)
                .with_op(u4::new(alu_op))
                .with_rm(require_lo(*rm, line, "shift")?)
                .with_rd(require_lo(*rd, line, "shift")?);
            Ok(emit16(hw.raw_value()))
        }
        _ => Err(AsmError::new(line, "invalid operands for shift")),
    }
}

fn encode_ldr(
    inst: &Instruction,
    offset: u32,
    symbols: &HashMap<String, (usize, u32)>,
    equs: &HashMap<String, i64>,
) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        // LDR Rt, [Rn, #imm] (word, Format 9)
        [Operand::Reg(rt), Operand::Memory { base, offset: MemOffset::Imm(imm), .. }]
            if rt.value() <= 7 && base.value() <= 7 && base.value() != 13 =>
        {
            let val = *imm as u32;
            if val % 4 != 0 || val > 124 {
                return Err(AsmError::new(line, "LDR [Rn, #imm]: offset must be 0..124, word-aligned"));
            }
            let hw = LdrStrImm::new_with_raw_value(0x6800)
                .with_rt(lo(*rt))
                .with_rn(lo(*base))
                .with_imm5(u5::new((val / 4) as u8));
            Ok(emit16(hw.raw_value()))
        }
        // LDR Rt, [SP, #imm] (Format 11)
        [Operand::Reg(rt), Operand::Memory { base: SP, offset: MemOffset::Imm(imm), .. }]
            if rt.value() <= 7 =>
        {
            let val = *imm as u32;
            if val % 4 != 0 || val > 1020 {
                return Err(AsmError::new(line, "LDR [SP, #imm]: offset must be 0..1020, word-aligned"));
            }
            let hw = SpRelLdrStr::new_with_raw_value(0x9800)
                .with_rt(lo(*rt))
                .with_imm8((val / 4) as u8);
            Ok(emit16(hw.raw_value()))
        }
        // LDR Rt, [Rn, Rm] (register offset, Format 7)
        [Operand::Reg(rt), Operand::Memory { base, offset: MemOffset::Reg(rm, _), .. }]
            if rt.value() <= 7 && base.value() <= 7 && rm.value() <= 7 =>
        {
            let hw = LdrStrRegOff::new_with_raw_value(0x5800)
                .with_rt(lo(*rt))
                .with_rn(lo(*base))
                .with_rm(lo(*rm));
            Ok(emit16(hw.raw_value()))
        }
        // LDR Rt, label (PC-relative, Format 6)
        [Operand::Reg(rt), Operand::Label(name)] if rt.value() <= 7 => {
            let target = resolve_label(name, symbols, equs, line)?;
            let pc = (offset + 4) & !3;
            let disp = target.wrapping_sub(pc);
            if disp % 4 != 0 || disp > 1020 {
                return Err(AsmError::new(line, "LDR PC-relative: offset out of range (0..1020, word-aligned)"));
            }
            let hw: u16 = 0x4800 | (((rt.value() as u16) & 7) << 8) | ((disp / 4) as u16 & 0xFF);
            Ok(emit16(hw))
        }
        _ => Err(AsmError::new(line, "invalid operands for LDR in Thumb mode")),
    }
}

fn encode_str(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        // STR Rt, [Rn, #imm] (word, Format 9)
        [Operand::Reg(rt), Operand::Memory { base, offset: MemOffset::Imm(imm), .. }]
            if rt.value() <= 7 && base.value() <= 7 && base.value() != 13 =>
        {
            let val = *imm as u32;
            if val % 4 != 0 || val > 124 {
                return Err(AsmError::new(line, "STR [Rn, #imm]: offset must be 0..124, word-aligned"));
            }
            let hw = LdrStrImm::new_with_raw_value(0x6000)
                .with_rt(lo(*rt))
                .with_rn(lo(*base))
                .with_imm5(u5::new((val / 4) as u8));
            Ok(emit16(hw.raw_value()))
        }
        // STR Rt, [SP, #imm] (Format 11)
        [Operand::Reg(rt), Operand::Memory { base: SP, offset: MemOffset::Imm(imm), .. }]
            if rt.value() <= 7 =>
        {
            let val = *imm as u32;
            if val % 4 != 0 || val > 1020 {
                return Err(AsmError::new(line, "STR [SP, #imm]: offset must be 0..1020, word-aligned"));
            }
            let hw = SpRelLdrStr::new_with_raw_value(0x9000)
                .with_rt(lo(*rt))
                .with_imm8((val / 4) as u8);
            Ok(emit16(hw.raw_value()))
        }
        // STR Rt, [Rn, Rm] (register offset, Format 7)
        [Operand::Reg(rt), Operand::Memory { base, offset: MemOffset::Reg(rm, _), .. }]
            if rt.value() <= 7 && base.value() <= 7 && rm.value() <= 7 =>
        {
            let hw = LdrStrRegOff::new_with_raw_value(0x5000)
                .with_rt(lo(*rt))
                .with_rn(lo(*base))
                .with_rm(lo(*rm));
            Ok(emit16(hw.raw_value()))
        }
        _ => Err(AsmError::new(line, "invalid operands for STR in Thumb mode")),
    }
}

fn encode_ldrb(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rt), Operand::Memory { base, offset: MemOffset::Imm(imm), .. }]
            if rt.value() <= 7 && base.value() <= 7 =>
        {
            let val = *imm as u32;
            if val > 31 {
                return Err(AsmError::new(line, "LDRB: offset must be 0..31"));
            }
            let hw = LdrStrImm::new_with_raw_value(0x7800)
                .with_rt(lo(*rt))
                .with_rn(lo(*base))
                .with_imm5(u5::new(val as u8));
            Ok(emit16(hw.raw_value()))
        }
        [Operand::Reg(rt), Operand::Memory { base, offset: MemOffset::Reg(rm, _), .. }]
            if rt.value() <= 7 && base.value() <= 7 && rm.value() <= 7 =>
        {
            let hw = LdrStrRegOff::new_with_raw_value(0x5C00)
                .with_rt(lo(*rt))
                .with_rn(lo(*base))
                .with_rm(lo(*rm));
            Ok(emit16(hw.raw_value()))
        }
        _ => Err(AsmError::new(line, "invalid operands for LDRB")),
    }
}

fn encode_strb(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rt), Operand::Memory { base, offset: MemOffset::Imm(imm), .. }]
            if rt.value() <= 7 && base.value() <= 7 =>
        {
            let val = *imm as u32;
            if val > 31 {
                return Err(AsmError::new(line, "STRB: offset must be 0..31"));
            }
            let hw = LdrStrImm::new_with_raw_value(0x7000)
                .with_rt(lo(*rt))
                .with_rn(lo(*base))
                .with_imm5(u5::new(val as u8));
            Ok(emit16(hw.raw_value()))
        }
        [Operand::Reg(rt), Operand::Memory { base, offset: MemOffset::Reg(rm, _), .. }]
            if rt.value() <= 7 && base.value() <= 7 && rm.value() <= 7 =>
        {
            let hw = LdrStrRegOff::new_with_raw_value(0x5400)
                .with_rt(lo(*rt))
                .with_rn(lo(*base))
                .with_rm(lo(*rm));
            Ok(emit16(hw.raw_value()))
        }
        _ => Err(AsmError::new(line, "invalid operands for STRB")),
    }
}

fn encode_ldrh(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rt), Operand::Memory { base, offset: MemOffset::Imm(imm), .. }]
            if rt.value() <= 7 && base.value() <= 7 =>
        {
            let val = *imm as u32;
            if val % 2 != 0 || val > 62 {
                return Err(AsmError::new(line, "LDRH: offset must be 0..62, halfword-aligned"));
            }
            let hw = LdrStrHalfImm::new_with_raw_value(0x8800)
                .with_rt(lo(*rt))
                .with_rn(lo(*base))
                .with_imm5(u5::new((val / 2) as u8));
            Ok(emit16(hw.raw_value()))
        }
        [Operand::Reg(rt), Operand::Memory { base, offset: MemOffset::Reg(rm, _), .. }]
            if rt.value() <= 7 && base.value() <= 7 && rm.value() <= 7 =>
        {
            let hw = LdrStrRegOff::new_with_raw_value(0x5A00)
                .with_rt(lo(*rt))
                .with_rn(lo(*base))
                .with_rm(lo(*rm));
            Ok(emit16(hw.raw_value()))
        }
        _ => Err(AsmError::new(line, "invalid operands for LDRH")),
    }
}

fn encode_strh(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rt), Operand::Memory { base, offset: MemOffset::Imm(imm), .. }]
            if rt.value() <= 7 && base.value() <= 7 =>
        {
            let val = *imm as u32;
            if val % 2 != 0 || val > 62 {
                return Err(AsmError::new(line, "STRH: offset must be 0..62, halfword-aligned"));
            }
            let hw = LdrStrHalfImm::new_with_raw_value(0x8000)
                .with_rt(lo(*rt))
                .with_rn(lo(*base))
                .with_imm5(u5::new((val / 2) as u8));
            Ok(emit16(hw.raw_value()))
        }
        [Operand::Reg(rt), Operand::Memory { base, offset: MemOffset::Reg(rm, _), .. }]
            if rt.value() <= 7 && base.value() <= 7 && rm.value() <= 7 =>
        {
            let hw = LdrStrRegOff::new_with_raw_value(0x5200)
                .with_rt(lo(*rt))
                .with_rn(lo(*base))
                .with_rm(lo(*rm));
            Ok(emit16(hw.raw_value()))
        }
        _ => Err(AsmError::new(line, "invalid operands for STRH")),
    }
}

fn encode_push(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::RegList(mask)] => {
            let lr = (*mask >> 14) & 1 != 0;
            let lo_mask = (*mask & 0xFF) as u8;
            if *mask & 0x1F00 != 0 {
                return Err(AsmError::new(line, "PUSH: only R0-R7 and LR allowed"));
            }
            let hw = PushPop::new_with_raw_value(0xB400)
                .with_reglist(lo_mask)
                .with_extra_reg(lr);
            Ok(emit16(hw.raw_value()))
        }
        _ => Err(AsmError::new(line, "PUSH requires register list")),
    }
}

fn encode_pop(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::RegList(mask)] => {
            let pc = (*mask >> 15) & 1 != 0;
            let lo_mask = (*mask & 0xFF) as u8;
            if *mask & 0x7F00 != 0 {
                return Err(AsmError::new(line, "POP: only R0-R7 and PC allowed"));
            }
            let hw = PushPop::new_with_raw_value(0xBC00)
                .with_reglist(lo_mask)
                .with_extra_reg(pc);
            Ok(emit16(hw.raw_value()))
        }
        _ => Err(AsmError::new(line, "POP requires register list")),
    }
}

fn encode_branch(
    inst: &Instruction,
    offset: u32,
    symbols: &HashMap<String, (usize, u32)>,
    equs: &HashMap<String, i64>,
) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let target_label = match inst.operands.as_slice() {
        [Operand::Label(name)] => name,
        _ => return Err(AsmError::new(line, "B requires a label operand")),
    };

    let target = resolve_label(target_label, symbols, equs, line)?;
    let pc = offset + 4; // Thumb PC = current + 4

    if let Some(cond) = inst.condition {
        // Conditional branch (Format 16): signed 8-bit offset * 2
        let disp = target as i32 - pc as i32;
        if disp % 2 != 0 {
            return Err(AsmError::new(line, "branch target not halfword-aligned"));
        }
        let imm = disp / 2;
        if imm < -128 || imm > 127 {
            return Err(AsmError::new(line, format!("conditional branch out of range: offset {disp}")));
        }
        let hw = CondBranch::new_with_raw_value(0xD000)
            .with_cond(cond)
            .with_imm8(imm as u8);
        Ok(emit16(hw.raw_value()))
    } else {
        // Unconditional branch (Format 18): signed 11-bit offset * 2
        let disp = target as i32 - pc as i32;
        if disp % 2 != 0 {
            return Err(AsmError::new(line, "branch target not halfword-aligned"));
        }
        let imm = disp / 2;
        if imm < -1024 || imm > 1023 {
            return Err(AsmError::new(line, format!("unconditional branch out of range: offset {disp}")));
        }
        let hw = UncondBranch::new_with_raw_value(0xE000)
            .with_imm11(u11::new(imm as u16 & 0x7FF));
        Ok(emit16(hw.raw_value()))
    }
}

fn encode_bl(
    inst: &Instruction,
    offset: u32,
    symbols: &HashMap<String, (usize, u32)>,
    equs: &HashMap<String, i64>,
) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let target_label = match inst.operands.as_slice() {
        [Operand::Label(name)] => name,
        _ => return Err(AsmError::new(line, "BL requires a label operand")),
    };

    let target = resolve_label(target_label, symbols, equs, line)?;
    let pc = offset + 4;
    let disp = target as i32 - pc as i32;

    if disp % 2 != 0 {
        return Err(AsmError::new(line, "BL target not halfword-aligned"));
    }

    // 25-bit signed range: +-16MB
    if disp < -(1 << 24) || disp >= (1 << 24) {
        return Err(AsmError::new(line, "BL target out of range"));
    }

    let offset_u = disp as u32;
    let s = (offset_u >> 24) & 1;
    let imm10 = (offset_u >> 12) & 0x3FF;
    let imm11 = (offset_u >> 1) & 0x7FF;
    let i1 = (offset_u >> 23) & 1;
    let i2 = (offset_u >> 22) & 1;
    let j1 = (!(i1 ^ s)) & 1;
    let j2 = (!(i2 ^ s)) & 1;

    let hw1: u16 = (0xF000 | (s << 10) | imm10) as u16;
    let hw2: u16 = (0xD000 | (j1 << 13) | (j2 << 11) | imm11) as u16;

    let mut bytes = Vec::with_capacity(4);
    bytes.extend_from_slice(&hw1.to_le_bytes());
    bytes.extend_from_slice(&hw2.to_le_bytes());
    Ok(bytes)
}

fn encode_bx(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rm)] => {
            let hw = HiRegOp::new_with_raw_value(0x4700)
                .with_rm(*rm);
            Ok(emit16(hw.raw_value()))
        }
        _ => Err(AsmError::new(line, "BX requires one register operand")),
    }
}

fn encode_blx(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rm)] => {
            let hw = HiRegOp::new_with_raw_value(0x4780)
                .with_rm(*rm);
            Ok(emit16(hw.raw_value()))
        }
        _ => Err(AsmError::new(line, "BLX requires one register operand")),
    }
}

fn encode_svc(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Imm(imm)] => {
            let hw = Svc::new_with_raw_value(0xDF00).with_imm8(imm_u8(*imm, line)?);
            Ok(emit16(hw.raw_value()))
        }
        _ => Err(AsmError::new(line, "SVC requires immediate operand")),
    }
}

fn encode_adr(
    inst: &Instruction,
    offset: u32,
    symbols: &HashMap<String, (usize, u32)>,
    equs: &HashMap<String, i64>,
) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Label(name)] if rd.value() <= 7 => {
            let target = resolve_label(name, symbols, equs, line)?;
            let pc = (offset + 4) & !3;
            let disp = target.wrapping_sub(pc);
            if disp % 4 != 0 || disp > 1020 {
                return Err(AsmError::new(line, "ADR: offset out of range (0..1020, word-aligned)"));
            }
            let hw = LoadAddr::new_with_raw_value(0xA000)
                .with_rd(require_lo(*rd, line, "ADR")?)
                .with_imm8((disp / 4) as u8);
            Ok(emit16(hw.raw_value()))
        }
        _ => Err(AsmError::new(line, "ADR requires low register and label")),
    }
}

fn encode_bkpt(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Imm(imm)] => {
            let val = imm_u8(*imm, line)?;
            Ok(emit16(0xBE00 | val as u16))
        }
        _ => Err(AsmError::new(line, "BKPT requires immediate")),
    }
}

fn encode_barrier_thumb(inst: &Instruction, base_lo: u16) -> Result<Vec<u8>, AsmError> {
    let option: u16 = match inst.operands.as_slice() {
        [] => 0xF, // default SY
        [Operand::Label(s)] => {
            match s.to_ascii_uppercase().as_str() {
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
                _ => return Err(AsmError::new(inst.line, format!("unknown barrier option: {s}"))),
            }
        }
        [Operand::Imm(n)] => (*n as u16) & 0xF,
        _ => return Err(AsmError::new(inst.line, "barrier: need option")),
    };
    // base_lo has the barrier type in the low byte (e.g. 0x5F for DMB SY)
    // Replace low nibble with the option
    let hw2 = 0x8F00 | (base_lo & 0xF0) | option;
    Ok(emit32_thumb(0xF3BF, hw2))
}

fn encode_misc_thumb(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let (rd, rm) = match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rm)] => (*rd, *rm),
        _ => return Err(AsmError::new(line, "expected two registers")),
    };
    if rd.value() > 7 || rm.value() > 7 {
        return Err(AsmError::new(line, "narrow encoding requires R0-R7"));
    }
    let opcode: u16 = match inst.mnemonic {
        Mnemonic::Rev => 0xBA00,
        Mnemonic::Rev16 => 0xBA40,
        Mnemonic::Revsh => 0xBAC0,
        Mnemonic::Sxth => 0xB200,
        Mnemonic::Sxtb => 0xB240,
        Mnemonic::Uxth => 0xB280,
        Mnemonic::Uxtb => 0xB2C0,
        _ => return Err(AsmError::new(line, "unsupported misc instruction")),
    };
    let hw = opcode | ((rm.value() as u16) << 3) | (rd.value() as u16);
    Ok(emit16(hw))
}

// ---------------------------------------------------------------------------
// Narrow 16-bit: CBZ/CBNZ, IT, LDM/STM
// ---------------------------------------------------------------------------

fn encode_cbz_cbnz(
    inst: &Instruction,
    offset: u32,
    symbols: &HashMap<String, (usize, u32)>,
    equs: &HashMap<String, i64>,
) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let (rn, label) = match inst.operands.as_slice() {
        [Operand::Reg(rn), Operand::Label(name)] => (*rn, name),
        _ => return Err(AsmError::new(line, "CBZ/CBNZ requires register and label")),
    };
    if rn.value() > 7 {
        return Err(AsmError::new(line, "CBZ/CBNZ: register must be R0-R7"));
    }
    let target = resolve_label(label, symbols, equs, line)?;
    let pc = offset + 4;
    let disp = target.wrapping_sub(pc);
    if disp > 126 || disp % 2 != 0 {
        return Err(AsmError::new(line, "CBZ/CBNZ: target out of range (0..126, even)"));
    }
    let imm5 = (disp >> 1) & 0x1F;
    let i = (disp >> 6) & 1;
    let op = if inst.mnemonic == Mnemonic::Cbnz { 1u16 } else { 0 };
    let hw: u16 = 0xB100 | (op << 11) | ((i as u16) << 9) | ((imm5 as u16) << 3) | (rn.value() as u16);
    Ok(emit16(hw))
}

fn encode_it(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let mask = match inst.operands.as_slice() {
        [Operand::Imm(m)] => *m as u8,
        _ => return Err(AsmError::new(line, "IT: invalid operands")),
    };
    let cond = inst.condition.ok_or_else(|| AsmError::new(line, "IT: missing condition"))?;
    let firstcond = cond.raw_value().value();
    let hw: u16 = 0xBF00 | ((firstcond as u16) << 4) | (mask as u16);
    Ok(emit16(hw))
}

fn encode_ldm_narrow(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        // LDMIA SP!, {R0-R7, PC} → narrow POP
        [Operand::Reg(SP), Operand::RegList(mask)]
            if inst.writeback && (*mask & 0x7F00) == 0 =>
        {
            let pc_bit = if (*mask & 0x8000) != 0 { 0x0100u16 } else { 0 };
            let hw: u16 = 0xBC00 | pc_bit | (*mask as u16 & 0xFF);
            Ok(emit16(hw))
        }
        [Operand::Reg(rn), Operand::RegList(mask)] if rn.value() <= 7 && (*mask & 0xFF00) == 0 => {
            // LDMIA Rn!, {reglist} (narrow: writeback is implicit, Rn not in list means writeback)
            let hw: u16 = 0xC800 | ((rn.value() as u16) << 8) | (*mask as u16 & 0xFF);
            Ok(emit16(hw))
        }
        _ => Err(AsmError::new(line, "LDM narrow: need low base reg and R0-R7")),
    }
}

fn encode_stm_narrow(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::Reg(rn), Operand::RegList(mask)] if rn.value() <= 7 && (*mask & 0xFF00) == 0 => {
            let hw: u16 = 0xC000 | ((rn.value() as u16) << 8) | (*mask as u16 & 0xFF);
            Ok(emit16(hw))
        }
        _ => Err(AsmError::new(line, "STM narrow: need low base reg and R0-R7")),
    }
}

// ===========================================================================
// Thumb-2 (32-bit) encoding functions
// ===========================================================================

// ---------------------------------------------------------------------------
// Data processing (modified immediate + shifted register)
// ---------------------------------------------------------------------------

fn encode_t2_data_proc(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let m = inst.mnemonic;
    let is_test = m.implicit_s();
    let is_move = matches!(m, Mnemonic::Mov | Mnemonic::Mvn);
    let opcode = t2_dp_opcode(m);
    let s = inst.set_flags || is_test;

    match inst.operands.as_slice() {
        // NEG Rd, Rm -> RSB Rd, Rm, #0
        [Operand::Reg(rd), Operand::Reg(rm)] if m == Mnemonic::Neg => {
            Ok(t2_dp_mod_imm(opcode, s, *rm, *rd, 0))
        }
        // MOV/MVN Rd, #imm
        [Operand::Reg(rd), Operand::Imm(imm)] if is_move => {
            let imm12 = thumb_expand_imm_encoding(*imm as u32)
                .ok_or_else(|| AsmError::new(line, format!("immediate {imm} not encodable as Thumb modified immediate")))?;
            Ok(t2_dp_mod_imm(opcode, s, PC, *rd, imm12))
        }
        // MOV/MVN Rd, Rm [, shift]
        [Operand::Reg(rd), Operand::Reg(rm)] if is_move => {
            Ok(t2_dp_shift_reg(opcode, s, PC, *rd, *rm, 0, 0))
        }
        [Operand::Reg(rd), Operand::Shifted(rm, st, amount)] if is_move => {
            let (stype, simm) = shift_encoding(*st, amount, line)?;
            Ok(t2_dp_shift_reg(opcode, s, PC, *rd, *rm, stype, simm))
        }
        // CMP/CMN/TST/TEQ Rn, #imm
        [Operand::Reg(rn), Operand::Imm(imm)] if is_test => {
            let imm12 = thumb_expand_imm_encoding(*imm as u32)
                .ok_or_else(|| AsmError::new(line, format!("immediate {imm} not encodable")))?;
            Ok(t2_dp_mod_imm(opcode, true, *rn, PC, imm12))
        }
        // CMP/CMN/TST/TEQ Rn, Rm [, shift]
        [Operand::Reg(rn), Operand::Reg(rm)] if is_test => {
            Ok(t2_dp_shift_reg(opcode, true, *rn, PC, *rm, 0, 0))
        }
        [Operand::Reg(rn), Operand::Shifted(rm, st, amount)] if is_test => {
            let (stype, simm) = shift_encoding(*st, amount, line)?;
            Ok(t2_dp_shift_reg(opcode, true, *rn, PC, *rm, stype, simm))
        }
        // Normal: OP Rd, Rn, #imm
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Imm(imm)] => {
            let imm12 = thumb_expand_imm_encoding(*imm as u32)
                .ok_or_else(|| AsmError::new(line, format!("immediate {imm} not encodable")))?;
            Ok(t2_dp_mod_imm(opcode, s, *rn, *rd, imm12))
        }
        // Normal: OP Rd, Rn, Rm [, shift]
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm)] => {
            Ok(t2_dp_shift_reg(opcode, s, *rn, *rd, *rm, 0, 0))
        }
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Shifted(rm, st, amount)] => {
            let (stype, simm) = shift_encoding(*st, amount, line)?;
            Ok(t2_dp_shift_reg(opcode, s, *rn, *rd, *rm, stype, simm))
        }
        // Two-operand: OP Rd, #imm (Rd is both dest and first source)
        [Operand::Reg(rd), Operand::Imm(imm)] if !is_test && !is_move => {
            let imm12 = thumb_expand_imm_encoding(*imm as u32)
                .ok_or_else(|| AsmError::new(line, format!("immediate {imm} not encodable")))?;
            Ok(t2_dp_mod_imm(opcode, s, *rd, *rd, imm12))
        }
        // Two-operand: OP Rd, Rm
        [Operand::Reg(rd), Operand::Reg(rm)] if !is_test && !is_move => {
            Ok(t2_dp_shift_reg(opcode, s, *rd, *rd, *rm, 0, 0))
        }
        _ => Err(AsmError::new(line, format!("invalid operands for {:?}.W", m))),
    }
}

fn shift_encoding(st: ShiftType, amount: &Operand, line: usize) -> Result<(u8, u8), AsmError> {
    let stype = st.encoding() as u8;
    let simm = match amount {
        Operand::Imm(n) => *n as u8,
        _ => return Err(AsmError::new(line, "expected immediate shift amount")),
    };
    Ok((stype, simm))
}

/// Encode Thumb-2 DP modified immediate: 11110 i 0 op S Rn | 0 imm3 Rd imm8
fn t2_dp_mod_imm(opcode: u8, s: bool, rn: u4, rd: u4, imm12: u16) -> Vec<u8> {
    let i = ((imm12 >> 11) & 1) as u16;
    let imm3 = ((imm12 >> 8) & 7) as u16;
    let imm8 = (imm12 & 0xFF) as u16;
    let hw1: u16 =
        0xF000 | (i << 10) | ((opcode as u16) << 5) | ((s as u16) << 4) | (rn.value() as u16);
    let hw2: u16 = (imm3 << 12) | ((rd.value() as u16) << 8) | imm8;
    emit32_thumb(hw1, hw2)
}

/// Encode Thumb-2 DP shifted register: 1110101 op S Rn | 0 imm3 Rd imm2 type Rm
fn t2_dp_shift_reg(opcode: u8, s: bool, rn: u4, rd: u4, rm: u4, stype: u8, simm: u8) -> Vec<u8> {
    let imm3 = ((simm >> 2) & 7) as u16;
    let imm2 = (simm & 3) as u16;
    let hw1: u16 =
        0xEA00 | ((opcode as u16) << 5) | ((s as u16) << 4) | (rn.value() as u16);
    let hw2: u16 = (imm3 << 12)
        | ((rd.value() as u16) << 8)
        | (imm2 << 6)
        | ((stype as u16 & 3) << 4)
        | (rm.value() as u16);
    emit32_thumb(hw1, hw2)
}

// ---------------------------------------------------------------------------
// MOVW / MOVT (16-bit immediate)
// ---------------------------------------------------------------------------

fn encode_t2_movw_movt(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let (rd, imm) = match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Imm(imm)] => (*rd, *imm as u32),
        _ => return Err(AsmError::new(line, "MOVW/MOVT: expected Rd, #imm16")),
    };
    if imm > 0xFFFF {
        return Err(AsmError::new(line, "MOVW/MOVT: immediate must be 0..65535"));
    }
    let imm4 = (imm >> 12) & 0xF;
    let i = (imm >> 11) & 1;
    let imm3 = (imm >> 8) & 7;
    let imm8 = imm & 0xFF;
    let base = if inst.mnemonic == Mnemonic::Movw { 0xF240u16 } else { 0xF2C0u16 };
    let hw1: u16 = base | ((i as u16) << 10) | imm4 as u16;
    let hw2: u16 = ((imm3 as u16) << 12) | ((rd.value() as u16 & 0xF) << 8) | imm8 as u16;
    Ok(emit32_thumb(hw1, hw2))
}

// ---------------------------------------------------------------------------
// Shifts (wide)
// ---------------------------------------------------------------------------

fn encode_t2_shift(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let stype: u8 = match inst.mnemonic {
        Mnemonic::Lsl => 0b00,
        Mnemonic::Lsr => 0b01,
        Mnemonic::Asr => 0b10,
        Mnemonic::Ror => 0b11,
        Mnemonic::Rrx => {
            // RRX Rd, Rm → MOV.W Rd, Rm, RRX
            let (rd, rm) = match inst.operands.as_slice() {
                [Operand::Reg(rd), Operand::Reg(rm)] => (*rd, *rm),
                _ => return Err(AsmError::new(line, "RRX: expected Rd, Rm")),
            };
            return Ok(t2_dp_shift_reg(0b0010, inst.set_flags, PC, rd, rm, 0b11, 0));
        }
        _ => unreachable!(),
    };

    match inst.operands.as_slice() {
        // LSL.W Rd, Rm, #imm → MOV.W Rd, Rm, LSL #imm
        [Operand::Reg(rd), Operand::Reg(rm), Operand::Imm(imm)] => {
            Ok(t2_dp_shift_reg(0b0010, inst.set_flags, PC, *rd, *rm, stype, *imm as u8))
        }
        // LSL.W Rd, Rn, Rs (register shift)
        // Encoding: 11111010 0 type S Rn | 1111 Rd 0000 Rm
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rs)] => {
            let s = inst.set_flags;
            let hw1: u16 = 0xFA00 | ((stype as u16) << 5) | ((s as u16) << 4) | (rn.value() as u16 & 0xF);
            let hw2: u16 = 0xF000 | ((rd.value() as u16 & 0xF) << 8) | (rs.value() as u16 & 0xF);
            Ok(emit32_thumb(hw1, hw2))
        }
        _ => Err(AsmError::new(line, "invalid operands for wide shift")),
    }
}

// ---------------------------------------------------------------------------
// Multiply / divide (wide)
// ---------------------------------------------------------------------------

fn encode_t2_mul(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        // MUL Rd, Rn, Rm: 11111011 0000 Rn | 1111 Rd 0000 Rm
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm)] if inst.mnemonic == Mnemonic::Mul => {
            let hw1: u16 = 0xFB00 | (rn.value() as u16 & 0xF);
            let hw2: u16 = 0xF000 | ((rd.value() as u16 & 0xF) << 8) | (rm.value() as u16 & 0xF);
            Ok(emit32_thumb(hw1, hw2))
        }
        // MLA Rd, Rn, Rm, Ra: 11111011 0000 Rn | Ra Rd 0000 Rm
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm), Operand::Reg(ra)]
            if inst.mnemonic == Mnemonic::Mla =>
        {
            let hw1: u16 = 0xFB00 | (rn.value() as u16 & 0xF);
            let hw2: u16 =
                ((ra.value() as u16 & 0xF) << 12) | ((rd.value() as u16 & 0xF) << 8) | (rm.value() as u16 & 0xF);
            Ok(emit32_thumb(hw1, hw2))
        }
        // MLS Rd, Rn, Rm, Ra: 11111011 0000 Rn | Ra Rd 0001 Rm
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm), Operand::Reg(ra)]
            if inst.mnemonic == Mnemonic::Mls =>
        {
            let hw1: u16 = 0xFB00 | (rn.value() as u16 & 0xF);
            let hw2: u16 =
                ((ra.value() as u16 & 0xF) << 12) | ((rd.value() as u16 & 0xF) << 8) | 0x0010 | (rm.value() as u16 & 0xF);
            Ok(emit32_thumb(hw1, hw2))
        }
        _ => Err(AsmError::new(line, format!("invalid operands for {:?}", inst.mnemonic))),
    }
}

fn encode_t2_long_mul(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    // UMULL/SMULL/UMLAL/SMLAL RdLo, RdHi, Rn, Rm
    let (rdlo, rdhi, rn, rm) = match inst.operands.as_slice() {
        [Operand::Reg(rdlo), Operand::Reg(rdhi), Operand::Reg(rn), Operand::Reg(rm)] => {
            (*rdlo, *rdhi, *rn, *rm)
        }
        _ => return Err(AsmError::new(line, "long multiply: need RdLo, RdHi, Rn, Rm")),
    };
    let op = match inst.mnemonic {
        Mnemonic::Smull => 0xFB80u16,
        Mnemonic::Umull => 0xFBA0u16,
        Mnemonic::Smlal => 0xFBC0u16,
        Mnemonic::Umlal => 0xFBE0u16,
        _ => unreachable!(),
    };
    let hw1: u16 = op | (rn.value() as u16 & 0xF);
    let hw2: u16 = ((rdlo.value() as u16 & 0xF) << 12) | ((rdhi.value() as u16 & 0xF) << 8) | (rm.value() as u16 & 0xF);
    Ok(emit32_thumb(hw1, hw2))
}

fn encode_t2_div(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let (rd, rn, rm) = match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm)] => (*rd, *rn, *rm),
        _ => return Err(AsmError::new(line, "SDIV/UDIV: need Rd, Rn, Rm")),
    };
    let op = if inst.mnemonic == Mnemonic::Sdiv { 0xFB90u16 } else { 0xFBB0u16 };
    let hw1: u16 = op | (rn.value() as u16 & 0xF);
    let hw2: u16 = 0xF0F0 | ((rd.value() as u16 & 0xF) << 8) | (rm.value() as u16 & 0xF);
    Ok(emit32_thumb(hw1, hw2))
}

// ---------------------------------------------------------------------------
// Load / Store (wide)
// ---------------------------------------------------------------------------

fn t2_ldr_str_opcode(m: Mnemonic, load: bool) -> (u16, u16, bool) {
    // Returns (T3 hw1 base, T4 hw1 base, is_signed)
    // T3 = positive imm12, T4 = imm8 with P/U/W
    match (m, load) {
        (Mnemonic::Ldr, true) => (0xF8D0, 0xF850, false),
        (Mnemonic::Str, false) => (0xF8C0, 0xF840, false),
        (Mnemonic::Ldrb, true) => (0xF890, 0xF810, false),
        (Mnemonic::Strb, false) => (0xF880, 0xF800, false),
        (Mnemonic::Ldrh, true) => (0xF8B0, 0xF830, false),
        (Mnemonic::Strh, false) => (0xF8A0, 0xF820, false),
        (Mnemonic::Ldrsb, true) => (0xF990, 0xF910, true),
        (Mnemonic::Ldrsh, true) => (0xF9B0, 0xF930, true),
        _ => unreachable!(),
    }
}

fn encode_t2_ldr_str(
    inst: &Instruction,
    offset: u32,
    symbols: &HashMap<String, (usize, u32)>,
    equs: &HashMap<String, i64>,
) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let m = inst.mnemonic;
    let is_load = matches!(m, Mnemonic::Ldr | Mnemonic::Ldrb | Mnemonic::Ldrh | Mnemonic::Ldrsb | Mnemonic::Ldrsh);
    let (t3_base, t4_base, _) = t2_ldr_str_opcode(m, is_load);

    match inst.operands.as_slice() {
        // LDR.W Rt, [Rn, #imm]
        [Operand::Reg(rt), Operand::Memory { base, offset: MemOffset::Imm(imm), pre_index, writeback }] => {
            let rn = *base;
            let imm_val = *imm;

            if *pre_index && !*writeback && imm_val >= 0 && imm_val <= 4095 {
                // T3 encoding: positive offset, imm12
                let hw1: u16 = t3_base | (rn.value() as u16 & 0xF);
                let hw2: u16 = ((rt.value() as u16 & 0xF) << 12) | (imm_val as u16 & 0xFFF);
                Ok(emit32_thumb(hw1, hw2))
            } else {
                // T4 encoding: imm8 with P/U/W bits
                let (add, abs_imm) = if imm_val >= 0 {
                    (true, imm_val as u32)
                } else {
                    (false, (-imm_val) as u32)
                };
                if abs_imm > 255 {
                    return Err(AsmError::new(line, "wide LDR/STR T4: offset must be -255..255"));
                }
                let p = *pre_index as u16;
                let u = add as u16;
                let w = *writeback as u16;
                let hw1: u16 = t4_base | (rn.value() as u16 & 0xF);
                let hw2: u16 = ((rt.value() as u16 & 0xF) << 12)
                    | 0x0800
                    | (p << 10)
                    | (u << 9)
                    | (w << 8)
                    | (abs_imm as u16 & 0xFF);
                Ok(emit32_thumb(hw1, hw2))
            }
        }
        // LDR.W Rt, [Rn, Rm, LSL #shift]
        [Operand::Reg(rt), Operand::Memory { base, offset: MemOffset::Reg(rm, _), .. }] => {
            let hw1: u16 = t4_base | (base.value() as u16 & 0xF);
            let hw2: u16 = ((rt.value() as u16 & 0xF) << 12) | (rm.value() as u16 & 0xF);
            Ok(emit32_thumb(hw1, hw2))
        }
        [Operand::Reg(rt), Operand::Memory { base, offset: MemOffset::RegShift(rm, ShiftType::Lsl, shift, _), .. }] => {
            let hw1: u16 = t4_base | (base.value() as u16 & 0xF);
            let hw2: u16 = ((rt.value() as u16 & 0xF) << 12) | ((*shift as u16 & 3) << 4) | (rm.value() as u16 & 0xF);
            Ok(emit32_thumb(hw1, hw2))
        }
        // LDR Rt, label (PC-relative, wide)
        [Operand::Reg(rt), Operand::Label(name)] if is_load => {
            let target = resolve_label(name, symbols, equs, line)?;
            let pc = (offset + 4) & !3;
            let disp = target as i32 - pc as i32;
            let (u, abs_disp) = if disp >= 0 { (1u16, disp as u32) } else { (0u16, (-disp) as u32) };
            if abs_disp > 4095 {
                return Err(AsmError::new(line, "LDR PC-relative: offset out of range"));
            }
            // LDR (literal) T2: 11111000 U1011111 Rt imm12
            let hw1: u16 = 0xF85F | (u << 7);
            let hw2: u16 = ((rt.value() as u16 & 0xF) << 12) | (abs_disp as u16 & 0xFFF);
            Ok(emit32_thumb(hw1, hw2))
        }
        _ => Err(AsmError::new(line, format!("invalid operands for {:?}.W", m))),
    }
}

// ---------------------------------------------------------------------------
// LDRD / STRD
// ---------------------------------------------------------------------------

fn encode_t2_ldrd_strd(
    inst: &Instruction,
    offset: u32,
    symbols: &HashMap<String, (usize, u32)>,
    equs: &HashMap<String, i64>,
) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let load = inst.mnemonic == Mnemonic::Ldrd;

    match inst.operands.as_slice() {
        [Operand::Reg(rt), Operand::Reg(rt2), Operand::Memory { base, offset: MemOffset::Imm(imm), pre_index, writeback }] =>
        {
            let (add, abs_imm) = if *imm >= 0 { (true, *imm as u32) } else { (false, (-*imm) as u32) };
            if abs_imm % 4 != 0 || abs_imm > 1020 {
                return Err(AsmError::new(line, "LDRD/STRD: offset must be word-aligned, max ±1020"));
            }
            let imm8 = (abs_imm / 4) as u16;
            let p = *pre_index as u16;
            let u = add as u16;
            let w = *writeback as u16;
            let l = load as u16;
            // 1110100 PU1WL Rn
            let hw1: u16 = 0xE800 | (p << 8) | (u << 7) | (1 << 6) | (w << 5) | (l << 4) | (base.value() as u16 & 0xF);
            let hw2: u16 = ((rt.value() as u16 & 0xF) << 12) | ((rt2.value() as u16 & 0xF) << 8) | imm8;
            Ok(emit32_thumb(hw1, hw2))
        }
        // LDRD Rt, Rt2, label (PC-relative)
        [Operand::Reg(rt), Operand::Reg(rt2), Operand::Label(name)] if load => {
            let target = resolve_label(name, symbols, equs, line)?;
            let pc = (offset + 4) & !3;
            let disp = target as i32 - pc as i32;
            let (u, abs_disp) = if disp >= 0 { (1u16, disp as u32) } else { (0u16, (-disp) as u32) };
            if abs_disp % 4 != 0 || abs_disp > 1020 {
                return Err(AsmError::new(line, "LDRD literal: offset out of range"));
            }
            let imm8 = (abs_disp / 4) as u16;
            // LDRD (literal): 1110100 PU1W1 1111
            let hw1: u16 = 0xE95F | (u << 7);
            let hw2: u16 = ((rt.value() as u16 & 0xF) << 12) | ((rt2.value() as u16 & 0xF) << 8) | imm8;
            Ok(emit32_thumb(hw1, hw2))
        }
        _ => Err(AsmError::new(line, "invalid operands for LDRD/STRD")),
    }
}

// ---------------------------------------------------------------------------
// LDM / STM (wide)
// ---------------------------------------------------------------------------

fn encode_t2_ldm(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let db = matches!(inst.mnemonic, Mnemonic::Ldmdb);
    let (rn, mask) = match inst.operands.as_slice() {
        [Operand::Reg(rn), Operand::RegList(mask)] => (*rn, *mask),
        _ => return Err(AsmError::new(line, "LDM: need Rn, {reglist}")),
    };
    let w = inst.writeback as u16;
    let base = if db { 0xE910u16 } else { 0xE890u16 };
    let hw1: u16 = base | (w << 5) | (rn.value() as u16 & 0xF);
    let hw2: u16 = mask;
    Ok(emit32_thumb(hw1, hw2))
}

fn encode_t2_stm(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let db = matches!(inst.mnemonic, Mnemonic::Stmdb | Mnemonic::Stmfd);
    let (rn, mask) = match inst.operands.as_slice() {
        [Operand::Reg(rn), Operand::RegList(mask)] => (*rn, *mask),
        _ => return Err(AsmError::new(line, "STM: need Rn, {reglist}")),
    };
    let w = inst.writeback as u16;
    let base = if db { 0xE900u16 } else { 0xE880u16 };
    let hw1: u16 = base | (w << 5) | (rn.value() as u16 & 0xF);
    let hw2: u16 = mask;
    Ok(emit32_thumb(hw1, hw2))
}

fn encode_t2_push(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::RegList(mask)] => {
            // PUSH = STMDB SP!, {reglist}
            let hw1: u16 = 0xE92D;
            let hw2: u16 = *mask;
            Ok(emit32_thumb(hw1, hw2))
        }
        _ => Err(AsmError::new(line, "PUSH requires register list")),
    }
}

fn encode_t2_pop(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        [Operand::RegList(mask)] => {
            // POP = LDMIA SP!, {reglist}
            let hw1: u16 = 0xE8BD;
            let hw2: u16 = *mask;
            Ok(emit32_thumb(hw1, hw2))
        }
        _ => Err(AsmError::new(line, "POP requires register list")),
    }
}

// ---------------------------------------------------------------------------
// Branch (wide)
// ---------------------------------------------------------------------------

fn encode_t2_branch(
    inst: &Instruction,
    offset: u32,
    symbols: &HashMap<String, (usize, u32)>,
    equs: &HashMap<String, i64>,
) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let target_label = match inst.operands.as_slice() {
        [Operand::Label(name)] => name,
        _ => return Err(AsmError::new(line, "B.W requires a label")),
    };
    let target = resolve_label(target_label, symbols, equs, line)?;
    let pc = offset + 4;
    let disp = target as i32 - pc as i32;
    if disp % 2 != 0 {
        return Err(AsmError::new(line, "branch target not halfword-aligned"));
    }

    if let Some(cond) = inst.condition {
        // Conditional B.W: ±1MB range (20-bit signed offset)
        let imm = disp >> 1;
        if imm < -(1 << 19) || imm >= (1 << 19) {
            return Err(AsmError::new(line, "conditional B.W: target out of range (±1MB)"));
        }
        let offset_u = disp as u32;
        let s = (offset_u >> 20) & 1;
        let imm6 = (offset_u >> 12) & 0x3F;
        let imm11 = (offset_u >> 1) & 0x7FF;
        let j1 = (offset_u >> 19) & 1;
        let j2 = (offset_u >> 18) & 1;
        let cond_val = cond.raw_value().value() as u16;
        let hw1: u16 = 0xF000 | ((s as u16) << 10) | (cond_val << 6) | (imm6 as u16);
        let hw2: u16 = 0x8000 | ((j1 as u16) << 13) | ((j2 as u16) << 11) | (imm11 as u16);
        Ok(emit32_thumb(hw1, hw2))
    } else {
        // Unconditional B.W: ±16MB range (24-bit signed offset)
        let imm = disp >> 1;
        if imm < -(1 << 23) || imm >= (1 << 23) {
            return Err(AsmError::new(line, "B.W: target out of range (±16MB)"));
        }
        let offset_u = disp as u32;
        let s = (offset_u >> 24) & 1;
        let imm10 = (offset_u >> 12) & 0x3FF;
        let imm11 = (offset_u >> 1) & 0x7FF;
        let i1 = (offset_u >> 23) & 1;
        let i2 = (offset_u >> 22) & 1;
        let j1 = (!(i1 ^ s)) & 1;
        let j2 = (!(i2 ^ s)) & 1;
        let hw1: u16 = 0xF000 | ((s as u16) << 10) | (imm10 as u16);
        let hw2: u16 = 0x9000 | ((j1 as u16) << 13) | ((j2 as u16) << 11) | (imm11 as u16);
        Ok(emit32_thumb(hw1, hw2))
    }
}

// ---------------------------------------------------------------------------
// Bit manipulation
// ---------------------------------------------------------------------------

fn encode_t2_clz_rbit(inst: &Instruction, op1: u8, op2: u8) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let (rd, rm) = match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rm)] => (*rd, *rm),
        _ => return Err(AsmError::new(line, "CLZ/RBIT: need Rd, Rm")),
    };
    // 11111010 1 op1 Rm | 1111 Rd 1 op2 Rm
    let hw1: u16 = 0xFA80 | ((op1 as u16 & 0xF) << 4) | (rm.value() as u16 & 0xF);
    let hw2: u16 = 0xF080 | ((rd.value() as u16 & 0xF) << 8) | ((op2 as u16 & 0xF) << 4) | (rm.value() as u16 & 0xF);
    Ok(emit32_thumb(hw1, hw2))
}

fn encode_t2_bfi(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    // BFI Rd, Rn, #lsb, #width
    let (rd, rn, lsb, width) = match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Imm(lsb), Operand::Imm(width)] => {
            (*rd, *rn, *lsb as u8, *width as u8)
        }
        _ => return Err(AsmError::new(line, "BFI: need Rd, Rn, #lsb, #width")),
    };
    let msb = lsb + width - 1;
    let imm3 = (lsb >> 2) & 7;
    let imm2 = lsb & 3;
    let hw1: u16 = 0xF360 | (rn.value() as u16 & 0xF);
    let hw2: u16 = ((imm3 as u16) << 12) | ((rd.value() as u16 & 0xF) << 8) | ((imm2 as u16) << 6) | (msb as u16 & 0x1F);
    Ok(emit32_thumb(hw1, hw2))
}

fn encode_t2_bfc(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    // BFC Rd, #lsb, #width
    let (rd, lsb, width) = match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Imm(lsb), Operand::Imm(width)] => (*rd, *lsb as u8, *width as u8),
        _ => return Err(AsmError::new(line, "BFC: need Rd, #lsb, #width")),
    };
    let msb = lsb + width - 1;
    let imm3 = (lsb >> 2) & 7;
    let imm2 = lsb & 3;
    let hw1: u16 = 0xF36F; // Rn = 1111 for BFC
    let hw2: u16 = ((imm3 as u16) << 12) | ((rd.value() as u16 & 0xF) << 8) | ((imm2 as u16) << 6) | (msb as u16 & 0x1F);
    Ok(emit32_thumb(hw1, hw2))
}

fn encode_t2_bfx(inst: &Instruction, signed: bool) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    // UBFX/SBFX Rd, Rn, #lsb, #width
    let (rd, rn, lsb, width) = match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Imm(lsb), Operand::Imm(width)] => {
            (*rd, *rn, *lsb as u8, *width as u8)
        }
        _ => return Err(AsmError::new(line, "UBFX/SBFX: need Rd, Rn, #lsb, #width")),
    };
    let widthm1 = width - 1;
    let imm3 = (lsb >> 2) & 7;
    let imm2 = lsb & 3;
    let op = if signed { 0xF340u16 } else { 0xF3C0u16 };
    let hw1: u16 = op | (rn.value() as u16 & 0xF);
    let hw2: u16 = ((imm3 as u16) << 12) | ((rd.value() as u16 & 0xF) << 8) | ((imm2 as u16) << 6) | (widthm1 as u16 & 0x1F);
    Ok(emit32_thumb(hw1, hw2))
}

// ---------------------------------------------------------------------------
// Exclusive load / store
// ---------------------------------------------------------------------------

fn encode_t2_ldrex(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let (rt, rn, imm) = match inst.operands.as_slice() {
        [Operand::Reg(rt), Operand::Memory { base, offset: MemOffset::Imm(imm), .. }] => (*rt, *base, *imm),
        _ => return Err(AsmError::new(line, "LDREX: need Rt, [Rn, #imm]")),
    };
    if imm < 0 || imm % 4 != 0 || imm > 1020 {
        return Err(AsmError::new(line, "LDREX: offset must be 0..1020, word-aligned"));
    }
    let hw1: u16 = 0xE850 | (rn.value() as u16 & 0xF);
    let hw2: u16 = ((rt.value() as u16 & 0xF) << 12) | 0x0F00 | ((imm as u16 / 4) & 0xFF);
    Ok(emit32_thumb(hw1, hw2))
}

fn encode_t2_strex(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let (rd, rt, rn, imm) = match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rt), Operand::Memory { base, offset: MemOffset::Imm(imm), .. }] => {
            (*rd, *rt, *base, *imm)
        }
        _ => return Err(AsmError::new(line, "STREX: need Rd, Rt, [Rn, #imm]")),
    };
    if imm < 0 || imm % 4 != 0 || imm > 1020 {
        return Err(AsmError::new(line, "STREX: offset must be 0..1020, word-aligned"));
    }
    let hw1: u16 = 0xE840 | (rn.value() as u16 & 0xF);
    let hw2: u16 = ((rt.value() as u16 & 0xF) << 12) | ((rd.value() as u16 & 0xF) << 8) | ((imm as u16 / 4) & 0xFF);
    Ok(emit32_thumb(hw1, hw2))
}

fn encode_t2_ldrex_bh(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let (rt, rn) = match inst.operands.as_slice() {
        [Operand::Reg(rt), Operand::Memory { base, offset: MemOffset::Imm(0), .. }] => (*rt, *base),
        [Operand::Reg(rt), Operand::Memory { base, .. }] => (*rt, *base),
        _ => return Err(AsmError::new(line, "LDREXB/H: need Rt, [Rn]")),
    };
    // LDREXB: hw2 = Rt:1111:0100:1111 = (Rt<<12)|0xF4F
    // LDREXH: hw2 = Rt:1111:0101:1111 = (Rt<<12)|0xF5F
    let suffix = if inst.mnemonic == Mnemonic::Ldrexb { 0x0F4F_u16 } else { 0x0F5F };
    let hw1: u16 = 0xE8D0 | (rn.value() as u16 & 0xF);
    let hw2: u16 = ((rt.value() as u16 & 0xF) << 12) | suffix;
    Ok(emit32_thumb(hw1, hw2))
}

fn encode_t2_strex_bh(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let (rd, rt, rn) = match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rt), Operand::Memory { base, offset: MemOffset::Imm(0), .. }] => (*rd, *rt, *base),
        [Operand::Reg(rd), Operand::Reg(rt), Operand::Memory { base, .. }] => (*rd, *rt, *base),
        _ => return Err(AsmError::new(line, "STREXB/H: need Rd, Rt, [Rn]")),
    };
    // STREXB: hw2 = Rt:1111:0100:Rd = (Rt<<12)|0xF40|Rd
    // STREXH: hw2 = Rt:1111:0101:Rd = (Rt<<12)|0xF50|Rd
    let suffix = if inst.mnemonic == Mnemonic::Strexb { 0x0F40_u16 } else { 0x0F50 };
    let hw1: u16 = 0xE8C0 | (rn.value() as u16 & 0xF);
    let hw2: u16 = ((rt.value() as u16 & 0xF) << 12) | suffix | (rd.value() as u16 & 0xF);
    Ok(emit32_thumb(hw1, hw2))
}

// ---------------------------------------------------------------------------
// System: MRS, MSR
// ---------------------------------------------------------------------------

fn encode_t2_mrs(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let (rd, sysm) = match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::SysReg(sysm)] => (*rd, *sysm as u16),
        [Operand::Reg(rd), Operand::Imm(sysm)] => (*rd, *sysm as u16),
        _ => return Err(AsmError::new(line, "MRS: need Rd, sysreg")),
    };
    let hw1: u16 = 0xF3EF;
    let hw2: u16 = 0x8000 | ((rd.value() as u16 & 0xF) << 8) | (sysm & 0xFF);
    Ok(emit32_thumb(hw1, hw2))
}

fn encode_t2_msr(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let (sysm, rn) = match inst.operands.as_slice() {
        [Operand::SysReg(sysm), Operand::Reg(rn)] => (*sysm as u16, *rn),
        [Operand::Imm(sysm), Operand::Reg(rn)] => (*sysm as u16, *rn),
        _ => return Err(AsmError::new(line, "MSR: need sysreg, Rn")),
    };
    let hw1: u16 = 0xF380 | (rn.value() as u16 & 0xF);
    let hw2: u16 = 0x8800 | (sysm & 0xFF);
    Ok(emit32_thumb(hw1, hw2))
}

// ---------------------------------------------------------------------------
// Table branch
// ---------------------------------------------------------------------------

fn encode_t2_tb(inst: &Instruction, half: bool) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let (rn, rm) = match inst.operands.as_slice() {
        [Operand::Memory { base, offset: MemOffset::Reg(rm, _), .. }] => (*base, *rm),
        [Operand::Memory { base, offset: MemOffset::RegShift(rm, _, _, _), .. }] => (*base, *rm),
        _ => return Err(AsmError::new(line, "TBB/TBH: need [Rn, Rm]")),
    };
    let h = half as u16;
    let hw1: u16 = 0xE8D0 | (rn.value() as u16 & 0xF);
    let hw2: u16 = 0xF000 | (h << 4) | (rm.value() as u16 & 0xF);
    Ok(emit32_thumb(hw1, hw2))
}

// ---------------------------------------------------------------------------
// Saturation: SSAT, USAT
// ---------------------------------------------------------------------------

fn encode_t2_ssat(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let (rd, sat, rn, shift_type, shift_amt) = parse_sat_operands(inst)?;
    let _ = line;
    let sh = if shift_type == 1 { 1u16 } else { 0u16 }; // ASR=1, LSL=0
    let imm3 = ((shift_amt >> 2) & 7) as u16;
    let imm2 = (shift_amt & 3) as u16;
    let sat_imm = (sat - 1) as u16;
    let hw1: u16 = 0xF300 | (sh << 5) | (rn.value() as u16 & 0xF);
    let hw2: u16 = (imm3 << 12) | ((rd.value() as u16 & 0xF) << 8) | (imm2 << 6) | (sat_imm & 0x1F);
    Ok(emit32_thumb(hw1, hw2))
}

fn encode_t2_usat(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let (rd, sat, rn, shift_type, shift_amt) = parse_sat_operands(inst)?;
    let _ = line;
    let sh = if shift_type == 1 { 1u16 } else { 0u16 };
    let imm3 = ((shift_amt >> 2) & 7) as u16;
    let imm2 = (shift_amt & 3) as u16;
    let sat_imm = sat as u16;
    let hw1: u16 = 0xF380 | (sh << 5) | (rn.value() as u16 & 0xF);
    let hw2: u16 = (imm3 << 12) | ((rd.value() as u16 & 0xF) << 8) | (imm2 << 6) | (sat_imm & 0x1F);
    Ok(emit32_thumb(hw1, hw2))
}

fn parse_sat_operands(inst: &Instruction) -> Result<(u4, u8, u4, u8, u8), AsmError> {
    let line = inst.line;
    match inst.operands.as_slice() {
        // SSAT/USAT Rd, #sat, Rn
        [Operand::Reg(rd), Operand::Imm(sat), Operand::Reg(rn)] => {
            Ok((*rd, *sat as u8, *rn, 0, 0))
        }
        // SSAT/USAT Rd, #sat, Rn, LSL/ASR #shift
        [Operand::Reg(rd), Operand::Imm(sat), Operand::Shifted(rn, st, amount)] => {
            let shift_type = match st {
                ShiftType::Lsl => 0u8,
                ShiftType::Asr => 1u8,
                _ => return Err(AsmError::new(line, "SSAT/USAT: only LSL/ASR shift")),
            };
            let shift_amt = match amount.as_ref() {
                Operand::Imm(n) => *n as u8,
                _ => return Err(AsmError::new(line, "expected immediate shift")),
            };
            Ok((*rd, *sat as u8, *rn, shift_type, shift_amt))
        }
        _ => Err(AsmError::new(line, "SSAT/USAT: invalid operands")),
    }
}

// ---------------------------------------------------------------------------
// Byte reversal / extend (wide forms)
// ---------------------------------------------------------------------------

fn encode_t2_extend(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let (rd, rm) = match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rm)] => (*rd, *rm),
        _ => return Err(AsmError::new(line, "expected Rd, Rm")),
    };
    let (hw1, hw2_base): (u16, u16) = match inst.mnemonic {
        Mnemonic::Sxth => (0xFA0F, 0xF080),    // SXTH: Rn=1111
        Mnemonic::Sxtb => (0xFA4F, 0xF080),
        Mnemonic::Uxth => (0xFA1F, 0xF080),
        Mnemonic::Uxtb => (0xFA5F, 0xF080),
        Mnemonic::Sxtb16 => (0xFA2F, 0xF080),
        Mnemonic::Uxtb16 => (0xFA3F, 0xF080),
        Mnemonic::Rev => (0xFA90 | (rm.value() as u16 & 0xF), 0xF080),
        Mnemonic::Rev16 => (0xFA90 | (rm.value() as u16 & 0xF), 0xF090),
        Mnemonic::Revsh => (0xFA90 | (rm.value() as u16 & 0xF), 0xF0B0),
        _ => unreachable!(),
    };
    let hw2: u16 = hw2_base | ((rd.value() as u16 & 0xF) << 8) | (rm.value() as u16 & 0xF);
    Ok(emit32_thumb(hw1, hw2))
}

// ---------------------------------------------------------------------------
// Extend and add
// ---------------------------------------------------------------------------

fn encode_t2_extend_add(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    // SXTAB Rd, Rn, Rm [, ROR #rot]
    let (rd, rn, rm, rot) = match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm)] => (*rd, *rn, *rm, 0u8),
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Shifted(rm, ShiftType::Ror, amt)] => {
            let r = match amt.as_ref() {
                Operand::Imm(n) => (*n as u8) / 8,
                _ => return Err(AsmError::new(line, "expected ROR #8/16/24")),
            };
            (*rd, *rn, *rm, r)
        }
        _ => return Err(AsmError::new(line, "extend-add: need Rd, Rn, Rm")),
    };
    let op: u16 = match inst.mnemonic {
        Mnemonic::Sxtab => 0xFA40,
        Mnemonic::Sxtah => 0xFA00,
        Mnemonic::Uxtab => 0xFA50,
        Mnemonic::Uxtah => 0xFA10,
        Mnemonic::Sxtab16 => 0xFA20,
        Mnemonic::Uxtab16 => 0xFA30,
        _ => unreachable!(),
    };
    let hw1: u16 = op | (rn.value() as u16 & 0xF);
    let hw2: u16 = 0xF080 | ((rd.value() as u16 & 0xF) << 8) | ((rot as u16 & 3) << 4) | (rm.value() as u16 & 0xF);
    Ok(emit32_thumb(hw1, hw2))
}

// ---------------------------------------------------------------------------
// DSP multiply
// ---------------------------------------------------------------------------

fn encode_t2_dsp_mul(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    // Most DSP multiply: Rd, Rn, Rm [, Ra]
    match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm)] => {
            let (hw1_base, hw2_op) = dsp_mul_opcode_3(inst.mnemonic, line)?;
            let hw1: u16 = hw1_base | (rn.value() as u16 & 0xF);
            let hw2: u16 = 0xF000 | ((rd.value() as u16 & 0xF) << 8) | hw2_op | (rm.value() as u16 & 0xF);
            Ok(emit32_thumb(hw1, hw2))
        }
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm), Operand::Reg(ra)] => {
            let (hw1_base, hw2_op) = dsp_mul_opcode_4(inst.mnemonic, line)?;
            let hw1: u16 = hw1_base | (rn.value() as u16 & 0xF);
            let hw2: u16 = ((ra.value() as u16 & 0xF) << 12) | ((rd.value() as u16 & 0xF) << 8) | hw2_op | (rm.value() as u16 & 0xF);
            Ok(emit32_thumb(hw1, hw2))
        }
        _ => Err(AsmError::new(line, "DSP multiply: invalid operands")),
    }
}

fn dsp_mul_opcode_3(m: Mnemonic, line: usize) -> Result<(u16, u16), AsmError> {
    Ok(match m {
        Mnemonic::Smmul => (0xFB50, 0x0000), // Ra=1111
        Mnemonic::Smuad => (0xFB20, 0x0000),
        Mnemonic::Smusd => (0xFB40, 0x0000),
        Mnemonic::Usad8 => (0xFB70, 0x0000),
        Mnemonic::Smulbb => (0xFB10, 0x0000),
        Mnemonic::Smulbt => (0xFB10, 0x0010),
        Mnemonic::Smultb => (0xFB10, 0x0020),
        Mnemonic::Smultt => (0xFB10, 0x0030),
        _ => return Err(AsmError::new(line, "unexpected DSP mul mnemonic")),
    })
}

fn dsp_mul_opcode_4(m: Mnemonic, line: usize) -> Result<(u16, u16), AsmError> {
    Ok(match m {
        Mnemonic::Smmla => (0xFB50, 0x0000),
        Mnemonic::Smmls => (0xFB60, 0x0000),
        Mnemonic::Smlad => (0xFB20, 0x0000),
        Mnemonic::Smlsd => (0xFB40, 0x0000),
        Mnemonic::Usada8 => (0xFB70, 0x0000),
        Mnemonic::Smlabb => (0xFB10, 0x0000),
        Mnemonic::Smlabt => (0xFB10, 0x0010),
        Mnemonic::Smlatb => (0xFB10, 0x0020),
        Mnemonic::Smlatt => (0xFB10, 0x0030),
        _ => return Err(AsmError::new(line, "unexpected DSP mul mnemonic")),
    })
}

fn encode_t2_dsp_long_mul(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let (rdlo, rdhi, rn, rm) = match inst.operands.as_slice() {
        [Operand::Reg(rdlo), Operand::Reg(rdhi), Operand::Reg(rn), Operand::Reg(rm)] => {
            (*rdlo, *rdhi, *rn, *rm)
        }
        _ => return Err(AsmError::new(line, "DSP long mul: need RdLo, RdHi, Rn, Rm")),
    };
    let (hw1_base, hw2_op): (u16, u16) = match inst.mnemonic {
        Mnemonic::Smlalbb => (0xFBC0, 0x0080),
        Mnemonic::Smlalbt => (0xFBC0, 0x0090),
        Mnemonic::Smlaltb => (0xFBC0, 0x00A0),
        Mnemonic::Smlaltt => (0xFBC0, 0x00B0),
        Mnemonic::Smlald => (0xFBC0, 0x00C0),
        Mnemonic::Smlsld => (0xFBD0, 0x00C0),
        _ => return Err(AsmError::new(line, "unexpected DSP long mul")),
    };
    let hw1: u16 = hw1_base | (rn.value() as u16 & 0xF);
    let hw2: u16 = ((rdlo.value() as u16 & 0xF) << 12) | ((rdhi.value() as u16 & 0xF) << 8) | hw2_op | (rm.value() as u16 & 0xF);
    Ok(emit32_thumb(hw1, hw2))
}

// ---------------------------------------------------------------------------
// Parallel arithmetic
// ---------------------------------------------------------------------------

fn encode_t2_parallel(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let (rd, rn, rm) = match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm)] => (*rd, *rn, *rm),
        _ => return Err(AsmError::new(line, "parallel: need Rd, Rn, Rm")),
    };
    let (hw1_base, hw2_op) = parallel_opcode(inst.mnemonic, line)?;
    let hw1: u16 = hw1_base | (rn.value() as u16 & 0xF);
    let hw2: u16 = 0xF000 | ((rd.value() as u16 & 0xF) << 8) | hw2_op | (rm.value() as u16 & 0xF);
    Ok(emit32_thumb(hw1, hw2))
}

fn parallel_opcode(m: Mnemonic, _line: usize) -> Result<(u16, u16), AsmError> {
    Ok(match m {
        // Signed
        Mnemonic::Sadd8 => (0xFA80, 0x0000),
        Mnemonic::Sadd16 => (0xFA90, 0x0000),
        Mnemonic::Ssub8 => (0xFAC0, 0x0000),
        Mnemonic::Ssub16 => (0xFAD0, 0x0000),
        Mnemonic::Sasx => (0xFAA0, 0x0000),
        Mnemonic::Ssax => (0xFAE0, 0x0000),
        // Unsigned
        Mnemonic::Uadd8 => (0xFA80, 0x0040),
        Mnemonic::Uadd16 => (0xFA90, 0x0040),
        Mnemonic::Usub8 => (0xFAC0, 0x0040),
        Mnemonic::Usub16 => (0xFAD0, 0x0040),
        Mnemonic::Uasx => (0xFAA0, 0x0040),
        Mnemonic::Usax => (0xFAE0, 0x0040),
        // Saturating
        Mnemonic::Qadd8 => (0xFA80, 0x0010),
        Mnemonic::Qadd16 => (0xFA90, 0x0010),
        Mnemonic::Qsub8 => (0xFAC0, 0x0010),
        Mnemonic::Qsub16 => (0xFAD0, 0x0010),
        Mnemonic::Qasx => (0xFAA0, 0x0010),
        Mnemonic::Qsax => (0xFAE0, 0x0010),
        // Signed halving
        Mnemonic::Shadd8 => (0xFA80, 0x0020),
        Mnemonic::Shadd16 => (0xFA90, 0x0020),
        Mnemonic::Shsub8 => (0xFAC0, 0x0020),
        Mnemonic::Shsub16 => (0xFAD0, 0x0020),
        Mnemonic::Shasx => (0xFAA0, 0x0020),
        Mnemonic::Shsax => (0xFAE0, 0x0020),
        // Unsigned halving
        Mnemonic::Uhadd8 => (0xFA80, 0x0060),
        Mnemonic::Uhadd16 => (0xFA90, 0x0060),
        Mnemonic::Uhsub8 => (0xFAC0, 0x0060),
        Mnemonic::Uhsub16 => (0xFAD0, 0x0060),
        Mnemonic::Uhasx => (0xFAA0, 0x0060),
        Mnemonic::Uhsax => (0xFAE0, 0x0060),
        // Unsigned saturating
        Mnemonic::Uqadd8 => (0xFA80, 0x0050),
        Mnemonic::Uqadd16 => (0xFA90, 0x0050),
        Mnemonic::Uqsub8 => (0xFAC0, 0x0050),
        Mnemonic::Uqsub16 => (0xFAD0, 0x0050),
        Mnemonic::Uqasx => (0xFAA0, 0x0050),
        Mnemonic::Uqsax => (0xFAE0, 0x0050),
        _ => unreachable!(),
    })
}

// ---------------------------------------------------------------------------
// Saturating arithmetic: QADD, QDADD, QSUB, QDSUB
// ---------------------------------------------------------------------------

fn encode_t2_sat_arith(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let (rd, rm, rn) = match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rm), Operand::Reg(rn)] => (*rd, *rm, *rn),
        _ => return Err(AsmError::new(line, "QADD/QSUB: need Rd, Rm, Rn")),
    };
    let (hw1_base, hw2_op): (u16, u16) = match inst.mnemonic {
        Mnemonic::Qadd => (0xFA80, 0xF080),
        Mnemonic::Qdadd => (0xFA80, 0xF090),
        Mnemonic::Qsub => (0xFA80, 0xF0A0),
        Mnemonic::Qdsub => (0xFA80, 0xF0B0),
        _ => unreachable!(),
    };
    let hw1: u16 = hw1_base | (rn.value() as u16 & 0xF);
    let hw2: u16 = hw2_op | ((rd.value() as u16 & 0xF) << 8) | (rm.value() as u16 & 0xF);
    Ok(emit32_thumb(hw1, hw2))
}

// ---------------------------------------------------------------------------
// Packing: PKHBT, PKHTB, SEL
// ---------------------------------------------------------------------------

fn encode_t2_pkhbt(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let (rd, rn, rm, shift_amt) = match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm)] => (*rd, *rn, *rm, 0u8),
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Shifted(rm, _, amt)] => {
            let a = match amt.as_ref() { Operand::Imm(n) => *n as u8, _ => 0 };
            (*rd, *rn, *rm, a)
        }
        _ => return Err(AsmError::new(line, "PKHBT/PKHTB: need Rd, Rn, Rm")),
    };
    let tb = inst.mnemonic == Mnemonic::Pkhtb;
    let stype = if tb { 0b10u16 } else { 0b00u16 }; // ASR for PKHTB, LSL for PKHBT
    let imm3 = ((shift_amt >> 2) & 7) as u16;
    let imm2 = (shift_amt & 3) as u16;
    let hw1: u16 = 0xEAC0 | (rn.value() as u16 & 0xF);
    let hw2: u16 = (imm3 << 12) | ((rd.value() as u16 & 0xF) << 8) | (imm2 << 6) | (stype << 4) | (rm.value() as u16 & 0xF);
    Ok(emit32_thumb(hw1, hw2))
}

fn encode_t2_sel(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let (rd, rn, rm) = match inst.operands.as_slice() {
        [Operand::Reg(rd), Operand::Reg(rn), Operand::Reg(rm)] => (*rd, *rn, *rm),
        _ => return Err(AsmError::new(line, "SEL: need Rd, Rn, Rm")),
    };
    let hw1: u16 = 0xFAA0 | (rn.value() as u16 & 0xF);
    let hw2: u16 = 0xF080 | ((rd.value() as u16 & 0xF) << 8) | (rm.value() as u16 & 0xF);
    Ok(emit32_thumb(hw1, hw2))
}

// ---------------------------------------------------------------------------
// Misc: DBG, CPS, unprivileged load/store, preload
// ---------------------------------------------------------------------------

fn encode_t2_dbg(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let opt = match inst.operands.as_slice() {
        [Operand::Imm(n)] => *n as u16 & 0xF,
        _ => return Err(AsmError::new(line, "DBG: need #option")),
    };
    Ok(emit32_thumb(0xF3AF, 0x80F0 | opt))
}

fn encode_t2_cps(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let enable = inst.mnemonic == Mnemonic::Cpsie;
    let flags = match inst.operands.as_slice() {
        [Operand::Label(s)] => {
            let mut f = 0u8;
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
    // 16-bit CPS: 10110110011 im AIF
    let im = if enable { 0u16 } else { 1u16 };
    let hw: u16 = 0xB660 | (im << 4) | (flags as u16 & 7);
    Ok(emit16(hw))
}

fn encode_t2_ldr_str_unpriv(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let (rt, rn, imm) = match inst.operands.as_slice() {
        [Operand::Reg(rt), Operand::Memory { base, offset: MemOffset::Imm(imm), .. }] => (*rt, *base, *imm),
        _ => return Err(AsmError::new(line, "unprivileged LDR/STR: need Rt, [Rn, #imm]")),
    };
    if imm < 0 || imm > 255 {
        return Err(AsmError::new(line, "unprivileged LDR/STR: offset must be 0..255"));
    }
    let hw1_base: u16 = match inst.mnemonic {
        Mnemonic::Ldrt => 0xF850,
        Mnemonic::Strt => 0xF840,
        Mnemonic::Ldrbt => 0xF810,
        Mnemonic::Strbt => 0xF800,
        Mnemonic::Ldrht => 0xF830,
        Mnemonic::Strht => 0xF820,
        Mnemonic::Ldrsbt => 0xF910,
        Mnemonic::Ldrsht => 0xF930,
        _ => unreachable!(),
    };
    let hw1: u16 = hw1_base | (rn.value() as u16 & 0xF);
    let hw2: u16 = ((rt.value() as u16 & 0xF) << 12) | 0x0E00 | (imm as u16 & 0xFF);
    Ok(emit32_thumb(hw1, hw2))
}

fn encode_t2_preload(inst: &Instruction) -> Result<Vec<u8>, AsmError> {
    let line = inst.line;
    let (rn, imm) = match inst.operands.as_slice() {
        [Operand::Memory { base, offset: MemOffset::Imm(imm), .. }] => (*base, *imm),
        _ => return Err(AsmError::new(line, "PLD/PLI: need [Rn, #imm]")),
    };
    let hw1_base: u16 = if inst.mnemonic == Mnemonic::Pld { 0xF890 } else { 0xF990 };
    if imm >= 0 && imm <= 4095 {
        let hw1: u16 = hw1_base | (rn.value() as u16 & 0xF);
        let hw2: u16 = 0xF000 | (imm as u16 & 0xFFF);
        Ok(emit32_thumb(hw1, hw2))
    } else if imm >= -255 && imm < 0 {
        let hw1: u16 = (hw1_base & 0xFF7F) | (rn.value() as u16 & 0xF); // clear U bit for negative
        let hw2: u16 = 0xFC00 | ((-imm) as u16 & 0xFF);
        Ok(emit32_thumb(hw1, hw2))
    } else {
        Err(AsmError::new(line, "PLD/PLI: offset out of range"))
    }
}
