use arbitrary_int::prelude::*;

use crate::ast::*;
use crate::error::AsmError;
use crate::lexer::{Token, TokenKind};

pub fn parse(tokens: &[Token]) -> Result<Vec<Statement>, AsmError> {
    let mut parser = Parser { tokens, pos: 0 };
    parser.parse_program()
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn cur(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.cur().kind
    }

    fn line(&self) -> usize {
        self.cur().line
    }

    fn advance(&mut self) -> &Token {
        let t = &self.tokens[self.pos.min(self.tokens.len() - 1)];
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn expect(&mut self, kind: &TokenKind) -> Result<(), AsmError> {
        if std::mem::discriminant(self.peek_kind()) == std::mem::discriminant(kind) {
            self.advance();
            Ok(())
        } else {
            Err(AsmError::new(
                self.line(),
                format!("expected {:?}, got {:?}", kind, self.peek_kind()),
            ))
        }
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if std::mem::discriminant(self.peek_kind()) == std::mem::discriminant(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn at_end_of_statement(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Eof)
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek_kind(), TokenKind::Newline) {
            self.advance();
        }
    }

    fn parse_program(&mut self) -> Result<Vec<Statement>, AsmError> {
        let mut stmts = Vec::new();

        loop {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::Eof) {
                break;
            }
            self.parse_line(&mut stmts)?;
        }

        Ok(stmts)
    }

    fn parse_line(&mut self, stmts: &mut Vec<Statement>) -> Result<(), AsmError> {
        // Check for label: ident followed by colon
        if let TokenKind::Ident(ref name) = self.peek_kind().clone() {
            if !name.starts_with('.') {
                if let Some(Token {
                    kind: TokenKind::Colon,
                    ..
                }) = self.tokens.get(self.pos + 1)
                {
                    let label = name.clone();
                    let line = self.line();
                    self.advance(); // ident
                    self.advance(); // colon
                    stmts.push(Statement::Label(label, line));

                    if self.at_end_of_statement() {
                        return Ok(());
                    }
                    // Fall through to parse instruction/directive on same line
                }
            }
        }

        // Check for directive
        if let TokenKind::Ident(ref name) = self.peek_kind().clone() {
            if name.starts_with('.') {
                let line = self.line();
                let dir = self.parse_directive()?;
                stmts.push(Statement::Directive(dir, line));
                return Ok(());
            }
        }

        // Parse instruction
        if !self.at_end_of_statement() {
            let inst = self.parse_instruction()?;
            stmts.push(Statement::Instruction(inst));
        }

        Ok(())
    }

    fn parse_directive(&mut self) -> Result<Directive, AsmError> {
        let name = match self.advance().kind.clone() {
            TokenKind::Ident(s) => s.to_ascii_lowercase(),
            _ => return Err(AsmError::new(self.line(), "expected directive")),
        };

        match name.as_str() {
            ".text" => Ok(Directive::Section(".text".into())),
            ".data" => Ok(Directive::Section(".data".into())),
            ".bss" => Ok(Directive::Section(".bss".into())),
            ".section" => {
                let sec = self.parse_ident()?;
                Ok(Directive::Section(sec))
            }
            ".global" | ".globl" => {
                let sym = self.parse_ident()?;
                Ok(Directive::Global(sym))
            }
            ".align" | ".p2align" => {
                let align = self.parse_number()? as u32;
                let fill = if self.eat(&TokenKind::Comma) {
                    Some(self.parse_number()? as u8)
                } else {
                    None
                };
                Ok(Directive::Align(align, fill))
            }
            ".word" | ".long" | ".4byte" => {
                let vals = self.parse_number_list()?;
                Ok(Directive::Word(vals))
            }
            ".short" | ".hword" | ".2byte" => {
                let vals = self.parse_number_list()?;
                Ok(Directive::Short(vals))
            }
            ".byte" => {
                let vals = self.parse_number_list()?;
                Ok(Directive::Byte(vals))
            }
            ".space" | ".skip" => {
                let size = self.parse_number()? as u32;
                let fill = if self.eat(&TokenKind::Comma) {
                    self.parse_number()? as u8
                } else {
                    0
                };
                Ok(Directive::Space(size, fill))
            }
            ".ascii" => {
                let s = self.parse_string()?;
                Ok(Directive::Ascii(s))
            }
            ".asciz" | ".string" => {
                let s = self.parse_string()?;
                Ok(Directive::Asciz(s))
            }
            ".thumb" => Ok(Directive::Thumb),
            ".arm" | ".code32" => Ok(Directive::Arm),
            ".syntax" => {
                let syntax = self.parse_ident()?;
                if syntax.to_ascii_lowercase() == "unified" {
                    Ok(Directive::SyntaxUnified)
                } else {
                    Err(AsmError::new(
                        self.line(),
                        "only .syntax unified is supported",
                    ))
                }
            }
            ".equ" | ".set" => {
                let name = self.parse_ident()?;
                self.expect(&TokenKind::Comma)?;
                let val = self.parse_expr()?;
                Ok(Directive::Equ(name, val))
            }
            ".type" => {
                let name = self.parse_ident()?;
                self.expect(&TokenKind::Comma)?;
                let ty = self.parse_ident()?;
                Ok(Directive::Type(name, ty))
            }
            ".thumb_func" | ".fnstart" | ".fnend" | ".cantunwind" | ".size" | ".ident"
            | ".file" => {
                // Skip to end of line (ignore these directives)
                while !self.at_end_of_statement() {
                    self.advance();
                }
                Ok(Directive::SyntaxUnified) // no-op placeholder
            }
            _ => {
                // Skip unknown directives
                while !self.at_end_of_statement() {
                    self.advance();
                }
                Ok(Directive::SyntaxUnified) // no-op placeholder
            }
        }
    }

    fn parse_instruction(&mut self) -> Result<Instruction, AsmError> {
        let line = self.line();
        let raw = match self.advance().kind.clone() {
            TokenKind::Ident(s) => s,
            _ => return Err(AsmError::new(line, "expected instruction mnemonic")),
        };

        let (mnemonic, condition, set_flags, wide) = parse_mnemonic(&raw, line)?;

        // Special handling for IT blocks: parse T/E pattern and condition operand
        if mnemonic == Mnemonic::It {
            let upper = raw.to_ascii_uppercase();
            // Strip .W/.N suffix if present
            let main = if let Some(dot_pos) = upper.rfind('.') {
                &upper[..dot_pos]
            } else {
                upper.as_str()
            };
            let it_pattern = &main[2..]; // chars after "IT"

            // Parse the condition code (next token)
            let cond_str = match self.advance().kind.clone() {
                TokenKind::Ident(s) => s.to_ascii_uppercase(),
                _ => return Err(AsmError::new(line, "IT: expected condition code")),
            };
            let cond = Condition::from_str(&cond_str).ok_or_else(|| {
                AsmError::new(line, format!("IT: unknown condition '{cond_str}'"))
            })?;

            let firstcond = cond.raw_value().value();
            let n = it_pattern.len();
            let mut mask: u8 = 0;
            for (i, ch) in it_pattern.chars().enumerate() {
                let bit_pos = 3 - i;
                let bit = match ch {
                    'T' => firstcond & 1,
                    'E' => (firstcond & 1) ^ 1,
                    _ => return Err(AsmError::new(line, "IT: invalid then/else pattern")),
                };
                mask |= bit << bit_pos;
            }
            // Set terminator bit
            mask |= 1 << (3 - n);

            return Ok(Instruction {
                mnemonic,
                condition: Some(cond),
                set_flags: false,
                wide,
                writeback: false,
                operands: vec![Operand::Imm(mask as i64)],
                line,
            });
        }

        // MRS Rd, sysreg  / MSR sysreg, Rn — special operand parsing
        if mnemonic == Mnemonic::Mrs {
            let rd = self.parse_reg()?;
            self.expect(&TokenKind::Comma)?;
            let sysm = self.parse_sysreg()?;
            return Ok(Instruction {
                mnemonic,
                condition,
                set_flags,
                wide,
                writeback: false,
                operands: vec![Operand::Reg(rd), Operand::SysReg(sysm)],
                line,
            });
        }
        if mnemonic == Mnemonic::Msr {
            let sysm = self.parse_sysreg()?;
            self.expect(&TokenKind::Comma)?;
            let rn = self.parse_reg()?;
            return Ok(Instruction {
                mnemonic,
                condition,
                set_flags,
                wide,
                writeback: false,
                operands: vec![Operand::SysReg(sysm), Operand::Reg(rn)],
                line,
            });
        }

        let mut operands = Vec::new();
        let mut writeback = false;
        while !self.at_end_of_statement() {
            if !operands.is_empty() {
                self.expect(&TokenKind::Comma)?;
            }
            let op = self.parse_operand()?;
            operands.push(op);

            // Check for ! (writeback) after a register operand (for LDM/STM)
            if let Some(&Operand::Reg(_)) = operands.last() {
                if self.eat(&TokenKind::Bang) {
                    writeback = true;
                }
            }

            // Check for shift suffix on the last register operand
            if let Some(&Operand::Reg(reg)) = operands.last() {
                if self.peek_is_comma_shift() {
                    self.advance(); // consume comma
                    let shifted = self.parse_shift(reg)?;
                    *operands.last_mut().unwrap() = shifted;
                }
            }
        }

        Ok(Instruction {
            mnemonic,
            condition,
            set_flags,
            wide,
            writeback,
            operands,
            line,
        })
    }

    fn peek_is_comma_shift(&self) -> bool {
        if !matches!(self.peek_kind(), TokenKind::Comma) {
            return false;
        }
        if let Some(t) = self.tokens.get(self.pos + 1) {
            if let TokenKind::Ident(ref s) = t.kind {
                return is_shift_type(s);
            }
        }
        false
    }

    fn parse_shift(&mut self, reg: u4) -> Result<Operand, AsmError> {
        let shift_str = match self.advance().kind.clone() {
            TokenKind::Ident(s) => s,
            _ => return Err(AsmError::new(self.line(), "expected shift type")),
        };
        let shift_type = match shift_str.to_ascii_uppercase().as_str() {
            "LSL" => ShiftType::Lsl,
            "LSR" => ShiftType::Lsr,
            "ASR" => ShiftType::Asr,
            "ROR" => ShiftType::Ror,
            "RRX" => {
                return Ok(Operand::Shifted(
                    reg,
                    ShiftType::Rrx,
                    Box::new(Operand::Imm(0)),
                ))
            }
            _ => return Err(AsmError::new(self.line(), "unknown shift type")),
        };

        // Parse shift amount: #imm or register
        if self.eat(&TokenKind::Hash) {
            let amount = self.parse_expr()?;
            Ok(Operand::Shifted(
                reg,
                shift_type,
                Box::new(Operand::Imm(amount)),
            ))
        } else {
            let amount_reg = self.parse_reg()?;
            Ok(Operand::Shifted(
                reg,
                shift_type,
                Box::new(Operand::Reg(amount_reg)),
            ))
        }
    }

    fn parse_operand(&mut self) -> Result<Operand, AsmError> {
        match self.peek_kind().clone() {
            TokenKind::LBrace => self.parse_reglist(),
            TokenKind::LBracket => self.parse_memory(),
            TokenKind::Hash => {
                self.advance();
                let val = self.parse_expr()?;
                Ok(Operand::Imm(val))
            }
            TokenKind::Minus => {
                self.advance();
                let val = self.parse_number()?;
                Ok(Operand::Imm(-val))
            }
            TokenKind::Ident(ref s) => {
                let s = s.clone();
                if let Some(reg) = parse_register(&s) {
                    self.advance();
                    Ok(Operand::Reg(reg))
                } else {
                    self.advance();
                    Ok(Operand::Label(s))
                }
            }
            TokenKind::Number(n) => {
                let n = n;
                self.advance();
                Ok(Operand::Imm(n))
            }
            _ => Err(AsmError::new(
                self.line(),
                format!("unexpected token in operand: {:?}", self.peek_kind()),
            )),
        }
    }

    fn parse_memory(&mut self) -> Result<Operand, AsmError> {
        self.expect(&TokenKind::LBracket)?;
        let base = self.parse_reg()?;

        if self.eat(&TokenKind::RBracket) {
            // [Rn]! or [Rn], post-index
            if self.eat(&TokenKind::Bang) {
                return Ok(Operand::Memory {
                    base,
                    offset: MemOffset::Imm(0),
                    pre_index: true,
                    writeback: true,
                });
            }
            // Check for post-index: [Rn], #imm or [Rn], Rm{, shift #amt}
            if self.eat(&TokenKind::Comma) {
                let (offset, neg) = if self.eat(&TokenKind::Hash) {
                    let val = self.parse_expr()? as i32;
                    (MemOffset::Imm(val), false)
                } else {
                    let neg = self.eat(&TokenKind::Minus);
                    let rm = self.parse_reg()?;
                    // Check for shift on register offset
                    if self.eat(&TokenKind::Comma) {
                        let shift_str = self.parse_ident()?;
                        let shift_type = match shift_str.to_ascii_uppercase().as_str() {
                            "LSL" => ShiftType::Lsl,
                            "LSR" => ShiftType::Lsr,
                            "ASR" => ShiftType::Asr,
                            "ROR" => ShiftType::Ror,
                            _ => return Err(AsmError::new(self.line(), "expected shift type")),
                        };
                        self.eat(&TokenKind::Hash);
                        let amount = self.parse_number()? as u8;
                        (MemOffset::RegShift(rm, shift_type, amount, neg), false)
                    } else {
                        (MemOffset::Reg(rm, neg), false)
                    }
                };
                let offset = if neg {
                    match offset {
                        MemOffset::Imm(v) => MemOffset::Imm(-v),
                        other => other,
                    }
                } else {
                    offset
                };
                return Ok(Operand::Memory {
                    base,
                    offset,
                    pre_index: false,
                    writeback: true,
                });
            }
            return Ok(Operand::Memory {
                base,
                offset: MemOffset::Imm(0),
                pre_index: true,
                writeback: false,
            });
        }

        self.expect(&TokenKind::Comma)?;

        let (offset, neg) = if self.eat(&TokenKind::Hash) {
            let val = self.parse_expr()? as i32;
            (MemOffset::Imm(val), false)
        } else {
            let neg = self.eat(&TokenKind::Minus);
            let rm = self.parse_reg()?;
            // Check for shift on register offset
            if self.eat(&TokenKind::Comma) {
                let shift_str = self.parse_ident()?;
                let shift_type = match shift_str.to_ascii_uppercase().as_str() {
                    "LSL" => ShiftType::Lsl,
                    "LSR" => ShiftType::Lsr,
                    "ASR" => ShiftType::Asr,
                    "ROR" => ShiftType::Ror,
                    _ => return Err(AsmError::new(self.line(), "expected shift type")),
                };
                self.eat(&TokenKind::Hash);
                let amount = self.parse_number()? as u8;
                (MemOffset::RegShift(rm, shift_type, amount, neg), false)
            } else {
                (MemOffset::Reg(rm, neg), false)
            }
        };

        self.expect(&TokenKind::RBracket)?;
        let writeback = self.eat(&TokenKind::Bang);

        // Apply negation for immediate offsets
        let offset = if neg {
            match offset {
                MemOffset::Imm(v) => MemOffset::Imm(-v),
                other => other,
            }
        } else {
            offset
        };

        Ok(Operand::Memory {
            base,
            offset,
            pre_index: true,
            writeback,
        })
    }

    fn parse_reglist(&mut self) -> Result<Operand, AsmError> {
        self.expect(&TokenKind::LBrace)?;
        let mut mask: u16 = 0;

        loop {
            let reg = self.parse_reg()?;
            if self.eat(&TokenKind::Minus) {
                let end_reg = self.parse_reg()?;
                for r in reg.value()..=end_reg.value() {
                    mask |= 1 << r;
                }
            } else {
                mask |= 1 << reg.value();
            }

            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }

        self.expect(&TokenKind::RBrace)?;
        Ok(Operand::RegList(mask))
    }

    fn parse_reg(&mut self) -> Result<u4, AsmError> {
        let line = self.line();
        match self.advance().kind.clone() {
            TokenKind::Ident(s) => parse_register(&s)
                .ok_or_else(|| AsmError::new(line, format!("expected register, got '{s}'"))),
            _ => Err(AsmError::new(line, "expected register")),
        }
    }

    fn parse_ident(&mut self) -> Result<String, AsmError> {
        match self.advance().kind.clone() {
            TokenKind::Ident(s) => Ok(s),
            _ => Err(AsmError::new(self.line(), "expected identifier")),
        }
    }

    fn parse_string(&mut self) -> Result<String, AsmError> {
        match self.advance().kind.clone() {
            TokenKind::StringLit(s) => Ok(s),
            _ => Err(AsmError::new(self.line(), "expected string literal")),
        }
    }

    fn parse_number(&mut self) -> Result<i64, AsmError> {
        self.parse_expr()
    }

    fn parse_expr(&mut self) -> Result<i64, AsmError> {
        let neg = if self.eat(&TokenKind::Minus) {
            true
        } else {
            self.eat(&TokenKind::Plus);
            false
        };

        let val = match self.advance().kind.clone() {
            TokenKind::Number(n) => n,
            other => {
                return Err(AsmError::new(
                    self.line(),
                    format!("expected number, got {:?}", other),
                ))
            }
        };

        Ok(if neg { -val } else { val })
    }

    fn parse_sysreg(&mut self) -> Result<u8, AsmError> {
        let name = self.parse_ident()?;
        let upper = name.to_ascii_uppercase();
        // Consume optional _suffix (e.g. APSR_nzcvq)
        let full = upper.clone();
        while self.eat(&TokenKind::Ident("_".into())) || {
            // check for underscore-prefixed token (lexer may split at _)
            false
        } {
            // shouldn't normally happen since _ is part of ident
        }
        // Handle APSR_nzcvq etc by checking if the ident already contains underscore
        let code = match full.as_str() {
            "APSR" | "APSR_NZCVQ" => 0,
            "IAPSR" => 1,
            "EAPSR" => 2,
            "XPSR" => 3,
            "IPSR" => 5,
            "EPSR" => 6,
            "IEPSR" => 7,
            "MSP" => 8,
            "PSP" => 9,
            "PRIMASK" => 16,
            "BASEPRI" => 17,
            "BASEPRI_MAX" => 18,
            "FAULTMASK" => 19,
            "CONTROL" => 20,
            _ => {
                return Err(AsmError::new(
                    self.line(),
                    format!("unknown system register: {name}"),
                ))
            }
        };
        Ok(code)
    }

    fn parse_number_list(&mut self) -> Result<Vec<i64>, AsmError> {
        let mut vals = vec![self.parse_expr()?];
        while self.eat(&TokenKind::Comma) {
            vals.push(self.parse_expr()?);
        }
        Ok(vals)
    }
}

fn parse_register(s: &str) -> Option<u4> {
    match s.to_ascii_uppercase().as_str() {
        "R0" => Some(u4::new(0)),
        "R1" => Some(u4::new(1)),
        "R2" => Some(u4::new(2)),
        "R3" => Some(u4::new(3)),
        "R4" => Some(u4::new(4)),
        "R5" => Some(u4::new(5)),
        "R6" => Some(u4::new(6)),
        "R7" => Some(u4::new(7)),
        "R8" => Some(u4::new(8)),
        "R9" => Some(u4::new(9)),
        "R10" => Some(u4::new(10)),
        "R11" | "FP" => Some(u4::new(11)),
        "R12" | "IP" => Some(u4::new(12)),
        "R13" | "SP" => Some(u4::new(13)),
        "R14" | "LR" => Some(u4::new(14)),
        "R15" | "PC" => Some(u4::new(15)),
        _ => None,
    }
}

fn is_shift_type(s: &str) -> bool {
    matches!(
        s.to_ascii_uppercase().as_str(),
        "LSL" | "LSR" | "ASR" | "ROR" | "RRX"
    )
}

fn parse_mnemonic(
    raw: &str,
    line: usize,
) -> Result<(Mnemonic, Option<Condition>, bool, bool), AsmError> {
    let upper = raw.to_ascii_uppercase();

    // Split width suffix (.W, .N)
    let (main, wide) = if let Some(dot_pos) = upper.rfind('.') {
        let suffix = &upper[dot_pos + 1..];
        match suffix {
            "W" => (&upper[..dot_pos], true),
            "N" => (&upper[..dot_pos], false),
            _ => {
                return Err(AsmError::new(
                    line,
                    format!("unknown width suffix: .{suffix}"),
                ))
            }
        }
    } else {
        (upper.as_str(), false)
    };

    // Special case: IT block (IT, ITE, ITT, ITTE, ITET, etc.)
    // The T/E pattern is part of the mnemonic, not condition codes.
    if main.len() >= 2 && main.starts_with("IT") {
        let suffix = &main[2..];
        if suffix.is_empty() || (suffix.len() <= 3 && suffix.chars().all(|c| c == 'T' || c == 'E'))
        {
            return Ok((Mnemonic::It, None, false, wide));
        }
    }

    // Try: base + cond + S
    if main.len() >= 4 && main.ends_with('S') {
        let without_s = &main[..main.len() - 1];
        if without_s.len() >= 3 {
            let cond_str = &without_s[without_s.len() - 2..];
            if let Some(cond) = Condition::from_str(cond_str) {
                let base = &without_s[..without_s.len() - 2];
                if let Some(mnemonic) = lookup_mnemonic(base) {
                    return Ok((mnemonic, Some(cond), true, wide));
                }
            }
        }
    }

    // Try: base + cond
    if main.len() >= 3 {
        let cond_str = &main[main.len() - 2..];
        if let Some(cond) = Condition::from_str(cond_str) {
            let base = &main[..main.len() - 2];
            if let Some(mnemonic) = lookup_mnemonic(base) {
                return Ok((mnemonic, Some(cond), false, wide));
            }
        }
    }

    // Try: base + S
    if main.len() >= 2 && main.ends_with('S') {
        let base = &main[..main.len() - 1];
        if let Some(mnemonic) = lookup_mnemonic(base) {
            return Ok((mnemonic, None, true, wide));
        }
    }

    // Try: just base
    if let Some(mnemonic) = lookup_mnemonic(main) {
        return Ok((mnemonic, None, false, wide));
    }

    Err(AsmError::new(line, format!("unknown instruction: {raw}")))
}

fn lookup_mnemonic(s: &str) -> Option<Mnemonic> {
    MNEMONICS
        .iter()
        .find(|(name, _)| *name == s)
        .map(|(_, m)| *m)
}
