use crate::meta::{
    ast::{Assoc, Expr, Grammar, Prec, Rule, RuleTy},
    error::GrammarError,
};

pub fn parse_grammar(src: &str) -> Result<Grammar, GrammarError> {
    let mut parser = MetaParser::new(src);
    parser.parse()
}

struct MetaParser<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> MetaParser<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    fn error_at(&self, pos: usize, message: impl Into<String>) -> GrammarError {
        GrammarError::new(message, pos, self.src)
    }

    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn skip_ws_and_comments(&mut self) -> Result<(), GrammarError> {
        loop {
            let rest = &self.src[self.pos..];
            if rest.starts_with("//") {
                if let Some(nl) = rest.find('\n') {
                    self.pos += nl + 1;
                } else {
                    self.pos = self.src.len();
                }
            } else if rest.starts_with("/*") {
                if let Some(end) = rest.find("*/") {
                    self.pos += end + 2;
                } else {
                    return Err(self.error_at(self.pos, "unclosed block comment"));
                }
            } else {
                let mut chars = rest.chars();
                if let Some(c) = chars.next() {
                    if c == ' ' || c == '\t' || c == '\r' || c == '\n' {
                        self.pos += c.len_utf8();
                        continue;
                    }
                }
                break;
            }
        }
        Ok(())
    }

    fn eat_char(&mut self, expected: char) -> Result<bool, GrammarError> {
        self.skip_ws_and_comments()?;
        if self.peek() == Some(expected) {
            self.bump();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn parse_ident(&mut self) -> Result<(String, (usize, usize)), GrammarError> {
        self.skip_ws_and_comments()?;
        let start = self.pos;
        let mut chars = self.src[self.pos..].chars();
        let first = chars
            .next()
            .ok_or_else(|| self.error_at(start, "expected identifier, found end of file"))?;
        if !(first.is_ascii_alphabetic() || first == '_') {
            return Err(self.error_at(start, format!("expected identifier, found '{}'", first)));
        }
        let mut len = first.len_utf8();
        for c in chars {
            if c.is_ascii_alphanumeric() || c == '_' {
                len += c.len_utf8();
            } else {
                break;
            }
        }
        self.pos += len;
        let end = self.pos;
        let name = self.src[start..end].to_string();
        Ok((name, (start, end)))
    }

    fn parse_u32(&mut self, context: &str) -> Result<u32, GrammarError> {
        self.skip_ws_and_comments()?;
        let start = self.pos;
        let mut chars = self.src[self.pos..].chars();
        let first = chars
            .next()
            .ok_or_else(|| self.error_at(start, format!("expected integer for {}, found end of file", context)))?;
        if !first.is_ascii_digit() {
            return Err(self.error_at(start, format!("expected integer for {}, found '{}'", context, first)));
        }
        let mut len = first.len_utf8();
        for c in chars {
            if c.is_ascii_digit() {
                len += c.len_utf8();
            } else {
                break;
            }
        }
        self.pos += len;
        let s = &self.src[start..self.pos];
        s.parse::<u32>()
            .map_err(|_| self.error_at(start, format!("integer overflow in {}", context)))
    }

    fn parse_escape(&mut self) -> Result<char, GrammarError> {
        let esc_start = self.pos;
        self.bump(); // consume '\\'
        let ch = match self.peek() {
            Some(c) => c,
            None => return Err(self.error_at(esc_start, "unexpected end of file in escape sequence")),
        };
        self.bump();
        match ch {
            'n' => Ok('\n'),
            'r' => Ok('\r'),
            't' => Ok('\t'),
            '\\' => Ok('\\'),
            '"' => Ok('"'),
            '\'' => Ok('\''),
            '0' => Ok('\0'),
            'x' => {
                let h1 = self.parse_hex_digit(esc_start)?;
                let h2 = self.parse_hex_digit(esc_start)?;
                let byte = (h1 << 4) | h2;
                Ok(byte as char)
            }
            'u' => {
                if self.peek() != Some('{') {
                    return Err(self.error_at(esc_start, "expected '{' after '\\u'"));
                }
                self.bump();
                let mut val: u32 = 0;
                let mut digit_count = 0;
                loop {
                    match self.peek() {
                        Some('}') => {
                            self.bump();
                            break;
                        }
                        Some(c) if c.is_ascii_hexdigit() => {
                            self.bump();
                            val = (val << 4) | c.to_digit(16).unwrap();
                            digit_count += 1;
                            if digit_count > 6 {
                                return Err(
                                    self.error_at(esc_start, "too many digits in unicode escape")
                                );
                            }
                        }
                        Some(_) => {
                            return Err(self
                                .error_at(self.pos, "expected hex digit or '}' in unicode escape"));
                        }
                        None => {
                            return Err(self
                                .error_at(esc_start, "unexpected end of file in unicode escape"));
                        }
                    }
                }
                if digit_count == 0 {
                    return Err(self.error_at(esc_start, "empty unicode escape"));
                }
                char::from_u32(val).ok_or_else(|| {
                    self.error_at(esc_start, "invalid unicode escape code point")
                })
            }
            _ => Err(self.error_at(esc_start, format!("invalid escape sequence '\\{}'", ch))),
        }
    }

    fn parse_hex_digit(&mut self, esc_start: usize) -> Result<u8, GrammarError> {
        match self.peek() {
            Some(c) if c.is_ascii_hexdigit() => {
                self.bump();
                Ok(c.to_digit(16).unwrap() as u8)
            }
            Some(c) => Err(self.error_at(
                self.pos,
                format!("expected hex digit in escape sequence, found '{}'", c),
            )),
            None => Err(self.error_at(
                esc_start,
                "unexpected end of file in hex escape sequence",
            )),
        }
    }

    fn parse_char_literal(&mut self) -> Result<char, GrammarError> {
        let start = self.pos;
        if !self.eat_char('\'')? {
            return Err(self.error_at(start, "expected single quote for character literal"));
        }
        let ch = match self.peek() {
            Some('\\') => self.parse_escape()?,
            Some('\'') => {
                return Err(self.error_at(start, "empty character literal"));
            }
            Some('\n') | Some('\r') => {
                return Err(self.error_at(start, "newline in character literal"));
            }
            Some(c) => {
                self.bump();
                c
            }
            None => return Err(self.error_at(start, "unexpected end of file in character literal")),
        };
        if self.peek() != Some('\'') {
            return Err(self.error_at(self.pos, "expected closing single quote"));
        }
        self.bump();
        Ok(ch)
    }

    fn parse_string_content(&mut self, start: usize) -> Result<String, GrammarError> {
        let mut s = String::new();
        loop {
            match self.peek() {
                Some('"') => {
                    self.bump();
                    return Ok(s);
                }
                Some('\\') => {
                    s.push(self.parse_escape()?);
                }
                Some('\n') | Some('\r') => {
                    return Err(self.error_at(start, "newline in string literal"));
                }
                Some(c) => {
                    self.bump();
                    s.push(c);
                }
                None => {
                    return Err(self.error_at(start, "unexpected end of file in string literal"))
                }
            }
        }
    }

    fn parse(&mut self) -> Result<Grammar, GrammarError> {
        let mut rules = Vec::new();
        self.skip_ws_and_comments()?;
        while self.pos < self.src.len() {
            rules.push(self.parse_rule()?);
            self.skip_ws_and_comments()?;
        }
        Ok(Grammar { rules })
    }

    fn parse_rule(&mut self) -> Result<Rule, GrammarError> {
        let (name, (start, _)) = self.parse_ident()?;
        self.skip_ws_and_comments()?;

        let prec = if self.eat_char('[')? {
            self.skip_ws_and_comments()?;
            let level = self.parse_u32("precedence level")?;
            self.skip_ws_and_comments()?;
            let assoc = if self.eat_char(',')? {
                self.skip_ws_and_comments()?;
                let (assoc_ident, (assoc_start, _)) = self.parse_ident()?;
                match assoc_ident.as_str() {
                    "L" => Some(Assoc::Left),
                    "R" => Some(Assoc::Right),
                    _ => {
                        return Err(self.error_at(
                            assoc_start,
                            format!(
                                "expected 'L' or 'R' for associativity, found '{}'",
                                assoc_ident
                            ),
                        ));
                    }
                }
            } else {
                None
            };
            self.skip_ws_and_comments()?;
            if !self.eat_char(']')? {
                return Err(self.error_at(self.pos, "expected ']' after precedence annotation"));
            }
            Some(Prec { level, assoc })
        } else {
            None
        };

        self.skip_ws_and_comments()?;
        if !self.eat_char('=')? {
            return Err(self.error_at(self.pos, format!("expected '=' after rule name '{}'", name)));
        }
        self.skip_ws_and_comments()?;

        let ty = match self.peek() {
            Some('{') => RuleTy::Normal,
            Some('_') => {
                self.bump();
                RuleTy::Silent
            }
            Some('@') => {
                self.bump();
                RuleTy::Atomic
            }
            Some('$') => {
                self.bump();
                RuleTy::CompoundAtomic
            }
            Some('!') => {
                self.bump();
                RuleTy::NonAtomic
            }
            Some(c) => {
                return Err(self.error_at(
                    self.pos,
                    format!("expected rule modifier or '{{', found '{}'", c),
                ))
            }
            None => return Err(self.error_at(self.pos, "expected rule body, found end of file")),
        };

        self.skip_ws_and_comments()?;
        if !self.eat_char('{')? {
            return Err(self.error_at(self.pos, "expected '{' before rule body"));
        }

        let expr = self.parse_expr()?;
        self.skip_ws_and_comments()?;
        if !self.eat_char('}')? {
            return Err(self.error_at(self.pos, "expected '}' after rule body"));
        }
        let end = self.pos;

        Ok(Rule {
            name,
            prec,
            ty,
            expr,
            span: (start, end),
        })
    }

    fn parse_expr(&mut self) -> Result<Expr, GrammarError> {
        self.parse_choice()
    }

    fn parse_choice(&mut self) -> Result<Expr, GrammarError> {
        let first = self.parse_seq()?;
        let mut choices = Vec::new();
        self.skip_ws_and_comments()?;
        if self.peek() == Some('|') {
            choices.push(first);
            while self.eat_char('|')? {
                choices.push(self.parse_seq()?);
                self.skip_ws_and_comments()?;
            }
            Ok(Expr::Choice(choices))
        } else {
            Ok(first)
        }
    }

    fn parse_seq(&mut self) -> Result<Expr, GrammarError> {
        let first = self.parse_prefix()?;
        let mut items = Vec::new();
        self.skip_ws_and_comments()?;
        if self.peek() == Some('~') {
            items.push(first);
            while self.eat_char('~')? {
                items.push(self.parse_prefix()?);
                self.skip_ws_and_comments()?;
            }
            Ok(Expr::Seq(items))
        } else {
            Ok(first)
        }
    }

    fn parse_prefix(&mut self) -> Result<Expr, GrammarError> {
        self.skip_ws_and_comments()?;
        if self.eat_char('&')? {
            let inner = self.parse_prefix()?;
            Ok(Expr::PosPred(Box::new(inner)))
        } else if self.eat_char('!')? {
            let inner = self.parse_prefix()?;
            Ok(Expr::NegPred(Box::new(inner)))
        } else {
            self.parse_postfix()
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, GrammarError> {
        let mut expr = self.parse_atom()?;
        loop {
            self.skip_ws_and_comments()?;
            match self.peek() {
                Some('?') => {
                    self.bump();
                    expr = Expr::Opt(Box::new(expr));
                }
                Some('*') => {
                    self.bump();
                    expr = Expr::Rep(Box::new(expr));
                }
                Some('+') => {
                    self.bump();
                    expr = Expr::RepOnce(Box::new(expr));
                }
                Some('{') => {
                    expr = self.parse_repetition_bounds(expr)?;
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_repetition_bounds(&mut self, expr: Expr) -> Result<Expr, GrammarError> {
        let start = self.pos;
        self.bump(); // consume '{'
        self.skip_ws_and_comments()?;
        if self.eat_char(',')? {
            // {, n}
            self.skip_ws_and_comments()?;
            let n = self.parse_u32("maximum repetition")?;
            self.skip_ws_and_comments()?;
            if !self.eat_char('}')? {
                return Err(self.error_at(self.pos, "expected '}' after repetition bound"));
            }
            Ok(Expr::RepMax(Box::new(expr), n))
        } else {
            let first = self.parse_u32("repetition bound")?;
            self.skip_ws_and_comments()?;
            if self.eat_char(',')? {
                self.skip_ws_and_comments()?;
                if self.eat_char('}')? {
                    // {n, }
                    Ok(Expr::RepMin(Box::new(expr), first))
                } else {
                    // {m, n}
                    let second = self.parse_u32("maximum repetition")?;
                    self.skip_ws_and_comments()?;
                    if !self.eat_char('}')? {
                        return Err(self.error_at(self.pos, "expected '}' after repetition bound"));
                    }
                    Ok(Expr::RepMinMax(Box::new(expr), first, second))
                }
            } else {
                if !self.eat_char('}')? {
                    return Err(self.error_at(start, "expected ',' or '}' in repetition bound"));
                }
                Ok(Expr::RepExact(Box::new(expr), first))
            }
        }
    }

    fn parse_atom(&mut self) -> Result<Expr, GrammarError> {
        self.skip_ws_and_comments()?;
        let start = self.pos;
        match self.peek() {
            Some('(') => {
                self.bump();
                let inner = self.parse_expr()?;
                self.skip_ws_and_comments()?;
                if !self.eat_char(')')? {
                    return Err(self.error_at(self.pos, "expected ')'"));
                }
                Ok(inner)
            }
            Some('"') => {
                self.bump();
                let s = self.parse_string_content(start)?;
                Ok(Expr::Str(s))
            }
            Some('^') => {
                self.bump();
                if self.peek() != Some('"') {
                    return Err(self.error_at(self.pos, "expected '\"' after '^'"));
                }
                self.bump();
                let s = self.parse_string_content(start)?;
                Ok(Expr::Insens(s))
            }
            Some('\'') => {
                let c1 = self.parse_char_literal()?;
                self.skip_ws_and_comments()?;
                let range_start = self.pos;
                if self.src[self.pos..].starts_with("..") {
                    self.pos += 2;
                    self.skip_ws_and_comments()?;
                    let c2 = self.parse_char_literal()?;
                    Ok(Expr::Range(c1, c2))
                } else {
                    Err(self.error_at(range_start, "expected '..' after character literal"))
                }
            }
            Some('#') => {
                self.bump();
                self.skip_ws_and_comments()?;
                let (tag_name, _) = self.parse_ident()?;
                self.skip_ws_and_comments()?;
                if !self.eat_char('=')? {
                    return Err(self.error_at(self.pos, "expected '=' after node tag"));
                }
                self.skip_ws_and_comments()?;
                let tagged_expr = self.parse_postfix()?;
                Ok(Expr::Tag(Box::new(tagged_expr), tag_name))
            }
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                let (ident, span) = self.parse_ident()?;
                self.skip_ws_and_comments()?;
                const STACK_OPS: &[&str] = &[
                    "PUSH", "PUSH_LITERAL", "POP", "POP_ALL", "PEEK", "PEEK_ALL", "DROP",
                ];
                if STACK_OPS.contains(&ident.as_str()) && self.peek() == Some('(') {
                    self.bump();
                    let mut depth = 1;
                    while depth > 0 && self.pos < self.src.len() {
                        match self.bump() {
                            Some('(') => depth += 1,
                            Some(')') => depth -= 1,
                            Some('"') => {
                                let _ = self.parse_string_content(self.pos);
                            }
                            Some('\'') => {
                                let _ = self.parse_char_literal();
                            }
                            _ => {}
                        }
                    }
                }
                Ok(Expr::Ident(ident, span))
            }
            Some(c) => Err(self.error_at(start, format!("expected expression, found '{}'", c))),
            None => Err(self.error_at(start, "expected expression, found end of file")),
        }
    }
}
