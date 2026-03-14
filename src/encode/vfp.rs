use std::collections::HashMap;

use crate::ast::*;
use crate::error::AsmError;

use super::EncodedInst;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn encode_vfp(
    inst: &Instruction,
    isa: Isa,
    _offset: u32,
    _symbols: &HashMap<String, (usize, u32)>,
    _equs: &HashMap<String, i64>,
    _local_labels: &HashMap<u32, Vec<(usize, u32)>>,
    _section: usize,
) -> Result<EncodedInst, AsmError> {
    let word = encode_vfp_a32(inst)?;
    match isa {
        Isa::A32 => Ok(emit32(word)),
        Isa::Thumb => {
            // Thumb-2 VFP: replace condition nibble with 0xE, split into two halfwords.
            let word = (word & 0x0FFF_FFFF) | 0xE000_0000;
            let hw1 = (word >> 16) as u16;
            let hw2 = word as u16;
            Ok(emit32_thumb(hw1, hw2))
        }
    }
}

/// Returns true if this mnemonic is a VFP instruction.
pub fn is_vfp(m: Mnemonic) -> bool {
    use Mnemonic::*;
    matches!(
        m,
        Vadd | Vsub | Vmul | Vdiv | Vsqrt | Vabs | Vneg | Vmov | Vcmp | Vcmpe | Vcvt | Vcvtr
            | Vldr | Vstr | Vpush | Vpop | Vmrs | Vmsr
    )
}

// ---------------------------------------------------------------------------
// Register field helpers
// ---------------------------------------------------------------------------

/// Single-precision register Sd: Vd = reg[4:1], D = reg[0]
fn sd_vd(reg: u8) -> u32 {
    ((reg >> 1) & 0xF) as u32
}
fn sd_d(reg: u8) -> u32 {
    (reg & 1) as u32
}

/// Single-precision register Sn: Vn = reg[4:1], N = reg[0]
fn sn_vn(reg: u8) -> u32 {
    ((reg >> 1) & 0xF) as u32
}
fn sn_n(reg: u8) -> u32 {
    (reg & 1) as u32
}

/// Single-precision register Sm: Vm = reg[4:1], M = reg[0]
fn sm_vm(reg: u8) -> u32 {
    ((reg >> 1) & 0xF) as u32
}
fn sm_m(reg: u8) -> u32 {
    (reg & 1) as u32
}

/// Double-precision register Dd: Vd = reg[3:0], D = reg[4]
fn dd_vd(reg: u8) -> u32 {
    (reg & 0xF) as u32
}
fn dd_d(reg: u8) -> u32 {
    ((reg >> 4) & 1) as u32
}

/// Double-precision register Dn: Vn = reg[3:0], N = reg[4]
fn dn_vn(reg: u8) -> u32 {
    (reg & 0xF) as u32
}
fn dn_n(reg: u8) -> u32 {
    ((reg >> 4) & 1) as u32
}

/// Double-precision register Dm: Vm = reg[3:0], M = reg[4]
fn dm_vm(reg: u8) -> u32 {
    (reg & 0xF) as u32
}
fn dm_m(reg: u8) -> u32 {
    ((reg >> 4) & 1) as u32
}

// ---------------------------------------------------------------------------
// Condition bits
// ---------------------------------------------------------------------------

fn cond_bits(inst: &Instruction) -> u32 {
    let c = inst.condition.unwrap_or(Condition::Al);
    c.raw_value().value() as u32
}

// ---------------------------------------------------------------------------
// Emit helpers
// ---------------------------------------------------------------------------

fn emit32(val: u32) -> EncodedInst {
    EncodedInst::W32(val)
}

fn emit32_thumb(hw1: u16, hw2: u16) -> EncodedInst {
    EncodedInst::W32((hw2 as u32) << 16 | hw1 as u32)
}

// ---------------------------------------------------------------------------
// sz bit: 0 for F32 (single), 1 for F64 (double)
// ---------------------------------------------------------------------------

fn sz_bit(fp_size: FpSize) -> u32 {
    match fp_size {
        FpSize::F32 => 0,
        FpSize::F64 => 1,
    }
}

// ---------------------------------------------------------------------------
// Dest/src field setters for the standard VFP layout
// ---------------------------------------------------------------------------

/// Place Vd/D fields for a destination register.
/// For single: D at bit 22, Vd at bits [15:12]
/// For double: D at bit 22, Vd at bits [15:12]
fn set_vd(word: u32, reg: u8, double: bool) -> u32 {
    if double {
        word | (dd_d(reg) << 22) | (dd_vd(reg) << 12)
    } else {
        word | (sd_d(reg) << 22) | (sd_vd(reg) << 12)
    }
}

/// Place Vn/N fields for the first source register.
/// For single: N at bit 7, Vn at bits [19:16]
/// For double: N at bit 7, Vn at bits [19:16]
fn set_vn(word: u32, reg: u8, double: bool) -> u32 {
    if double {
        word | (dn_n(reg) << 7) | (dn_vn(reg) << 16)
    } else {
        word | (sn_n(reg) << 7) | (sn_vn(reg) << 16)
    }
}

/// Place Vm/M fields for the second source register.
/// For single: M at bit 5, Vm at bits [3:0]
/// For double: M at bit 5, Vm at bits [3:0]
fn set_vm(word: u32, reg: u8, double: bool) -> u32 {
    if double {
        word | (dm_m(reg) << 5) | (dm_vm(reg))
    } else {
        word | (sm_m(reg) << 5) | (sm_vm(reg))
    }
}

// ---------------------------------------------------------------------------
// VFP immediate encoding (8-bit float)
// ---------------------------------------------------------------------------

/// Encode a floating-point constant as a VFP 8-bit immediate.
/// The 8-bit value abcdefgh encodes the float:
///   (-1)^a * 2^(1-bcd) * 1.efgh (for F32, scaled appropriately for F64)
fn encode_vfp_imm(val: f64, fp_size: FpSize) -> Option<u8> {
    match fp_size {
        FpSize::F32 => {
            let bits = (val as f32).to_bits();
            // Sign bit
            let sign = (bits >> 31) & 1;
            // Exponent: bits [30:23]
            let exp = (bits >> 23) & 0xFF;
            // Mantissa: bits [22:0]
            let mant = bits & 0x7F_FFFF;
            // Low 19 bits of mantissa must be zero
            if mant & 0x7_FFFF != 0 {
                return None;
            }
            let mant4 = (mant >> 19) & 0xF;
            // Exponent must be 100_xxxx or 011_xxxx pattern
            // exp[7] != exp[6] is required, and exp[5:4] must equal exp[6]
            let exp7 = (exp >> 7) & 1;
            let exp6 = (exp >> 6) & 1;
            if exp7 == exp6 {
                return None;
            }
            // exp[5:4] must both equal exp[6]
            if ((exp >> 5) & 1) != exp6 || ((exp >> 4) & 1) != exp6 {
                return None;
            }
            let _exp_low = exp & 0xF;
            // Reconstruct: a=sign, bcd from exponent, efgh from mantissa
            // b = NOT(exp[6]) = exp[7]... wait.
            // The 8 bits are: a:NOT(B):C:D:e:f:g:h
            // where sign=a, exp = NOT(a) B C D + bias
            // Actually the encoding is: imm8 = a:B:c:d:e:f:g:h
            // a = sign bit
            // B = NOT(exp[6])  (which equals exp[7] since they must differ)
            // cd = exp[5:4]... but we already checked those equal exp[6]
            // Actually let me re-derive:
            // For F32: the encoded value decodes as:
            //   bit7 = sign (a)
            //   bit6 = NOT(exp bit 7) essentially
            //   bit[5:4] = exp[1:0] mapped
            //   bit[3:0] = mantissa top 4 bits
            //
            // Simpler approach: enumerate to check
            // abcdefgh -> (-1)^a * 2^(bcd - 3) * (1.efgh)
            // where bcd is interpreted with b being NOT of the exp high bit
            //
            // F32 exp bias=127. The exponent range is 124..131 (i.e. -3..+4)
            // exp = (NOT(b), b, b, b, b, c, d) + something...
            //
            // Let me just use the standard check:
            // For F32: exp must be in [124, 131]
            if exp < 124 || exp > 131 {
                return None;
            }
            let biased = exp - 124; // 0..7
            // imm8 = sign:NOT(biased[2]):biased[1:0]:mant4
            let b_not = ((biased >> 2) & 1) ^ 1;
            let cd = biased & 3;
            let imm8 = (sign << 7) | (b_not << 6) | (cd << 4) | mant4;
            Some(imm8 as u8)
        }
        FpSize::F64 => {
            let bits = val.to_bits();
            let sign = (bits >> 63) & 1;
            let exp = ((bits >> 52) & 0x7FF) as u32;
            let mant = bits & 0xF_FFFF_FFFF_FFFF;
            // Low 48 bits of mantissa must be zero
            if mant & 0xFFFF_FFFF_FFFF != 0 {
                return None;
            }
            let mant4 = ((mant >> 48) & 0xF) as u32;
            // F64 exp bias=1023. Range is [1020, 1027]
            if exp < 1020 || exp > 1027 {
                return None;
            }
            let biased = exp - 1020;
            let b_not = ((biased >> 2) & 1) ^ 1;
            let cd = biased & 3;
            let imm8 = ((sign as u32) << 7) | (b_not << 6) | (cd << 4) | mant4;
            Some(imm8 as u8)
        }
    }
}

// ---------------------------------------------------------------------------
// A32 encoding core
// ---------------------------------------------------------------------------

fn encode_vfp_a32(inst: &Instruction) -> Result<u32, AsmError> {
    use Mnemonic::*;
    let line = inst.line;
    match inst.mnemonic {
        Vadd | Vsub | Vmul | Vdiv => encode_binary_dp(inst),
        Vabs | Vneg | Vsqrt => encode_unary_dp(inst),
        Vmov => encode_vmov(inst),
        Vcmp | Vcmpe => encode_vcmp(inst),
        Vcvt => encode_vcvt(inst),
        Vcvtr => encode_vcvtr(inst),
        Vldr | Vstr => encode_vldr_vstr(inst),
        Vpush | Vpop => encode_vpush_vpop(inst),
        Vmrs => encode_vmrs(inst),
        Vmsr => encode_vmsr(inst),
        _ => Err(AsmError::new(line, format!("{:?} not a VFP instruction", inst.mnemonic))),
    }
}

// ---------------------------------------------------------------------------
// Binary data-processing: VADD, VSUB, VMUL, VDIV
// cond 1110 {p}{D}{q} Vn Vd 101{sz} {N}{r}{M}0 Vm
// ---------------------------------------------------------------------------

fn encode_binary_dp(inst: &Instruction) -> Result<u32, AsmError> {
    use Mnemonic::*;
    let line = inst.line;
    let fp_size = inst.fp_size.ok_or_else(|| AsmError::new(line, "VFP binary op: need .F32 or .F64 suffix"))?;
    let double = fp_size == FpSize::F64;

    let (rd, rn, rm) = match inst.operands.as_slice() {
        [dest, src1, src2] => {
            let d = get_fp_reg(dest, double, line, "Vd")?;
            let n = get_fp_reg(src1, double, line, "Vn")?;
            let m = get_fp_reg(src2, double, line, "Vm")?;
            (d, n, m)
        }
        _ => return Err(AsmError::new(line, "VFP binary op: need Vd, Vn, Vm")),
    };

    // Base: cond 1110 .... .... .... 101z ...0 ....
    let mut word: u32 = (cond_bits(inst) << 28) | (0b1110 << 24) | (0b101 << 9) | (sz_bit(fp_size) << 8);

    // Instruction-specific bits
    match inst.mnemonic {
        Vadd => {
            // op=11 (bit23=0, bit20=0), op2=0 (bit6=0)
            // cond 1110 0D11 Vn Vd 101z N0M0 Vm
            word |= 0b11 << 20;
        }
        Vsub => {
            // op=11, op2=1 (bit6=1)
            // cond 1110 0D11 Vn Vd 101z N1M0 Vm
            word |= 0b11 << 20;
            word |= 1 << 6;
        }
        Vmul => {
            // op=10, op2=0
            // cond 1110 0D10 Vn Vd 101z N0M0 Vm
            word |= 0b10 << 20;
        }
        Vdiv => {
            // bit23=1, op=00, op2=0
            // cond 1110 1D00 Vn Vd 101z N0M0 Vm
            word |= 1 << 23;
        }
        _ => unreachable!(),
    }

    word = set_vd(word, rd, double);
    word = set_vn(word, rn, double);
    word = set_vm(word, rm, double);

    Ok(word)
}

// ---------------------------------------------------------------------------
// Unary data-processing: VABS, VNEG, VSQRT
// cond 1110 1D11 opc2 Vd 101z o1-o2-M-0 Vm
// ---------------------------------------------------------------------------

fn encode_unary_dp(inst: &Instruction) -> Result<u32, AsmError> {
    use Mnemonic::*;
    let line = inst.line;
    let fp_size = inst.fp_size.ok_or_else(|| AsmError::new(line, "VFP unary op: need .F32 or .F64 suffix"))?;
    let double = fp_size == FpSize::F64;

    let (rd, rm) = match inst.operands.as_slice() {
        [dest, src] => {
            let d = get_fp_reg(dest, double, line, "Vd")?;
            let m = get_fp_reg(src, double, line, "Vm")?;
            (d, m)
        }
        _ => return Err(AsmError::new(line, "VFP unary op: need Vd, Vm")),
    };

    // Base: cond 1110 1D11 opc2 Vd 101z o1-o2-M-0 Vm
    let mut word: u32 = (cond_bits(inst) << 28)
        | (0b1110_1 << 23)
        | (0b11 << 20)
        | (0b101 << 9)
        | (sz_bit(fp_size) << 8);

    let (opc2, o1, o2) = match inst.mnemonic {
        Vabs =>  (0b0000, 1u32, 1u32),
        Vneg =>  (0b0001, 0u32, 1u32),
        Vsqrt => (0b0001, 1u32, 1u32),
        _ => unreachable!(),
    };

    word |= opc2 << 16;
    word |= o1 << 7;
    word |= o2 << 6;
    word = set_vd(word, rd, double);
    word = set_vm(word, rm, double);

    Ok(word)
}

// ---------------------------------------------------------------------------
// VMOV - multiple forms
// ---------------------------------------------------------------------------

fn encode_vmov(inst: &Instruction) -> Result<u32, AsmError> {
    let line = inst.line;
    let ops = &inst.operands;

    match ops.as_slice() {
        // VMOV Sd, Sm  (FP reg-to-reg copy, single)
        [Operand::SReg(d), Operand::SReg(m)] => {
            // This is the unary VMOV: cond 1110 1D11 0000 Vd 101z 01M0 Vm
            let fp_size = inst.fp_size.unwrap_or(FpSize::F32);
            if fp_size != FpSize::F32 {
                return Err(AsmError::new(line, "VMOV single regs require .F32"));
            }
            let mut word: u32 = (cond_bits(inst) << 28)
                | (0b1110_1 << 23)
                | (0b11 << 20)
                | (0b0000 << 16)
                | (0b101 << 9)
                | (0 << 8)   // sz=0 for single
                | (0 << 7)   // o1=0
                | (1 << 6);  // o2=1
            word = set_vd(word, *d, false);
            word = set_vm(word, *m, false);
            Ok(word)
        }
        // VMOV Dd, Dm  (FP reg-to-reg copy, double)
        [Operand::DReg(d), Operand::DReg(m)] => {
            let fp_size = inst.fp_size.unwrap_or(FpSize::F64);
            if fp_size != FpSize::F64 {
                return Err(AsmError::new(line, "VMOV double regs require .F64"));
            }
            let mut word: u32 = (cond_bits(inst) << 28)
                | (0b1110_1 << 23)
                | (0b11 << 20)
                | (0b0000 << 16)
                | (0b101 << 9)
                | (1 << 8)   // sz=1 for double
                | (0 << 7)   // o1=0
                | (1 << 6);  // o2=1
            word = set_vd(word, *d, true);
            word = set_vm(word, *m, true);
            Ok(word)
        }
        // VMOV Sd, #imm  (FP immediate, single)
        [Operand::SReg(d), Operand::FpImm(val)] => {
            let fp_size = inst.fp_size.unwrap_or(FpSize::F32);
            let imm8 = encode_vfp_imm(*val, fp_size)
                .ok_or_else(|| AsmError::new(line, "VMOV: float not representable as VFP imm8"))?;
            let imm4h = ((imm8 >> 4) & 0xF) as u32;
            let imm4l = (imm8 & 0xF) as u32;
            let mut word: u32 = (cond_bits(inst) << 28)
                | (0b1110_1 << 23)
                | (0b11 << 20)
                | (imm4h << 16)
                | (0b101 << 9)
                | (sz_bit(fp_size) << 8)
                | imm4l;
            word = set_vd(word, *d, false);
            Ok(word)
        }
        // VMOV Dd, #imm  (FP immediate, double)
        [Operand::DReg(d), Operand::FpImm(val)] => {
            let fp_size = inst.fp_size.unwrap_or(FpSize::F64);
            let imm8 = encode_vfp_imm(*val, fp_size)
                .ok_or_else(|| AsmError::new(line, "VMOV: float not representable as VFP imm8"))?;
            let imm4h = ((imm8 >> 4) & 0xF) as u32;
            let imm4l = (imm8 & 0xF) as u32;
            let mut word: u32 = (cond_bits(inst) << 28)
                | (0b1110_1 << 23)
                | (0b11 << 20)
                | (imm4h << 16)
                | (0b101 << 9)
                | (sz_bit(fp_size) << 8)
                | imm4l;
            word = set_vd(word, *d, true);
            Ok(word)
        }
        // VMOV Sn, Rt  (core register to single)
        [Operand::SReg(n), Operand::Reg(rt)] => {
            // cond 1110 0000 Vn Rt 1010 N001 0000
            let rt_val = rt.value() as u32;
            let mut word: u32 = (cond_bits(inst) << 28)
                | (0b1110 << 24)
                | (0b0000 << 20)
                | (rt_val << 12)
                | (0b1010 << 8)
                | (0b0001 << 4)
                | 0b0000;
            word |= sn_vn(*n) << 16;
            word |= sn_n(*n) << 7;
            Ok(word)
        }
        // VMOV Rt, Sn  (single to core register)
        [Operand::Reg(rt), Operand::SReg(n)] => {
            // cond 1110 0001 Vn Rt 1010 N001 0000
            let rt_val = rt.value() as u32;
            let mut word: u32 = (cond_bits(inst) << 28)
                | (0b1110 << 24)
                | (0b0001 << 20)
                | (rt_val << 12)
                | (0b1010 << 8)
                | (0b0001 << 4)
                | 0b0000;
            word |= sn_vn(*n) << 16;
            word |= sn_n(*n) << 7;
            Ok(word)
        }
        // VMOV Dm, Rt, Rt2  (two core regs to double)
        [Operand::DReg(m), Operand::Reg(rt), Operand::Reg(rt2)] => {
            // cond 1100 0100 Rt2 Rt 1011 00M1 Vm
            let rt_val = rt.value() as u32;
            let rt2_val = rt2.value() as u32;
            let word: u32 = (cond_bits(inst) << 28)
                | (0b1100 << 24)
                | (0b0100 << 20)
                | (rt2_val << 16)
                | (rt_val << 12)
                | (0b1011 << 8)
                | (dm_m(*m) << 5)
                | (1 << 4)
                | dm_vm(*m);
            Ok(word)
        }
        // VMOV Rt, Rt2, Dm  (double to two core regs)
        [Operand::Reg(rt), Operand::Reg(rt2), Operand::DReg(m)] => {
            // cond 1100 0101 Rt2 Rt 1011 00M1 Vm
            let rt_val = rt.value() as u32;
            let rt2_val = rt2.value() as u32;
            let word: u32 = (cond_bits(inst) << 28)
                | (0b1100 << 24)
                | (0b0101 << 20)
                | (rt2_val << 16)
                | (rt_val << 12)
                | (0b1011 << 8)
                | (dm_m(*m) << 5)
                | (1 << 4)
                | dm_vm(*m);
            Ok(word)
        }
        _ => Err(AsmError::new(line, "VMOV: unsupported operand combination")),
    }
}

// ---------------------------------------------------------------------------
// VCMP / VCMPE
// ---------------------------------------------------------------------------

fn encode_vcmp(inst: &Instruction) -> Result<u32, AsmError> {
    let line = inst.line;
    let fp_size = inst.fp_size.ok_or_else(|| AsmError::new(line, "VCMP/VCMPE: need .F32 or .F64 suffix"))?;
    let double = fp_size == FpSize::F64;
    let is_vcmpe = inst.mnemonic == Mnemonic::Vcmpe;

    let o1: u32 = if is_vcmpe { 1 } else { 0 };

    match inst.operands.as_slice() {
        // VCMP{E} Vd, Vm
        [dest, src] if matches!(src, Operand::SReg(_) | Operand::DReg(_)) => {
            let rd = get_fp_reg(dest, double, line, "Vd")?;
            let rm = get_fp_reg(src, double, line, "Vm")?;
            // cond 1110 1D11 0100 Vd 101z o1-1-M-0 Vm
            let mut word: u32 = (cond_bits(inst) << 28)
                | (0b1110_1 << 23)
                | (0b11 << 20)
                | (0b0100 << 16)
                | (0b101 << 9)
                | (sz_bit(fp_size) << 8)
                | (o1 << 7)
                | (1 << 6); // o2=1
            word = set_vd(word, rd, double);
            word = set_vm(word, rm, double);
            Ok(word)
        }
        // VCMP{E} Vd, #0
        [dest, imm_zero] if is_zero_operand(imm_zero) => {
            let rd = get_fp_reg(dest, double, line, "Vd")?;
            // cond 1110 1D11 0101 Vd 101z o1-1-0-0
            let mut word: u32 = (cond_bits(inst) << 28)
                | (0b1110_1 << 23)
                | (0b11 << 20)
                | (0b0101 << 16)
                | (0b101 << 9)
                | (sz_bit(fp_size) << 8)
                | (o1 << 7)
                | (1 << 6); // o2=1
            word = set_vd(word, rd, double);
            // Vm=0, M=0 (already zero)
            Ok(word)
        }
        _ => Err(AsmError::new(line, "VCMP: need Vd, Vm or Vd, #0")),
    }
}

// ---------------------------------------------------------------------------
// VCVT
// ---------------------------------------------------------------------------

fn encode_vcvt(inst: &Instruction) -> Result<u32, AsmError> {
    let line = inst.line;
    let vcvt_kind = inst.vcvt_kind.ok_or_else(|| AsmError::new(line, "VCVT: need conversion kind suffix"))?;

    match vcvt_kind {
        VcvtKind::F32ToF64 | VcvtKind::F64ToF32 => encode_vcvt_precision(inst, vcvt_kind),
        _ => encode_vcvt_int_float(inst, vcvt_kind, true),
    }
}

fn encode_vcvtr(inst: &Instruction) -> Result<u32, AsmError> {
    let line = inst.line;
    let vcvt_kind = inst.vcvt_kind.ok_or_else(|| AsmError::new(line, "VCVTR: need conversion kind suffix"))?;
    encode_vcvt_int_float(inst, vcvt_kind, false)
}

/// VCVT between F32 and F64
/// cond 1110 1D11 0111 Vd 101{sz} 11M0 Vm
fn encode_vcvt_precision(inst: &Instruction, kind: VcvtKind) -> Result<u32, AsmError> {
    let line = inst.line;
    let (rd, rm) = match inst.operands.as_slice() {
        [dest, src] => (dest, src),
        _ => return Err(AsmError::new(line, "VCVT F32<->F64: need Dd, Sm or Sd, Dm")),
    };

    match kind {
        VcvtKind::F64ToF32 => {
            // Suffix .F64.F32: dest type is F64, so dest is Dd, src is Sm.
            // sz=0 for single-to-double conversion (sz indicates source is single)
            let d = match rd { Operand::DReg(r) => *r, _ => return Err(AsmError::new(line, "VCVT.F64.F32: dest must be Dd")) };
            let m = match rm { Operand::SReg(r) => *r, _ => return Err(AsmError::new(line, "VCVT.F64.F32: src must be Sm")) };
            let mut word: u32 = (cond_bits(inst) << 28)
                | (0b1110_1 << 23)
                | (0b11 << 20)
                | (0b0111 << 16)
                | (0b101 << 9)
                | (0 << 8)      // sz=0 (source is single, converting to double)
                | (0b11 << 6);
            word = set_vd(word, d, true);    // dest is double
            word = set_vm(word, m, false);   // src is single
            Ok(word)
        }
        VcvtKind::F32ToF64 => {
            // Suffix .F32.F64: dest type is F32, so dest is Sd, src is Dm.
            // sz=1 for double-to-single conversion (sz indicates source is double)
            let d = match rd { Operand::SReg(r) => *r, _ => return Err(AsmError::new(line, "VCVT.F32.F64: dest must be Sd")) };
            let m = match rm { Operand::DReg(r) => *r, _ => return Err(AsmError::new(line, "VCVT.F32.F64: src must be Dm")) };
            let mut word: u32 = (cond_bits(inst) << 28)
                | (0b1110_1 << 23)
                | (0b11 << 20)
                | (0b0111 << 16)
                | (0b101 << 9)
                | (1 << 8)      // sz=1 (source is double, converting to single)
                | (0b11 << 6);
            word = set_vd(word, d, false);   // dest is single
            word = set_vm(word, m, true);    // src is double
            Ok(word)
        }
        _ => unreachable!(),
    }
}

/// VCVT / VCVTR between int and float
/// To float: cond 1110 1D11 1000 Vd 101{sz} {op}1M0 Vm
///   op=1 signed, op=0 unsigned; src is always Sm (integer in S reg)
/// To int:   cond 1110 1D11 110{to_signed} Vd 101{sz} {rz}1M0 Vm
///   rz=1 for VCVT (round to zero), rz=0 for VCVTR (use FPSCR rounding)
fn encode_vcvt_int_float(inst: &Instruction, kind: VcvtKind, round_to_zero: bool) -> Result<u32, AsmError> {
    let line = inst.line;
    let (rd_op, rm_op) = match inst.operands.as_slice() {
        [dest, src] => (dest, src),
        _ => return Err(AsmError::new(line, "VCVT int<->float: need Vd, Vm")),
    };

    match kind {
        // To float: suffix .F32.S32 / .F32.U32 / .F64.S32 / .F64.U32
        // Dest type is float (first suffix), src is integer Sm
        VcvtKind::F32ToS32 | VcvtKind::F32ToU32 | VcvtKind::F64ToS32 | VcvtKind::F64ToU32 => {
            let signed = matches!(kind, VcvtKind::F32ToS32 | VcvtKind::F64ToS32);
            let to_double = matches!(kind, VcvtKind::F64ToS32 | VcvtKind::F64ToU32);
            let op: u32 = if signed { 1 } else { 0 };

            let d = get_fp_reg(rd_op, to_double, line, "Vd")?;
            let m = match rm_op { Operand::SReg(r) => *r, _ => return Err(AsmError::new(line, "VCVT to float: src must be Sm")) };

            // cond 1110 1D11 1000 Vd 101{sz} {op}1M0 Vm
            let mut word: u32 = (cond_bits(inst) << 28)
                | (0b1110_1 << 23)
                | (0b11 << 20)
                | (0b1000 << 16)
                | (0b101 << 9)
                | (sz_bit(if to_double { FpSize::F64 } else { FpSize::F32 }) << 8)
                | (op << 7)
                | (1 << 6);
            word = set_vd(word, d, to_double);
            word = set_vm(word, m, false);  // src is always single (contains integer)
            Ok(word)
        }
        // To int: suffix .S32.F32 / .U32.F32 / .S32.F64 / .U32.F64
        // Dest type is integer (first suffix), src is float Sd/Dd
        VcvtKind::S32ToF32 | VcvtKind::U32ToF32 | VcvtKind::S32ToF64 | VcvtKind::U32ToF64 => {
            let to_signed = matches!(kind, VcvtKind::S32ToF32 | VcvtKind::S32ToF64);
            let from_double = matches!(kind, VcvtKind::S32ToF64 | VcvtKind::U32ToF64);
            let rz: u32 = if round_to_zero { 1 } else { 0 };

            let d = match rd_op { Operand::SReg(r) => *r, _ => return Err(AsmError::new(line, "VCVT to int: dest must be Sd")) };
            let m = get_fp_reg(rm_op, from_double, line, "Vm")?;

            // cond 1110 1D11 110{to_signed} Vd 101{sz} {rz}1M0 Vm
            let opc2_top3: u32 = 0b110;
            let ts: u32 = if to_signed { 1 } else { 0 };
            let opc2 = (opc2_top3 << 1) | ts;

            let mut word: u32 = (cond_bits(inst) << 28)
                | (0b1110_1 << 23)
                | (0b11 << 20)
                | (opc2 << 16)
                | (0b101 << 9)
                | (sz_bit(if from_double { FpSize::F64 } else { FpSize::F32 }) << 8)
                | (rz << 7)
                | (1 << 6);
            word = set_vd(word, d, false);  // dest is always single (contains integer)
            word = set_vm(word, m, from_double);
            Ok(word)
        }
        _ => Err(AsmError::new(line, "VCVT: unsupported conversion kind")),
    }
}

// ---------------------------------------------------------------------------
// VLDR / VSTR
// cond 1101 UD{load}1 Rn Vd 101z imm8
// ---------------------------------------------------------------------------

fn encode_vldr_vstr(inst: &Instruction) -> Result<u32, AsmError> {
    let line = inst.line;
    let is_load = inst.mnemonic == Mnemonic::Vldr;
    let ops = &inst.operands;

    let (fp_reg, mem) = match ops.as_slice() {
        [reg, mem @ Operand::Memory { .. }] => (reg, mem),
        _ => return Err(AsmError::new(line, "VLDR/VSTR: need Vd, [Rn, #offset]")),
    };

    let (base, offset_val) = match mem {
        Operand::Memory { base, offset, pre_index: true, writeback: false } => {
            let off = match offset {
                MemOffset::Imm(v) => *v,
                _ => return Err(AsmError::new(line, "VLDR/VSTR: only immediate offset supported")),
            };
            (*base, off)
        }
        _ => return Err(AsmError::new(line, "VLDR/VSTR: need pre-indexed without writeback")),
    };

    // offset must be a multiple of 4 and in range [-1020, 1020]
    if offset_val % 4 != 0 && offset_val != 0 {
        // Check if abs value is multiple of 4
        if offset_val.unsigned_abs() % 4 != 0 {
            return Err(AsmError::new(line, "VLDR/VSTR: offset must be a multiple of 4"));
        }
    }
    let abs_off = offset_val.unsigned_abs();
    if abs_off > 1020 {
        return Err(AsmError::new(line, "VLDR/VSTR: offset out of range (-1020..1020)"));
    }
    let imm8 = (abs_off / 4) as u32;
    let u_bit: u32 = if offset_val >= 0 { 1 } else { 0 };
    let load_bit: u32 = if is_load { 1 } else { 0 };

    let base_val = base.value() as u32;

    // Determine single vs double from the register operand
    let (double, reg_num) = match fp_reg {
        Operand::SReg(r) => (false, *r),
        Operand::DReg(r) => (true, *r),
        _ => return Err(AsmError::new(line, "VLDR/VSTR: first operand must be Sd or Dd")),
    };

    // cond 1101 UD{load}1 Rn Vd 101z imm8
    // Bit layout: [31:28]=cond [27:24]=1101 [23]=U [22]=D [21:20]={load}1
    // Wait, re-check encoding:
    // cond 1101 UD01 Rn Vd 101z imm8  (VLDR)
    // cond 1101 UD00 Rn Vd 101z imm8  (VSTR)
    // Actually: bit[20] = L (load), bit[21:20] = ?
    // The ARM ARM says: cond 110P UDW1 Rn Vd 101z imm8 for VLDR
    // For single load: P=1, U=sign, W=0, L=1 -> 1101 U D 0 1
    // For single store: P=1, U=sign, W=0, L=0 -> 1101 U D 0 0
    let mut word: u32 = (cond_bits(inst) << 28)
        | (0b1101 << 24)
        | (u_bit << 23)
        | (load_bit << 20)
        | (base_val << 16)
        | (0b101 << 9)
        | (sz_bit(if double { FpSize::F64 } else { FpSize::F32 }) << 8)
        | imm8;

    word = set_vd(word, reg_num, double);

    Ok(word)
}

// ---------------------------------------------------------------------------
// VPUSH / VPOP
// VPUSH: cond 1101 0D10 1101 Vd 101z imm8
// VPOP:  cond 1100 1D11 1101 Vd 101z imm8
// ---------------------------------------------------------------------------

fn encode_vpush_vpop(inst: &Instruction) -> Result<u32, AsmError> {
    let line = inst.line;
    let is_pop = inst.mnemonic == Mnemonic::Vpop;

    let reglist = match inst.operands.as_slice() {
        [Operand::FpRegList { start, count, double }] => (*start, *count, *double),
        _ => return Err(AsmError::new(line, "VPUSH/VPOP: need {Sx-Sy} or {Dx-Dy}")),
    };

    let (start, count, double) = reglist;
    let imm8: u32 = if double { (count as u32) * 2 } else { count as u32 };

    let mut word: u32;
    if is_pop {
        // VPOP:  cond 1100 1D11 1101 Vd 101z imm8
        word = (cond_bits(inst) << 28)
            | (0b1100_1 << 23)
            | (0b11 << 20)
            | (0b1101 << 16)
            | (0b101 << 9)
            | (sz_bit(if double { FpSize::F64 } else { FpSize::F32 }) << 8)
            | imm8;
    } else {
        // VPUSH: cond 1101 0D10 1101 Vd 101z imm8
        word = (cond_bits(inst) << 28)
            | (0b1101_0 << 23)
            | (0b10 << 20)
            | (0b1101 << 16)
            | (0b101 << 9)
            | (sz_bit(if double { FpSize::F64 } else { FpSize::F32 }) << 8)
            | imm8;
    }

    word = set_vd(word, start, double);

    Ok(word)
}

// ---------------------------------------------------------------------------
// VMRS / VMSR
// VMRS Rt, FPSCR: cond 1110 1111 0001 Rt 1010 0001 0000
// VMSR FPSCR, Rt: cond 1110 1110 0001 Rt 1010 0001 0000
// ---------------------------------------------------------------------------

fn encode_vmrs(inst: &Instruction) -> Result<u32, AsmError> {
    let line = inst.line;
    // VMRS Rt, FPSCR  or  VMRS APSR_nzcv, FPSCR
    let rt_val: u32 = match inst.operands.as_slice() {
        [Operand::Reg(rt), Operand::Fpscr] => rt.value() as u32,
        [Operand::ApsrNzcv, Operand::Fpscr] => 15,
        _ => return Err(AsmError::new(line, "VMRS: need Rt, FPSCR or APSR_nzcv, FPSCR")),
    };
    // cond 1110 1111 0001 Rt 1010 0001 0000
    let word: u32 = (cond_bits(inst) << 28)
        | (0b1110_1111 << 20)
        | (0b0001 << 16)
        | (rt_val << 12)
        | (0b1010 << 8)
        | (0b0001 << 4);
    Ok(word)
}

fn encode_vmsr(inst: &Instruction) -> Result<u32, AsmError> {
    let line = inst.line;
    // VMSR FPSCR, Rt
    let rt_val: u32 = match inst.operands.as_slice() {
        [Operand::Fpscr, Operand::Reg(rt)] => rt.value() as u32,
        _ => return Err(AsmError::new(line, "VMSR: need FPSCR, Rt")),
    };
    // cond 1110 1110 0001 Rt 1010 0001 0000
    let word: u32 = (cond_bits(inst) << 28)
        | (0b1110_1110 << 20)
        | (0b0001 << 16)
        | (rt_val << 12)
        | (0b1010 << 8)
        | (0b0001 << 4);
    Ok(word)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Check if an operand represents zero (integer 0 or float 0.0).
fn is_zero_operand(op: &Operand) -> bool {
    match op {
        Operand::Imm(0) => true,
        Operand::FpImm(v) => *v == 0.0,
        _ => false,
    }
}

/// Extract the register number from an operand, checking it matches the expected precision.
fn get_fp_reg(op: &Operand, double: bool, line: usize, ctx: &str) -> Result<u8, AsmError> {
    match (op, double) {
        (Operand::SReg(r), false) => Ok(*r),
        (Operand::DReg(r), true) => Ok(*r),
        (Operand::SReg(_), true) => Err(AsmError::new(line, format!("{ctx}: expected D register, got S register"))),
        (Operand::DReg(_), false) => Err(AsmError::new(line, format!("{ctx}: expected S register, got D register"))),
        _ => Err(AsmError::new(line, format!("{ctx}: expected FP register"))),
    }
}
