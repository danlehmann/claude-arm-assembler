mod a32;
mod thumb;
mod vfp;

use std::collections::HashMap;

use crate::ast::*;
use crate::error::AsmError;

enum EncodedInst {
    W16(u16),
    W32(u32),
}

impl EncodedInst {
    fn len(&self) -> u32 {
        match self {
            EncodedInst::W16(_) => 2,
            EncodedInst::W32(_) => 4,
        }
    }

    fn extend_into(&self, buf: &mut Vec<u8>) {
        match self {
            EncodedInst::W16(v) => buf.extend_from_slice(&v.to_le_bytes()),
            EncodedInst::W32(v) => buf.extend_from_slice(&v.to_le_bytes()),
        }
    }
}
use crate::{AsmConfig, AsmOutput, Section, Symbol};

struct AsmState {
    sections: Vec<SectionBuilder>,
    current_section: usize,
    isa: Isa,
    symbols: HashMap<String, (usize, u32)>, // (section_index, offset)
    globals: Vec<String>,
    equs: HashMap<String, i64>,
    /// Local (numeric) labels: number → sorted list of (section_index, offset).
    local_labels: HashMap<u32, Vec<(usize, u32)>>,
    /// Pending literal pool entries not yet flushed.
    pending_pool: Vec<PendingPoolEntry>,
    /// Maps (section_index, ldr_instruction_offset) → pool_word_offset.
    pool_map: HashMap<(usize, u32), u32>,
    /// Flushed pools ready for pass2 emission.
    pool_emissions: Vec<PoolEmission>,
    /// Index into pool_emissions for pass2 iteration.
    next_pool_emission: usize,
}

struct PendingPoolEntry {
    expr: Expr,
    line: usize,
    section: usize,
    ldr_offset: u32,
}

struct PoolEmission {
    section: usize,
    padding: u32,
    entries: Vec<(Expr, usize, u32)>, // (expr, line, ldr_offset for label resolution)
}

struct SectionBuilder {
    name: String,
    data: Vec<u8>,
    offset: u32,
}

impl SectionBuilder {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            data: Vec::new(),
            offset: 0,
        }
    }
}

pub fn assemble(stmts: &[Statement], config: &AsmConfig) -> Result<AsmOutput, AsmError> {
    let mut state = AsmState {
        sections: vec![SectionBuilder::new(".text")],
        current_section: 0,
        isa: config.default_isa,
        symbols: HashMap::new(),
        globals: Vec::new(),
        equs: HashMap::new(),
        local_labels: HashMap::new(),
        pending_pool: Vec::new(),
        pool_map: HashMap::new(),
        pool_emissions: Vec::new(),
        next_pool_emission: 0,
    };

    // Pass 1: collect labels, compute sizes
    pass1(stmts, &mut state)?;

    // Reset for pass 2
    for sec in &mut state.sections {
        sec.offset = 0;
    }
    state.current_section = 0;
    state.isa = config.default_isa;
    state.next_pool_emission = 0;

    // Pass 2: encode
    pass2(stmts, &mut state)?;

    // Build output
    let symbols = state
        .symbols
        .iter()
        .map(|(name, (sec_idx, offset))| Symbol {
            name: name.clone(),
            section_index: *sec_idx,
            offset: *offset,
            global: state.globals.contains(name),
        })
        .collect();

    let sections = state
        .sections
        .into_iter()
        .map(|sb| Section {
            name: sb.name,
            data: sb.data,
        })
        .collect();

    Ok(AsmOutput { sections, symbols })
}

fn pass1(stmts: &[Statement], state: &mut AsmState) -> Result<(), AsmError> {
    for stmt in stmts {
        match stmt {
            Statement::Label(name, _line) => {
                if let Ok(n) = name.parse::<u32>() {
                    // Numeric (local) label — can be defined multiple times
                    state
                        .local_labels
                        .entry(n)
                        .or_default()
                        .push((
                            state.current_section,
                            state.sections[state.current_section].offset,
                        ));
                } else {
                    state.symbols.insert(
                        name.clone(),
                        (
                            state.current_section,
                            state.sections[state.current_section].offset,
                        ),
                    );
                }
            }
            Statement::Instruction(inst) => {
                // Register literal pool entries before computing size
                if let [_, Operand::Pool(expr)] = inst.operands.as_slice() {
                    let sec = state.current_section;
                    let offset = state.sections[sec].offset;
                    state.pending_pool.push(PendingPoolEntry {
                        expr: expr.clone(),
                        line: inst.line,
                        section: sec,
                        ldr_offset: offset,
                    });
                }
                let size = instruction_size(inst, state.isa);
                state.sections[state.current_section].offset += size;
            }
            Statement::Directive(dir, _line) => {
                handle_directive_pass1(dir, state)?;
            }
        }
    }
    // Auto-flush remaining pool entries at end of assembly
    flush_pool(state);
    Ok(())
}

/// Flush pending literal pool entries, assigning offsets and recording for pass2.
fn flush_pool(state: &mut AsmState) {
    if state.pending_pool.is_empty() {
        return;
    }
    let sec_idx = state.current_section;
    let sec_offset = &mut state.sections[sec_idx].offset;

    // Align pool to 4 bytes
    let padding = (4 - (*sec_offset % 4)) % 4;
    *sec_offset += padding;

    let base = *sec_offset;
    let entries: Vec<(Expr, usize, u32)> = state
        .pending_pool
        .drain(..)
        .enumerate()
        .map(|(i, entry)| {
            let pool_word_offset = base + (i as u32) * 4;
            state
                .pool_map
                .insert((entry.section, entry.ldr_offset), pool_word_offset);
            (entry.expr, entry.line, entry.ldr_offset)
        })
        .collect();

    *sec_offset += (entries.len() as u32) * 4;
    state.pool_emissions.push(PoolEmission {
        section: sec_idx,
        padding,
        entries,
    });
}

fn pass2(stmts: &[Statement], state: &mut AsmState) -> Result<(), AsmError> {
    for stmt in stmts {
        match stmt {
            Statement::Label(_, _) => {}
            Statement::Instruction(inst) => {
                let offset = state.sections[state.current_section].offset;
                let enc = encode_instruction(inst, state.isa, offset, &state.symbols, &state.equs, &state.local_labels, state.current_section, &state.pool_map)?;
                debug_assert_eq!(
                    enc.len(),
                    instruction_size(inst, state.isa),
                    "instruction size mismatch at line {}: predicted {} bytes, encoded {}",
                    inst.line,
                    instruction_size(inst, state.isa),
                    enc.len()
                );
                let sec = &mut state.sections[state.current_section];
                enc.extend_into(&mut sec.data);
                sec.offset += enc.len();
            }
            Statement::Directive(dir, line) => {
                handle_directive_pass2(dir, *line, state)?;
            }
        }
    }
    // Emit any remaining pools at end of assembly
    emit_pool(state)?;
    Ok(())
}

/// Emit the next pending pool emission in pass2.
fn emit_pool(state: &mut AsmState) -> Result<(), AsmError> {
    while state.next_pool_emission < state.pool_emissions.len() {
        let emission = &state.pool_emissions[state.next_pool_emission];
        if emission.section != state.current_section {
            break;
        }
        // Clone data to avoid borrow conflict
        let padding = emission.padding;
        let entries: Vec<(Expr, usize, u32)> = emission.entries.clone();
        let sec_idx = emission.section;
        state.next_pool_emission += 1;

        // Emit alignment padding
        let sec = &mut state.sections[sec_idx];
        for _ in 0..padding {
            sec.data.push(0);
        }
        sec.offset += padding;

        // Emit pool words
        for (expr, line, ldr_offset) in &entries {
            // Resolve expression at the LDR instruction's offset (not the pool word's offset)
            // so that local label forward/backward references work correctly
            let val = resolve_expr(
                expr,
                &state.symbols,
                &state.equs,
                &state.local_labels,
                sec_idx,
                *ldr_offset,
                *line,
            )?;
            let sec = &mut state.sections[sec_idx];
            sec.data.extend_from_slice(&(val as u32).to_le_bytes());
            sec.offset += 4;
        }
    }
    Ok(())
}

fn instruction_size(inst: &Instruction, isa: Isa) -> u32 {
    match isa {
        Isa::A32 => 4,
        Isa::Thumb => {
            if inst.wide {
                return 4;
            }
            // Always 32-bit Thumb-2 instructions
            match inst.mnemonic {
                Mnemonic::Bl | Mnemonic::Movw | Mnemonic::Movt | Mnemonic::Orn
                | Mnemonic::Sdiv | Mnemonic::Udiv | Mnemonic::Mls
                | Mnemonic::Smlal | Mnemonic::Umlal | Mnemonic::Umaal
                | Mnemonic::Clz | Mnemonic::Rbit
                | Mnemonic::Bfi | Mnemonic::Bfc | Mnemonic::Ubfx | Mnemonic::Sbfx
                | Mnemonic::Ldrd | Mnemonic::Strd
                | Mnemonic::Ldrex | Mnemonic::Strex
                | Mnemonic::Ldrexb | Mnemonic::Strexb
                | Mnemonic::Ldrexh | Mnemonic::Strexh | Mnemonic::Clrex
                | Mnemonic::Mrs | Mnemonic::Msr
                | Mnemonic::Tbb | Mnemonic::Tbh
                | Mnemonic::Ssat | Mnemonic::Usat | Mnemonic::Ssat16 | Mnemonic::Usat16
                | Mnemonic::Ldrt | Mnemonic::Strt
                | Mnemonic::Ldrbt | Mnemonic::Strbt
                | Mnemonic::Ldrht | Mnemonic::Strht
                | Mnemonic::Ldrsbt | Mnemonic::Ldrsht
                | Mnemonic::Pld | Mnemonic::Pldw | Mnemonic::Pli
                | Mnemonic::Dbg | Mnemonic::Rrx
                // VFP (always 32-bit)
                | Mnemonic::Vadd | Mnemonic::Vsub | Mnemonic::Vmul | Mnemonic::Vdiv
                | Mnemonic::Vsqrt | Mnemonic::Vabs | Mnemonic::Vneg
                | Mnemonic::Vmov | Mnemonic::Vcmp | Mnemonic::Vcmpe
                | Mnemonic::Vcvt | Mnemonic::Vcvtr
                | Mnemonic::Vldr | Mnemonic::Vstr
                | Mnemonic::Vpush | Mnemonic::Vpop
                | Mnemonic::Vmrs | Mnemonic::Vmsr
                // DSP multiply (always 32-bit)
                | Mnemonic::Smmul | Mnemonic::Smmulr
                | Mnemonic::Smmla | Mnemonic::Smmlar
                | Mnemonic::Smmls | Mnemonic::Smmlsr
                | Mnemonic::Smulbb | Mnemonic::Smulbt
                | Mnemonic::Smultb | Mnemonic::Smultt
                | Mnemonic::Smulwb | Mnemonic::Smulwt
                | Mnemonic::Smlabb | Mnemonic::Smlabt
                | Mnemonic::Smlatb | Mnemonic::Smlatt
                | Mnemonic::Smlawb | Mnemonic::Smlawt
                | Mnemonic::Smlalbb | Mnemonic::Smlalbt
                | Mnemonic::Smlaltb | Mnemonic::Smlaltt
                | Mnemonic::Smuad | Mnemonic::Smuadx
                | Mnemonic::Smusd | Mnemonic::Smusdx
                | Mnemonic::Smlad | Mnemonic::Smladx
                | Mnemonic::Smlsd | Mnemonic::Smlsdx
                | Mnemonic::Smlald | Mnemonic::Smlaldx
                | Mnemonic::Smlsld | Mnemonic::Smlsldx
                | Mnemonic::Usad8 | Mnemonic::Usada8
                // Parallel arithmetic (always 32-bit)
                | Mnemonic::Sadd16 | Mnemonic::Sadd8
                | Mnemonic::Ssub16 | Mnemonic::Ssub8
                | Mnemonic::Uadd16 | Mnemonic::Uadd8
                | Mnemonic::Usub16 | Mnemonic::Usub8
                | Mnemonic::Qadd16 | Mnemonic::Qadd8
                | Mnemonic::Qsub16 | Mnemonic::Qsub8
                | Mnemonic::Shadd16 | Mnemonic::Shadd8
                | Mnemonic::Shsub16 | Mnemonic::Shsub8
                | Mnemonic::Uhadd16 | Mnemonic::Uhadd8
                | Mnemonic::Uhsub16 | Mnemonic::Uhsub8
                | Mnemonic::Uqadd16 | Mnemonic::Uqadd8
                | Mnemonic::Uqsub16 | Mnemonic::Uqsub8
                | Mnemonic::Sasx | Mnemonic::Ssax
                | Mnemonic::Uasx | Mnemonic::Usax
                | Mnemonic::Qasx | Mnemonic::Qsax
                | Mnemonic::Shasx | Mnemonic::Shsax
                | Mnemonic::Uhasx | Mnemonic::Uhsax
                | Mnemonic::Uqasx | Mnemonic::Uqsax
                // Saturating/packing/extend-add (always 32-bit)
                | Mnemonic::Qadd | Mnemonic::Qdadd
                | Mnemonic::Qsub | Mnemonic::Qdsub
                | Mnemonic::Pkhbt | Mnemonic::Pkhtb | Mnemonic::Sel
                | Mnemonic::Sxtab | Mnemonic::Sxtah
                | Mnemonic::Uxtab | Mnemonic::Uxtah
                | Mnemonic::Sxtab16 | Mnemonic::Uxtab16
                | Mnemonic::Sxtb16 | Mnemonic::Uxtb16
                => 4,
                // Narrow 16-bit instructions by default (may widen via fallback)
                _ => thumb::thumb_instruction_size(inst),
            }
        }
    }
}

fn handle_directive_pass1(dir: &Directive, state: &mut AsmState) -> Result<(), AsmError> {
    match dir {
        Directive::Section(name) => {
            // Flush pool before switching sections
            flush_pool(state);
            if let Some(idx) = state.sections.iter().position(|s| s.name == *name) {
                state.current_section = idx;
            } else {
                state.sections.push(SectionBuilder::new(name));
                state.current_section = state.sections.len() - 1;
            }
        }
        Directive::Global(name) => {
            state.globals.push(name.clone());
        }
        Directive::Align(power, _fill) => {
            let alignment = 1u32 << power;
            let sec = &mut state.sections[state.current_section];
            let padding = (alignment - (sec.offset % alignment)) % alignment;
            sec.offset += padding;
        }
        Directive::Balign(alignment, _fill) => {
            let sec = &mut state.sections[state.current_section];
            let padding = (alignment - (sec.offset % alignment)) % alignment;
            sec.offset += padding;
        }
        Directive::Word(vals) => {
            state.sections[state.current_section].offset += (vals.len() * 4) as u32;
        }
        Directive::Short(vals) => {
            state.sections[state.current_section].offset += (vals.len() * 2) as u32;
        }
        Directive::Byte(vals) => {
            state.sections[state.current_section].offset += vals.len() as u32;
        }
        Directive::Space(size, _fill) => {
            state.sections[state.current_section].offset += size;
        }
        Directive::Ascii(s) => {
            state.sections[state.current_section].offset += s.len() as u32;
        }
        Directive::Asciz(s) => {
            state.sections[state.current_section].offset += s.len() as u32 + 1;
        }
        Directive::Thumb => {
            state.isa = Isa::Thumb;
        }
        Directive::Arm => {
            state.isa = Isa::A32;
        }
        Directive::Equ(name, val) => {
            // Try to resolve; in pass1 some symbols may not exist yet, so ignore errors
            if let Ok(v) = resolve_expr(val, &state.symbols, &state.equs, &state.local_labels, state.current_section, state.sections[state.current_section].offset, 0) {
                state.equs.insert(name.clone(), v);
            }
        }
        Directive::Pool => {
            flush_pool(state);
        }
        Directive::SyntaxUnified | Directive::Type(_, _) | Directive::Fpu(_) => {}
    }
    Ok(())
}

fn handle_directive_pass2(dir: &Directive, line: usize, state: &mut AsmState) -> Result<(), AsmError> {
    match dir {
        Directive::Section(name) => {
            // Emit pool before switching sections
            emit_pool(state)?;
            if let Some(idx) = state.sections.iter().position(|s| s.name == *name) {
                state.current_section = idx;
            }
        }
        Directive::Align(power, fill) => {
            let alignment = 1u32 << power;
            let sec = &mut state.sections[state.current_section];
            let fill_byte = fill.unwrap_or(0);
            let padding = (alignment - (sec.offset % alignment)) % alignment;
            for _ in 0..padding {
                sec.data.push(fill_byte);
            }
            sec.offset += padding;
        }
        Directive::Balign(alignment, fill) => {
            let sec = &mut state.sections[state.current_section];
            let fill_byte = fill.unwrap_or(0);
            let padding = (alignment - (sec.offset % alignment)) % alignment;
            for _ in 0..padding {
                sec.data.push(fill_byte);
            }
            sec.offset += padding;
        }
        Directive::Word(vals) => {
            for v in vals {
                let offset = state.sections[state.current_section].offset;
                let resolved = resolve_expr(v, &state.symbols, &state.equs, &state.local_labels, state.current_section, offset, line)?;
                let sec = &mut state.sections[state.current_section];
                sec.data.extend_from_slice(&(resolved as u32).to_le_bytes());
                sec.offset += 4;
            }
        }
        Directive::Short(vals) => {
            for v in vals {
                let offset = state.sections[state.current_section].offset;
                let resolved = resolve_expr(v, &state.symbols, &state.equs, &state.local_labels, state.current_section, offset, line)?;
                let sec = &mut state.sections[state.current_section];
                sec.data.extend_from_slice(&(resolved as u16).to_le_bytes());
                sec.offset += 2;
            }
        }
        Directive::Byte(vals) => {
            for v in vals {
                let offset = state.sections[state.current_section].offset;
                let resolved = resolve_expr(v, &state.symbols, &state.equs, &state.local_labels, state.current_section, offset, line)?;
                let sec = &mut state.sections[state.current_section];
                sec.data.push(resolved as u8);
                sec.offset += 1;
            }
        }
        Directive::Space(size, fill) => {
            let sec = &mut state.sections[state.current_section];
            for _ in 0..*size {
                sec.data.push(*fill);
            }
            sec.offset += size;
        }
        Directive::Ascii(s) => {
            let sec = &mut state.sections[state.current_section];
            sec.data.extend_from_slice(s.as_bytes());
            sec.offset += s.len() as u32;
        }
        Directive::Asciz(s) => {
            let sec = &mut state.sections[state.current_section];
            sec.data.extend_from_slice(s.as_bytes());
            sec.data.push(0);
            sec.offset += s.len() as u32 + 1;
        }
        Directive::Thumb => {
            state.isa = Isa::Thumb;
        }
        Directive::Arm => {
            state.isa = Isa::A32;
        }
        Directive::Equ(name, val) => {
            let offset = state.sections[state.current_section].offset;
            let resolved = resolve_expr(val, &state.symbols, &state.equs, &state.local_labels, state.current_section, offset, line)?;
            state.equs.insert(name.clone(), resolved);
        }
        Directive::Pool => {
            emit_pool(state)?;
        }
        Directive::SyntaxUnified | Directive::Type(_, _) | Directive::Global(_)
        | Directive::Fpu(_) => {}
    }
    Ok(())
}

/// Convenience: resolve expression to u32 (for label/address contexts).
pub fn resolve_expr_u32(
    expr: &Expr,
    symbols: &HashMap<String, (usize, u32)>,
    equs: &HashMap<String, i64>,
    local_labels: &HashMap<u32, Vec<(usize, u32)>>,
    section: usize,
    offset: u32,
    line: usize,
) -> Result<u32, AsmError> {
    resolve_expr(expr, symbols, equs, local_labels, section, offset, line).map(|v| v as u32)
}

fn encode_instruction(
    inst: &Instruction,
    isa: Isa,
    offset: u32,
    symbols: &HashMap<String, (usize, u32)>,
    equs: &HashMap<String, i64>,
    local_labels: &HashMap<u32, Vec<(usize, u32)>>,
    section: usize,
    pool_map: &HashMap<(usize, u32), u32>,
) -> Result<EncodedInst, AsmError> {
    // Handle LDR =expr (literal pool pseudo-instruction)
    if let [Operand::Reg(rt), Operand::Pool(_)] = inst.operands.as_slice() {
        let pool_word_offset = *pool_map
            .get(&(section, offset))
            .ok_or_else(|| AsmError::new(inst.line, "internal error: pool entry not found"))?;
        // Rewrite as LDR Rt, <pool_address> (PC-relative) using the existing Expr path
        let mut rewritten = inst.clone();
        rewritten.operands[1] = Operand::Expr(Expr::Num(pool_word_offset as i64));
        // Force wide in Thumb only for high registers (low regs use narrow 2-byte encoding)
        if isa == Isa::Thumb && rt.value() > 7 {
            rewritten.wide = true;
        }
        return encode_instruction_inner(&rewritten, isa, offset, symbols, equs, local_labels, section);
    }
    encode_instruction_inner(inst, isa, offset, symbols, equs, local_labels, section)
}

fn encode_instruction_inner(
    inst: &Instruction,
    isa: Isa,
    offset: u32,
    symbols: &HashMap<String, (usize, u32)>,
    equs: &HashMap<String, i64>,
    local_labels: &HashMap<u32, Vec<(usize, u32)>>,
    section: usize,
) -> Result<EncodedInst, AsmError> {
    // VFP instructions share the same encoding for A32 and Thumb-2
    if vfp::is_vfp(inst.mnemonic) {
        return vfp::encode_vfp(inst, isa, offset, symbols, equs, local_labels, section);
    }
    match isa {
        Isa::Thumb => thumb::encode_thumb(inst, offset, symbols, equs, local_labels, section),
        Isa::A32 => a32::encode_a32(inst, offset, symbols, equs, local_labels, section),
    }
}

/// Resolve a label name to its absolute offset.
pub fn resolve_label(
    name: &str,
    symbols: &HashMap<String, (usize, u32)>,
    equs: &HashMap<String, i64>,
    line: usize,
) -> Result<u32, AsmError> {
    if let Some((_, offset)) = symbols.get(name) {
        Ok(*offset)
    } else if let Some(val) = equs.get(name) {
        Ok(*val as u32)
    } else {
        Err(AsmError::new(line, format!("undefined symbol: {name}")))
    }
}

/// Evaluate an expression to an i64, resolving any symbol references.
pub fn resolve_expr(
    expr: &Expr,
    symbols: &HashMap<String, (usize, u32)>,
    equs: &HashMap<String, i64>,
    local_labels: &HashMap<u32, Vec<(usize, u32)>>,
    section: usize,
    offset: u32,
    line: usize,
) -> Result<i64, AsmError> {
    let r = |e: &Expr| resolve_expr(e, symbols, equs, local_labels, section, offset, line);
    match expr {
        Expr::Num(n) => Ok(*n),
        Expr::Symbol(name) => resolve_label(name, symbols, equs, line).map(|v| v as i64),
        Expr::LocalLabel(n, forward) => {
            resolve_local_label(*n, *forward, local_labels, section, offset, line)
                .map(|v| v as i64)
        }
        Expr::Add(a, b) => Ok(r(a)? + r(b)?),
        Expr::Sub(a, b) => Ok(r(a)? - r(b)?),
        Expr::Mul(a, b) => Ok(r(a)? * r(b)?),
        Expr::Div(a, b) => {
            let divisor = r(b)?;
            if divisor == 0 {
                return Err(AsmError::new(line, "division by zero in expression"));
            }
            Ok(r(a)? / divisor)
        }
    }
}

/// Resolve a local (numeric) label reference to its absolute offset.
fn resolve_local_label(
    num: u32,
    forward: bool,
    local_labels: &HashMap<u32, Vec<(usize, u32)>>,
    section: usize,
    offset: u32,
    line: usize,
) -> Result<u32, AsmError> {
    let entries = local_labels
        .get(&num)
        .ok_or_else(|| AsmError::new(line, format!("undefined local label: {num}")))?;
    if forward {
        // First definition in the same section with offset > current
        entries
            .iter()
            .find(|(sec, off)| *sec == section && *off > offset)
            .map(|(_, off)| *off)
            .ok_or_else(|| AsmError::new(line, format!("no forward definition of local label {num}")))
    } else {
        // Last definition in the same section with offset <= current
        entries
            .iter()
            .rev()
            .find(|(sec, off)| *sec == section && *off <= offset)
            .map(|(_, off)| *off)
            .ok_or_else(|| AsmError::new(line, format!("no backward definition of local label {num}")))
    }
}
