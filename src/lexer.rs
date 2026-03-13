use crate::error::AsmError;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Ident(String),
    Number(i64),
    StringLit(String),
    Comma,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Hash,
    Colon,
    Plus,
    Minus,
    Bang,
    Newline,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
}

pub fn tokenize(input: &str) -> Result<Vec<Token>, AsmError> {
    let mut tokens = Vec::new();
    let mut line = 1;

    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        // Skip whitespace (not newlines)
        if ch == ' ' || ch == '\t' || ch == '\r' {
            i += 1;
            continue;
        }

        // Newline
        if ch == '\n' {
            // Collapse multiple newlines and don't emit if last token was newline
            if tokens
                .last()
                .map_or(true, |t: &Token| t.kind != TokenKind::Newline)
            {
                tokens.push(Token {
                    kind: TokenKind::Newline,
                    line,
                });
            }
            line += 1;
            i += 1;
            continue;
        }

        // Line comment: @ or // or ;
        if ch == '@' || ch == ';' || (ch == '/' && i + 1 < chars.len() && chars[i + 1] == '/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }

        // Block comment /* ... */
        if ch == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                if chars[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            if i + 1 < chars.len() {
                i += 2; // skip */
            }
            continue;
        }

        // String literal
        if ch == '"' {
            i += 1;
            let mut s = String::new();
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 1;
                    match chars[i] {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        'r' => s.push('\r'),
                        '0' => s.push('\0'),
                        '\\' => s.push('\\'),
                        '"' => s.push('"'),
                        _ => {
                            s.push('\\');
                            s.push(chars[i]);
                        }
                    }
                } else {
                    s.push(chars[i]);
                }
                i += 1;
            }
            if i < chars.len() {
                i += 1; // skip closing "
            }
            tokens.push(Token {
                kind: TokenKind::StringLit(s),
                line,
            });
            continue;
        }

        // Identifiers: [a-zA-Z_.][a-zA-Z0-9_.]*
        if ch.is_ascii_alphabetic() || ch == '_' || ch == '.' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.')
            {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            tokens.push(Token {
                kind: TokenKind::Ident(s),
                line,
            });
            continue;
        }

        // Numbers: decimal, 0x hex, 0b binary
        if ch.is_ascii_digit() {
            let start = i;
            if ch == '0' && i + 1 < chars.len() {
                let next = chars[i + 1].to_ascii_lowercase();
                if next == 'x' {
                    i += 2;
                    while i < chars.len() && chars[i].is_ascii_hexdigit() {
                        i += 1;
                    }
                    let s: String = chars[start + 2..i].iter().collect();
                    let val = i64::from_str_radix(&s, 16)
                        .map_err(|_| AsmError::new(line, format!("invalid hex literal: 0x{s}")))?;
                    tokens.push(Token {
                        kind: TokenKind::Number(val),
                        line,
                    });
                    continue;
                } else if next == 'b' {
                    i += 2;
                    while i < chars.len() && (chars[i] == '0' || chars[i] == '1') {
                        i += 1;
                    }
                    let s: String = chars[start + 2..i].iter().collect();
                    let val = i64::from_str_radix(&s, 2).map_err(|_| {
                        AsmError::new(line, format!("invalid binary literal: 0b{s}"))
                    })?;
                    tokens.push(Token {
                        kind: TokenKind::Number(val),
                        line,
                    });
                    continue;
                }
            }
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            let val = s
                .parse::<i64>()
                .map_err(|_| AsmError::new(line, format!("invalid number: {s}")))?;
            tokens.push(Token {
                kind: TokenKind::Number(val),
                line,
            });
            continue;
        }

        // Punctuation
        let kind = match ch {
            ',' => TokenKind::Comma,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '#' => TokenKind::Hash,
            ':' => TokenKind::Colon,
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '!' => TokenKind::Bang,
            _ => return Err(AsmError::new(line, format!("unexpected character: '{ch}'"))),
        };
        tokens.push(Token { kind, line });
        i += 1;
    }

    // Ensure we end with newline + eof
    if tokens.last().map_or(true, |t| t.kind != TokenKind::Newline) {
        tokens.push(Token {
            kind: TokenKind::Newline,
            line,
        });
    }
    tokens.push(Token {
        kind: TokenKind::Eof,
        line,
    });

    Ok(tokens)
}
