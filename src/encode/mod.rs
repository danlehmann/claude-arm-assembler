mod a32;
mod thumb;

use std::collections::HashMap;

use crate::ast::*;
use crate::error::AsmError;
use crate::{AsmConfig, AsmOutput, Section, Symbol};

struct AsmState {
    sections: Vec<SectionBuilder>,
    current_section: usize,
    isa: Isa,
    symbols: HashMap<String, (usize, u32)>, // (section_index, offset)
    globals: Vec<String>,
    equs: HashMap<String, i64>,
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
    };

    // Pass 1: collect labels, compute sizes
    pass1(stmts, &mut state)?;

    // Reset for pass 2
    for sec in &mut state.sections {
        sec.offset = 0;
    }
    state.current_section = 0;
    state.isa = config.default_isa;

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
                state.symbols.insert(
                    name.clone(),
                    (
                        state.current_section,
                        state.sections[state.current_section].offset,
                    ),
                );
            }
            Statement::Instruction(inst) => {
                let size = instruction_size(inst, state.isa);
                state.sections[state.current_section].offset += size;
            }
            Statement::Directive(dir, _line) => {
                handle_directive_pass1(dir, state)?;
            }
        }
    }
    Ok(())
}

fn pass2(stmts: &[Statement], state: &mut AsmState) -> Result<(), AsmError> {
    for stmt in stmts {
        match stmt {
            Statement::Label(_, _) => {}
            Statement::Instruction(inst) => {
                let offset = state.sections[state.current_section].offset;
                let bytes =
                    encode_instruction(inst, state.isa, offset, &state.symbols, &state.equs)?;
                debug_assert_eq!(
                    bytes.len() as u32,
                    instruction_size(inst, state.isa),
                    "instruction size mismatch at line {}: predicted {} bytes, encoded {}",
                    inst.line,
                    instruction_size(inst, state.isa),
                    bytes.len()
                );
                let sec = &mut state.sections[state.current_section];
                sec.data.extend_from_slice(&bytes);
                sec.offset += bytes.len() as u32;
            }
            Statement::Directive(dir, _line) => {
                handle_directive_pass2(dir, state)?;
            }
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
                | Mnemonic::Smlal | Mnemonic::Umlal
                | Mnemonic::Clz | Mnemonic::Rbit
                | Mnemonic::Bfi | Mnemonic::Bfc | Mnemonic::Ubfx | Mnemonic::Sbfx
                | Mnemonic::Ldrd | Mnemonic::Strd
                | Mnemonic::Ldrex | Mnemonic::Strex
                | Mnemonic::Ldrexb | Mnemonic::Strexb
                | Mnemonic::Ldrexh | Mnemonic::Strexh | Mnemonic::Clrex
                | Mnemonic::Mrs | Mnemonic::Msr
                | Mnemonic::Tbb | Mnemonic::Tbh
                | Mnemonic::Ssat | Mnemonic::Usat
                | Mnemonic::Ldrt | Mnemonic::Strt
                | Mnemonic::Ldrbt | Mnemonic::Strbt
                | Mnemonic::Ldrht | Mnemonic::Strht
                | Mnemonic::Ldrsbt | Mnemonic::Ldrsht
                | Mnemonic::Pld | Mnemonic::Pli
                | Mnemonic::Dbg | Mnemonic::Rrx
                // DSP multiply (always 32-bit)
                | Mnemonic::Smmul | Mnemonic::Smmla | Mnemonic::Smmls
                | Mnemonic::Smulbb | Mnemonic::Smulbt
                | Mnemonic::Smultb | Mnemonic::Smultt
                | Mnemonic::Smlabb | Mnemonic::Smlabt
                | Mnemonic::Smlatb | Mnemonic::Smlatt
                | Mnemonic::Smlalbb | Mnemonic::Smlalbt
                | Mnemonic::Smlaltb | Mnemonic::Smlaltt
                | Mnemonic::Smuad | Mnemonic::Smusd
                | Mnemonic::Smlad | Mnemonic::Smlsd
                | Mnemonic::Smlald | Mnemonic::Smlsld
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
            state.equs.insert(name.clone(), *val);
        }
        Directive::SyntaxUnified | Directive::Type(_, _) => {}
    }
    Ok(())
}

fn handle_directive_pass2(dir: &Directive, state: &mut AsmState) -> Result<(), AsmError> {
    match dir {
        Directive::Section(name) => {
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
        Directive::Word(vals) => {
            let sec = &mut state.sections[state.current_section];
            for v in vals {
                sec.data.extend_from_slice(&(*v as u32).to_le_bytes());
            }
            sec.offset += (vals.len() * 4) as u32;
        }
        Directive::Short(vals) => {
            let sec = &mut state.sections[state.current_section];
            for v in vals {
                sec.data.extend_from_slice(&(*v as u16).to_le_bytes());
            }
            sec.offset += (vals.len() * 2) as u32;
        }
        Directive::Byte(vals) => {
            let sec = &mut state.sections[state.current_section];
            for v in vals {
                sec.data.push(*v as u8);
            }
            sec.offset += vals.len() as u32;
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
            state.equs.insert(name.clone(), *val);
        }
        Directive::SyntaxUnified | Directive::Type(_, _) | Directive::Global(_) => {}
    }
    Ok(())
}

fn encode_instruction(
    inst: &Instruction,
    isa: Isa,
    offset: u32,
    symbols: &HashMap<String, (usize, u32)>,
    equs: &HashMap<String, i64>,
) -> Result<Vec<u8>, AsmError> {
    match isa {
        Isa::Thumb => thumb::encode_thumb(inst, offset, symbols, equs),
        Isa::A32 => a32::encode_a32(inst, offset, symbols, equs),
    }
}

/// Resolve a label operand to its absolute offset.
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
