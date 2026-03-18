use arbitrary_int::prelude::*;

use crate::ast::*;
use crate::error::AsmError;
use crate::lexer::{Token, TokenKind};

pub fn parse(tokens: &[Token]) -> Result<Vec<Statement>, AsmError> {
    let mut parser = Parser {
        tokens,
        pos: 0,
        if_stack: Vec::new(),
    };
    parser.parse_program()
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    /// Stack of `.if` condition states: true = emitting, false = skipping.
    if_stack: Vec<IfState>,
}

/// State of a single `.if`/`.else`/`.endif` block.
#[derive(Clone, Copy)]
struct IfState {
    /// Whether the `.if` condition was true (the "then" branch should emit).
    condition: bool,
    /// Whether we're currently in the `.else` branch.
    in_else: bool,
}

impl<'a> Parser<'a> {
    fn cur(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.cur().kind
    }

    /// Returns true if we should emit code (all `.if` conditions on the stack are active).
    fn emitting(&self) -> bool {
        self.if_stack.iter().all(|s| {
            if s.in_else {
                !s.condition
            } else {
                s.condition
            }
        })
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

            // Check for conditional assembly directives before anything else
            if let TokenKind::Ident(ref name) = self.peek_kind().clone() {
                match name.as_str() {
                    ".if" => {
                        self.advance();
                        let val = self.parse_number()?;
                        self.if_stack.push(IfState {
                            condition: val != 0,
                            in_else: false,
                        });
                        continue;
                    }
                    ".else" => {
                        let line = self.line();
                        self.advance();
                        let top = self.if_stack.last_mut().ok_or_else(|| {
                            AsmError::new(line, ".else without matching .if")
                        })?;
                        if top.in_else {
                            return Err(AsmError::new(line, "duplicate .else"));
                        }
                        top.in_else = true;
                        continue;
                    }
                    ".endif" => {
                        self.advance();
                        if self.if_stack.pop().is_none() {
                            return Err(AsmError::new(
                                self.line(),
                                ".endif without matching .if",
                            ));
                        }
                        continue;
                    }
                    _ => {}
                }
            }

            if self.emitting() {
                self.parse_line(&mut stmts)?;
            } else {
                // Skip this line
                while !self.at_end_of_statement() {
                    self.advance();
                }
            }
        }

        if !self.if_stack.is_empty() {
            return Err(AsmError::new(self.line(), "unterminated .if block"));
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

        // Check for numeric (local) label: number followed by colon
        if let TokenKind::Number(n) = *self.peek_kind() {
            if let Some(Token {
                kind: TokenKind::Colon,
                ..
            }) = self.tokens.get(self.pos + 1)
            {
                let label = n.to_string();
                let line = self.line();
                self.advance(); // number
                self.advance(); // colon
                stmts.push(Statement::Label(label, line));

                if self.at_end_of_statement() {
                    return Ok(());
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
            ".balign" => {
                let align = self.parse_number()? as u32;
                let fill = if self.eat(&TokenKind::Comma) {
                    Some(self.parse_number()? as u8)
                } else {
                    None
                };
                Ok(Directive::Balign(align, fill))
            }
            ".word" | ".long" | ".4byte" => {
                let vals = self.parse_expr_list()?;
                Ok(Directive::Word(vals))
            }
            ".short" | ".hword" | ".2byte" => {
                let vals = self.parse_expr_list()?;
                Ok(Directive::Short(vals))
            }
            ".byte" => {
                let vals = self.parse_expr_list()?;
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
                let val = self.parse_full_expr()?;
                Ok(Directive::Equ(name, val))
            }
            ".type" => {
                let name = self.parse_ident()?;
                self.expect(&TokenKind::Comma)?;
                let ty = self.parse_ident()?;
                Ok(Directive::Type(name, ty))
            }
            ".fpu" => {
                // .fpu names can contain hyphens (e.g. vfpv3-d16), so consume all tokens
                let mut name = String::new();
                while !self.at_end_of_statement() {
                    let tok = self.advance();
                    match &tok.kind {
                        TokenKind::Ident(s) => name.push_str(s),
                        TokenKind::Minus => name.push('-'),
                        TokenKind::Number(n) => name.push_str(&n.to_string()),
                        _ => break,
                    }
                }
                Ok(Directive::Fpu(name))
            }
            ".pool" | ".ltorg" => Ok(Directive::Pool),
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

        let (mnemonic, condition, set_flags, wide, fp_size, vcvt_kind) =
            parse_mnemonic(&raw, line)?;

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
                fp_size: None,
                vcvt_kind: None,
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
                fp_size,
                vcvt_kind,
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
                fp_size,
                vcvt_kind,
            });
        }

        // VMRS / VMSR — special operand parsing
        if mnemonic == Mnemonic::Vmrs {
            // VMRS Rd, FPSCR  or  VMRS APSR_nzcv, FPSCR
            let first = self.parse_vfp_or_core_operand()?;
            self.expect(&TokenKind::Comma)?;
            let second = self.parse_vfp_or_core_operand()?;
            return Ok(Instruction {
                mnemonic,
                condition,
                set_flags,
                wide,
                writeback: false,
                operands: vec![first, second],
                line,
                fp_size,
                vcvt_kind,
            });
        }
        if mnemonic == Mnemonic::Vmsr {
            // VMSR FPSCR, Rn
            let first = self.parse_vfp_or_core_operand()?;
            self.expect(&TokenKind::Comma)?;
            let second = self.parse_vfp_or_core_operand()?;
            return Ok(Instruction {
                mnemonic,
                condition,
                set_flags,
                wide,
                writeback: false,
                operands: vec![first, second],
                line,
                fp_size,
                vcvt_kind,
            });
        }

        let mut operands = Vec::new();
        let mut writeback = false;
        let is_vfp = is_vfp_mnemonic(mnemonic);
        while !self.at_end_of_statement() {
            if !operands.is_empty() {
                self.expect(&TokenKind::Comma)?;
            }
            let op = if is_vfp {
                self.parse_vfp_operand()?
            } else {
                self.parse_operand()?
            };
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
            fp_size,
            vcvt_kind,
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
            let amount = self.parse_number()?;
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
            TokenKind::Eq => {
                self.advance();
                let expr = self.parse_full_expr()?;
                Ok(Operand::Pool(expr))
            }
            TokenKind::Hash => {
                self.advance();
                let val = self.parse_number()?;
                Ok(Operand::Imm(val))
            }
            TokenKind::Minus => {
                self.advance();
                let val = self.parse_number()?;
                Ok(Operand::Imm(-val))
            }
            TokenKind::Ident(ref s) => {
                let s = s.clone();
                let upper = s.to_ascii_uppercase();
                // Check FP registers and special names
                if let Some(n) = parse_sreg(&upper) {
                    self.advance();
                    return Ok(Operand::SReg(n));
                }
                if let Some(n) = parse_dreg(&upper) {
                    self.advance();
                    return Ok(Operand::DReg(n));
                }
                if upper == "FPSCR" {
                    self.advance();
                    return Ok(Operand::Fpscr);
                }
                if upper == "APSR_NZCV" {
                    self.advance();
                    return Ok(Operand::ApsrNzcv);
                }
                if let Some(reg) = parse_register(&s) {
                    self.advance();
                    Ok(Operand::Reg(reg))
                } else {
                    let expr = self.parse_full_expr()?;
                    Ok(Operand::Expr(expr))
                }
            }
            TokenKind::Number(_) => {
                let expr = self.parse_full_expr()?;
                match expr {
                    Expr::Num(n) => Ok(Operand::Imm(n)),
                    _ => Ok(Operand::Expr(expr)),
                }
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
                    let val = self.parse_number()? as i32;
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
            let val = self.parse_number()? as i32;
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
        let expr = self.parse_full_expr()?;
        match expr {
            Expr::Num(n) => Ok(n),
            _ => Err(AsmError::new(self.line(), "expected numeric constant")),
        }
    }

    /// Parse a full expression that may contain symbols, arithmetic, and parens.
    /// Grammar: expr = term (('+' | '-') term)*
    fn parse_full_expr(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.parse_term()?;
        loop {
            if self.eat(&TokenKind::Plus) {
                let right = self.parse_term()?;
                left = Expr::Add(Box::new(left), Box::new(right));
            } else if self.eat(&TokenKind::Minus) {
                let right = self.parse_term()?;
                left = Expr::Sub(Box::new(left), Box::new(right));
            } else {
                break;
            }
        }
        Ok(left)
    }

    /// term = unary (('*' | '/') unary)*
    fn parse_term(&mut self) -> Result<Expr, AsmError> {
        let mut left = self.parse_unary()?;
        loop {
            if self.eat(&TokenKind::Star) {
                let right = self.parse_unary()?;
                left = Expr::Mul(Box::new(left), Box::new(right));
            } else if self.eat(&TokenKind::Slash) {
                let right = self.parse_unary()?;
                left = Expr::Div(Box::new(left), Box::new(right));
            } else {
                break;
            }
        }
        Ok(left)
    }

    /// unary = '-' unary | '+' unary | atom
    fn parse_unary(&mut self) -> Result<Expr, AsmError> {
        if self.eat(&TokenKind::Minus) {
            let inner = self.parse_unary()?;
            match inner {
                Expr::Num(n) => Ok(Expr::Num(-n)),
                _ => Ok(Expr::Sub(Box::new(Expr::Num(0)), Box::new(inner))),
            }
        } else {
            self.eat(&TokenKind::Plus);
            self.parse_atom()
        }
    }

    /// atom = Number | Ident | '(' expr ')'
    fn parse_atom(&mut self) -> Result<Expr, AsmError> {
        if self.eat(&TokenKind::LParen) {
            let expr = self.parse_full_expr()?;
            if !self.eat(&TokenKind::RParen) {
                return Err(AsmError::new(self.line(), "expected ')'"));
            }
            return Ok(expr);
        }
        match self.peek_kind().clone() {
            TokenKind::Number(n) => {
                // Check for local label reference: Number followed by Ident("f"/"b")
                if let Some(Token {
                    kind: TokenKind::Ident(ref dir),
                    ..
                }) = self.tokens.get(self.pos + 1)
                {
                    if (dir == "f" || dir == "b") && n >= 0 {
                        let is_forward = dir == "f";
                        self.advance(); // number
                        self.advance(); // f/b
                        return Ok(Expr::LocalLabel(n as u32, is_forward));
                    }
                }
                self.advance();
                Ok(Expr::Num(n))
            }
            TokenKind::Ident(ref s) => {
                let s = s.clone();
                self.advance();
                Ok(Expr::Symbol(s))
            }
            other => Err(AsmError::new(
                self.line(),
                format!("expected number or symbol, got {:?}", other),
            )),
        }
    }

    /// Parse an operand that can be an FP register, core register, FP register list,
    /// memory, FP immediate, or label/expression. Used for VFP instructions.
    fn parse_vfp_operand(&mut self) -> Result<Operand, AsmError> {
        match self.peek_kind().clone() {
            TokenKind::LBrace => self.parse_fp_reglist(),
            TokenKind::LBracket => self.parse_memory(),
            TokenKind::Hash => {
                self.advance();
                // Try to parse as floating-point immediate: #1.0, #-0.5, etc.
                // After '#', we may see a Number followed by an Ident starting with '.'
                // (because the lexer treats '.0' as an ident), or just a plain integer.
                let neg = self.eat(&TokenKind::Minus);
                match self.peek_kind().clone() {
                    TokenKind::Number(int_part) => {
                        self.advance();
                        // Check if next token is a dot-suffix like ".0", ".5", etc.
                        if let TokenKind::Ident(ref s) = self.peek_kind().clone() {
                            if s.starts_with('.') {
                                // Try to parse as float: combine int_part + suffix
                                let float_str = format!("{}{}", int_part, s);
                                if let Ok(val) = float_str.parse::<f64>() {
                                    self.advance(); // consume the dot-suffix ident
                                    let val = if neg { -val } else { val };
                                    return Ok(Operand::FpImm(val));
                                }
                            }
                        }
                        // Plain integer immediate
                        let val = if neg { -int_part } else { int_part };
                        Ok(Operand::Imm(val))
                    }
                    _ => {
                        let val = self.parse_number()?;
                        let val = if neg { -val } else { val };
                        Ok(Operand::Imm(val))
                    }
                }
            }
            TokenKind::Ident(ref s) => {
                let s = s.clone();
                let upper = s.to_ascii_uppercase();
                // FP registers
                if let Some(n) = parse_sreg(&upper) {
                    self.advance();
                    return Ok(Operand::SReg(n));
                }
                if let Some(n) = parse_dreg(&upper) {
                    self.advance();
                    return Ok(Operand::DReg(n));
                }
                // Special registers
                if upper == "FPSCR" {
                    self.advance();
                    return Ok(Operand::Fpscr);
                }
                if upper == "APSR_NZCV" {
                    self.advance();
                    return Ok(Operand::ApsrNzcv);
                }
                // Core registers
                if let Some(reg) = parse_register(&s) {
                    self.advance();
                    return Ok(Operand::Reg(reg));
                }
                // Label / expression
                let expr = self.parse_full_expr()?;
                Ok(Operand::Expr(expr))
            }
            TokenKind::Minus => {
                self.advance();
                let val = self.parse_number()?;
                Ok(Operand::Imm(-val))
            }
            TokenKind::Number(_) => {
                let expr = self.parse_full_expr()?;
                match expr {
                    Expr::Num(n) => Ok(Operand::Imm(n)),
                    _ => Ok(Operand::Expr(expr)),
                }
            }
            _ => Err(AsmError::new(
                self.line(),
                format!("unexpected token in VFP operand: {:?}", self.peek_kind()),
            )),
        }
    }

    /// Parse an operand for VMRS/VMSR: can be APSR_nzcv, FPSCR, or a core register.
    fn parse_vfp_or_core_operand(&mut self) -> Result<Operand, AsmError> {
        let line = self.line();
        match self.peek_kind().clone() {
            TokenKind::Ident(ref s) => {
                let s = s.clone();
                let upper = s.to_ascii_uppercase();
                if upper == "FPSCR" {
                    self.advance();
                    Ok(Operand::Fpscr)
                } else if upper == "APSR_NZCV" {
                    self.advance();
                    Ok(Operand::ApsrNzcv)
                } else if let Some(reg) = parse_register(&s) {
                    self.advance();
                    Ok(Operand::Reg(reg))
                } else {
                    Err(AsmError::new(
                        line,
                        format!("expected register, FPSCR, or APSR_nzcv, got '{s}'"),
                    ))
                }
            }
            _ => Err(AsmError::new(line, "expected register or special VFP operand")),
        }
    }

    /// Parse an FP register list: {S0-S3} or {S0, S1, S2} or {D0-D3} etc.
    /// Only contiguous ranges are valid for VPUSH/VPOP.
    fn parse_fp_reglist(&mut self) -> Result<Operand, AsmError> {
        self.expect(&TokenKind::LBrace)?;
        let line = self.line();

        // Determine if this is an S or D register list from the first register
        let (first_reg, is_double) = match self.peek_kind().clone() {
            TokenKind::Ident(ref s) => {
                let upper = s.to_ascii_uppercase();
                if let Some(n) = parse_sreg(&upper) {
                    self.advance();
                    (n, false)
                } else if let Some(n) = parse_dreg(&upper) {
                    self.advance();
                    (n, true)
                } else {
                    return Err(AsmError::new(line, "expected FP register in register list"));
                }
            }
            _ => return Err(AsmError::new(line, "expected FP register in register list")),
        };

        // Check for range: {S0-S3}
        if self.eat(&TokenKind::Minus) {
            let end_reg = match self.peek_kind().clone() {
                TokenKind::Ident(ref s) => {
                    let upper = s.to_ascii_uppercase();
                    let n = if is_double {
                        parse_dreg(&upper)
                    } else {
                        parse_sreg(&upper)
                    };
                    match n {
                        Some(r) => {
                            self.advance();
                            r
                        }
                        None => {
                            return Err(AsmError::new(
                                line,
                                "expected matching FP register type in range",
                            ))
                        }
                    }
                }
                _ => return Err(AsmError::new(line, "expected FP register after '-'")),
            };
            if end_reg < first_reg {
                return Err(AsmError::new(line, "invalid FP register range"));
            }
            self.expect(&TokenKind::RBrace)?;
            return Ok(Operand::FpRegList {
                start: first_reg,
                count: end_reg - first_reg + 1,
                double: is_double,
            });
        }

        // Comma-separated list: {S0, S1, S2} — must be contiguous
        let mut count: u8 = 1;
        let mut last_reg = first_reg;
        while self.eat(&TokenKind::Comma) {
            let next_reg = match self.peek_kind().clone() {
                TokenKind::Ident(ref s) => {
                    let upper = s.to_ascii_uppercase();
                    let n = if is_double {
                        parse_dreg(&upper)
                    } else {
                        parse_sreg(&upper)
                    };
                    match n {
                        Some(r) => {
                            self.advance();
                            r
                        }
                        None => {
                            return Err(AsmError::new(
                                line,
                                "expected matching FP register type in list",
                            ))
                        }
                    }
                }
                _ => return Err(AsmError::new(line, "expected FP register after ','")),
            };
            if next_reg != last_reg + 1 {
                return Err(AsmError::new(line, "FP register list must be contiguous"));
            }
            last_reg = next_reg;
            count += 1;
        }

        self.expect(&TokenKind::RBrace)?;
        Ok(Operand::FpRegList {
            start: first_reg,
            count,
            double: is_double,
        })
    }

    /// Parse a system register name.
    ///
    /// Encoding for M-profile registers: raw SYSm value (0..20).
    /// Encoding for A-profile CPSR/SPSR: bit 7 set, bit 4 = R (1=SPSR),
    /// bits 3:0 = field mask (c=1, x=2, s=4, f=8).
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

        // A-profile CPSR/SPSR with optional field mask suffix
        if let Some(code) = parse_cpsr_spsr(&full) {
            return Ok(code);
        }

        // M-profile system registers
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

    fn parse_expr_list(&mut self) -> Result<Vec<Expr>, AsmError> {
        let mut vals = vec![self.parse_full_expr()?];
        while self.eat(&TokenKind::Comma) {
            vals.push(self.parse_full_expr()?);
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

/// Parsed VFP type suffix information.
struct FpSuffix {
    fp_size: Option<FpSize>,
    vcvt_kind: Option<VcvtKind>,
}

/// Try to parse VFP type suffixes from a dot-separated tail string.
/// `tail` is the portion after the mnemonic base, e.g. ".F32" or ".F32.S32".
fn parse_fp_suffix(tail: &str) -> Option<FpSuffix> {
    // Split on dots (tail starts with '.', so first element is empty)
    let parts: Vec<&str> = tail.split('.').filter(|s| !s.is_empty()).collect();
    match parts.len() {
        1 => match parts[0] {
            "F32" => Some(FpSuffix {
                fp_size: Some(FpSize::F32),
                vcvt_kind: None,
            }),
            "F64" => Some(FpSuffix {
                fp_size: Some(FpSize::F64),
                vcvt_kind: None,
            }),
            _ => None,
        },
        2 => {
            // Double suffix for VCVT: .F32.S32, .F64.F32, etc.
            let kind = match (parts[0], parts[1]) {
                ("F32", "F64") => VcvtKind::F32ToF64,
                ("F64", "F32") => VcvtKind::F64ToF32,
                ("F32", "S32") => VcvtKind::F32ToS32,
                ("F32", "U32") => VcvtKind::F32ToU32,
                ("F64", "S32") => VcvtKind::F64ToS32,
                ("F64", "U32") => VcvtKind::F64ToU32,
                ("S32", "F32") => VcvtKind::S32ToF32,
                ("U32", "F32") => VcvtKind::U32ToF32,
                ("S32", "F64") => VcvtKind::S32ToF64,
                ("U32", "F64") => VcvtKind::U32ToF64,
                _ => return None,
            };
            // For double suffixes, fp_size is derived from the destination type (first suffix)
            let fp_size = match parts[0] {
                "F32" | "S32" | "U32" => Some(FpSize::F32),
                "F64" => Some(FpSize::F64),
                _ => None,
            };
            Some(FpSuffix {
                fp_size,
                vcvt_kind: Some(kind),
            })
        }
        _ => None,
    }
}

fn parse_mnemonic(
    raw: &str,
    line: usize,
) -> Result<(Mnemonic, Option<Condition>, bool, bool, Option<FpSize>, Option<VcvtKind>), AsmError> {
    let upper = raw.to_ascii_uppercase();

    // Split width suffix (.W, .N) — but only if not a VFP suffix.
    // VFP suffixes like .F32, .F64, .F32.S32 are handled separately.
    // Strategy: find the first dot. Everything before it is the "main" part
    // (which may contain condition codes etc). Everything from the first dot
    // onward is the suffix (.W, .N, .F32, .F64, .F32.S32, etc).
    let (main, wide, fp_size, vcvt_kind) = if let Some(dot_pos) = upper.find('.') {
        let tail = &upper[dot_pos..]; // e.g. ".F32" or ".F32.S32" or ".W"
        let base = &upper[..dot_pos];
        match tail {
            ".W" => (base, true, None, None),
            ".N" => (base, false, None, None),
            _ => {
                if let Some(fps) = parse_fp_suffix(tail) {
                    (base, false, fps.fp_size, fps.vcvt_kind)
                } else {
                    return Err(AsmError::new(
                        line,
                        format!("unknown suffix: {tail}"),
                    ));
                }
            }
        }
    } else {
        (upper.as_str(), false, None, None)
    };

    // Special case: IT block (IT, ITE, ITT, ITTE, ITET, etc.)
    // The T/E pattern is part of the mnemonic, not condition codes.
    if main.len() >= 2 && main.starts_with("IT") {
        let suffix = &main[2..];
        if suffix.is_empty() || (suffix.len() <= 3 && suffix.chars().all(|c| c == 'T' || c == 'E'))
        {
            return Ok((Mnemonic::It, None, false, wide, fp_size, vcvt_kind));
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
                    return Ok((mnemonic, Some(cond), true, wide, fp_size, vcvt_kind));
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
                return Ok((mnemonic, Some(cond), false, wide, fp_size, vcvt_kind));
            }
        }
    }

    // Try: base + S
    if main.len() >= 2 && main.ends_with('S') {
        let base = &main[..main.len() - 1];
        if let Some(mnemonic) = lookup_mnemonic(base) {
            return Ok((mnemonic, None, true, wide, fp_size, vcvt_kind));
        }
    }

    // Try: just base
    if let Some(mnemonic) = lookup_mnemonic(main) {
        return Ok((mnemonic, None, false, wide, fp_size, vcvt_kind));
    }

    Err(AsmError::new(line, format!("unknown instruction: {raw}")))
}

fn lookup_mnemonic(s: &str) -> Option<Mnemonic> {
    MNEMONICS
        .iter()
        .find(|(name, _)| *name == s)
        .map(|(_, m)| *m)
}

/// Parse a single-precision register name (S0-S31). Input must be uppercase.
fn parse_sreg(s: &str) -> Option<u8> {
    if s.starts_with('S') && s.len() >= 2 {
        let num: u8 = s[1..].parse().ok()?;
        if num <= 31 {
            return Some(num);
        }
    }
    None
}

/// Parse a double-precision register name (D0-D31). Input must be uppercase.
fn parse_dreg(s: &str) -> Option<u8> {
    if s.starts_with('D') && s.len() >= 2 {
        let num: u8 = s[1..].parse().ok()?;
        if num <= 31 {
            return Some(num);
        }
    }
    None
}

/// Parse A-profile CPSR/SPSR with optional field mask suffix.
/// Returns encoded u8: bit 7 set (A-profile), bit 4 = R (1=SPSR),
/// bits 3:0 = field mask (c=1, x=2, s=4, f=8).
fn parse_cpsr_spsr(name: &str) -> Option<u8> {
    let (base, rest) = if let Some(suffix) = name.strip_prefix("CPSR") {
        (0x80u8, suffix) // R=0
    } else if let Some(suffix) = name.strip_prefix("SPSR") {
        (0x90u8, suffix) // R=1
    } else {
        return None;
    };

    if rest.is_empty() {
        // Bare CPSR/SPSR — all fields
        return Some(base | 0x0F);
    }

    // Must have underscore then field letters: _f, _fs, _cxsf, etc.
    let fields = rest.strip_prefix('_')?;
    if fields.is_empty() {
        return None;
    }
    let mut mask = 0u8;
    for ch in fields.chars() {
        match ch {
            'C' => mask |= 1, // control
            'X' => mask |= 2, // extension
            'S' => mask |= 4, // status
            'F' => mask |= 8, // flags
            _ => return None,
        }
    }
    Some(base | mask)
}

/// Whether a mnemonic is a VFP instruction (uses FP register operands).
fn is_vfp_mnemonic(m: Mnemonic) -> bool {
    matches!(
        m,
        Mnemonic::Vadd
            | Mnemonic::Vsub
            | Mnemonic::Vmul
            | Mnemonic::Vdiv
            | Mnemonic::Vsqrt
            | Mnemonic::Vabs
            | Mnemonic::Vneg
            | Mnemonic::Vmov
            | Mnemonic::Vcmp
            | Mnemonic::Vcmpe
            | Mnemonic::Vcvt
            | Mnemonic::Vcvtr
            | Mnemonic::Vldr
            | Mnemonic::Vstr
            | Mnemonic::Vpush
            | Mnemonic::Vpop
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;

    fn parse_one(src: &str) -> Instruction {
        let tokens = tokenize(src).unwrap();
        let stmts = parse(&tokens).unwrap();
        match stmts.into_iter().next().unwrap() {
            Statement::Instruction(inst) => inst,
            other => panic!("expected instruction, got {:?}", other),
        }
    }

    #[test]
    fn vfp_vadd_f32() {
        let inst = parse_one("VADD.F32 S0, S1, S2\n");
        assert_eq!(inst.mnemonic, Mnemonic::Vadd);
        assert_eq!(inst.fp_size, Some(FpSize::F32));
        assert_eq!(inst.operands, vec![Operand::SReg(0), Operand::SReg(1), Operand::SReg(2)]);
    }

    #[test]
    fn vfp_vadd_f64() {
        let inst = parse_one("VADD.F64 D0, D1, D2\n");
        assert_eq!(inst.mnemonic, Mnemonic::Vadd);
        assert_eq!(inst.fp_size, Some(FpSize::F64));
        assert_eq!(inst.operands, vec![Operand::DReg(0), Operand::DReg(1), Operand::DReg(2)]);
    }

    #[test]
    fn vfp_conditional() {
        let inst = parse_one("VADDNE.F64 D0, D1, D2\n");
        assert_eq!(inst.mnemonic, Mnemonic::Vadd);
        assert_eq!(inst.condition, Some(Condition::Ne));
        assert_eq!(inst.fp_size, Some(FpSize::F64));
    }

    #[test]
    fn vfp_vcvt_double_suffix() {
        let inst = parse_one("VCVT.F32.S32 S0, S1\n");
        assert_eq!(inst.mnemonic, Mnemonic::Vcvt);
        assert_eq!(inst.vcvt_kind, Some(VcvtKind::F32ToS32));

        let inst = parse_one("VCVT.S32.F64 S0, D1\n");
        assert_eq!(inst.vcvt_kind, Some(VcvtKind::S32ToF64));

        let inst = parse_one("VCVT.F64.F32 D0, S1\n");
        assert_eq!(inst.vcvt_kind, Some(VcvtKind::F64ToF32));
    }

    #[test]
    fn vfp_vpush_range() {
        let inst = parse_one("VPUSH {S0-S3}\n");
        assert_eq!(inst.mnemonic, Mnemonic::Vpush);
        assert_eq!(
            inst.operands,
            vec![Operand::FpRegList { start: 0, count: 4, double: false }]
        );
    }

    #[test]
    fn vfp_vpop_dregs() {
        let inst = parse_one("VPOP {D4-D7}\n");
        assert_eq!(inst.mnemonic, Mnemonic::Vpop);
        assert_eq!(
            inst.operands,
            vec![Operand::FpRegList { start: 4, count: 4, double: true }]
        );
    }

    #[test]
    fn vfp_vpush_comma_list() {
        let inst = parse_one("VPUSH {S0, S1, S2}\n");
        assert_eq!(
            inst.operands,
            vec![Operand::FpRegList { start: 0, count: 3, double: false }]
        );
    }

    #[test]
    fn vfp_vmrs_apsr() {
        let inst = parse_one("VMRS APSR_nzcv, FPSCR\n");
        assert_eq!(inst.mnemonic, Mnemonic::Vmrs);
        assert_eq!(inst.operands, vec![Operand::ApsrNzcv, Operand::Fpscr]);
    }

    #[test]
    fn vfp_vmrs_reg() {
        let inst = parse_one("VMRS R0, FPSCR\n");
        assert_eq!(inst.operands[0], Operand::Reg(u4::new(0)));
        assert_eq!(inst.operands[1], Operand::Fpscr);
    }

    #[test]
    fn vfp_vmsr() {
        let inst = parse_one("VMSR FPSCR, R0\n");
        assert_eq!(inst.operands[0], Operand::Fpscr);
        assert_eq!(inst.operands[1], Operand::Reg(u4::new(0)));
    }

    #[test]
    fn vfp_vldr_memory() {
        let inst = parse_one("VLDR S0, [R1, #8]\n");
        assert_eq!(inst.mnemonic, Mnemonic::Vldr);
        assert_eq!(inst.operands[0], Operand::SReg(0));
        assert!(matches!(inst.operands[1], Operand::Memory { .. }));
    }

    #[test]
    fn vfp_vmov_fp_imm() {
        let inst = parse_one("VMOV.F32 S0, #1.0\n");
        assert_eq!(inst.mnemonic, Mnemonic::Vmov);
        assert_eq!(inst.fp_size, Some(FpSize::F32));
        assert_eq!(inst.operands[0], Operand::SReg(0));
        assert_eq!(inst.operands[1], Operand::FpImm(1.0));
    }

    #[test]
    fn vfp_vmov_neg_fp_imm() {
        let inst = parse_one("VMOV.F32 S0, #-0.5\n");
        assert_eq!(inst.operands[1], Operand::FpImm(-0.5));
    }

    #[test]
    fn vfp_vcmp_f32() {
        let inst = parse_one("VCMP.F32 S0, S1\n");
        assert_eq!(inst.mnemonic, Mnemonic::Vcmp);
        assert_eq!(inst.fp_size, Some(FpSize::F32));
        assert_eq!(inst.operands, vec![Operand::SReg(0), Operand::SReg(1)]);
    }

    #[test]
    fn vfp_sreg_boundaries() {
        let inst = parse_one("VMOV.F32 S31, S0\n");
        assert_eq!(inst.operands[0], Operand::SReg(31));
    }

    #[test]
    fn vfp_dreg_boundaries() {
        let inst = parse_one("VMOV.F64 D15, D0\n");
        assert_eq!(inst.operands[0], Operand::DReg(15));
    }

    #[test]
    fn vfp_vcmpe_f64() {
        let inst = parse_one("VCMPE.F64 D0, D1\n");
        assert_eq!(inst.mnemonic, Mnemonic::Vcmpe);
        assert_eq!(inst.fp_size, Some(FpSize::F64));
    }

    #[test]
    fn vfp_vsqrt() {
        let inst = parse_one("VSQRT.F32 S0, S1\n");
        assert_eq!(inst.mnemonic, Mnemonic::Vsqrt);
        assert_eq!(inst.fp_size, Some(FpSize::F32));
    }

    #[test]
    fn vfp_vmov_core_to_sreg() {
        // VMOV S0, R0 — core register to FP register
        let inst = parse_one("VMOV S0, R0\n");
        assert_eq!(inst.mnemonic, Mnemonic::Vmov);
        assert_eq!(inst.operands[0], Operand::SReg(0));
        assert_eq!(inst.operands[1], Operand::Reg(u4::new(0)));
    }

    #[test]
    fn existing_sp_still_works() {
        // Make sure SP is not confused with S-register parsing
        let inst = parse_one("MOV R0, SP\n");
        assert_eq!(inst.operands[1], Operand::Reg(u4::new(13)));
    }

    #[test]
    fn vfp_vcvt_u32_f32() {
        let inst = parse_one("VCVT.U32.F32 S0, S1\n");
        assert_eq!(inst.vcvt_kind, Some(VcvtKind::U32ToF32));
    }

    #[test]
    fn vfp_no_suffix() {
        // VPUSH/VPOP don't need .F32/.F64
        let inst = parse_one("VPUSH {D0-D3}\n");
        assert_eq!(inst.fp_size, None);
        assert_eq!(inst.mnemonic, Mnemonic::Vpush);
    }

    #[test]
    fn vfp_vcvtr() {
        let inst = parse_one("VCVTR.S32.F32 S0, S1\n");
        assert_eq!(inst.mnemonic, Mnemonic::Vcvtr);
        assert_eq!(inst.vcvt_kind, Some(VcvtKind::S32ToF32));
    }
}
