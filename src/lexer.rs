use std::borrow::Cow;
use std::iter::Peekable;
use std::str::CharIndices;

use crate::consts;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Token<'a> {
    Begin, 
    End,      
    Let, 
    If,
    Else,
    While, 
    For,
    In,
    Func,
    Inc, 
    Dec, 
    LBracket,
    RBracket, 
    TypeNumber,
    TypeFloat,
    FatArrow,
    UnderScope,
    Match,
    TypeBool,
    TypeStr,
    TypeChar, 
    Number(i64),
    Char(char), 
    Float(f64), 
    Bool(bool), 
    Some, 
    Ok, 
    Err, 
    None,
    String(&'a str),
    Ident(&'a str), 
    Assign,
    AssignOper(usize),
    Pow, 
    Plus, 
    Minus, 
    Div,
    Arrow,
    Colon,
    Equal, 
    Less,
    Greater, 
    Mult, 
    LessOrEqual, 
    GreaterOrEqual,
    Mod, 
    LogicalAnd, 
    ArifmAnd, 
    Or,
    ArifmOr,
    Not, 
    NotEqual, 
    LParen, 
    RParen, 
    Comma,
    Semicolon, 
    Eof,
    Dot, 
    DotDot,
}

pub struct Lexer<'a> {
    input: &'a str,
    chars: Peekable<CharIndices<'a>>,
    pos: usize,
    line: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            input: text,
            chars: text.char_indices().peekable(),
            line: 0,
            pos: 0,
        }
    }

    pub fn get_pos(&self) -> (usize, usize) {
        (self.line, self.pos)
    }

    #[inline(always)]
    fn bump(&mut self) -> Option<(usize, char)> {
        let res = self.chars.next();
        if let Some((_, c)) = res {
            if c == '\n' {
                self.pos = 0;
                self.line += 1;
            } else {
                self.pos += 1;
            }
        }
        res
    }

    #[inline(always)]
    fn next(&mut self) -> Option<char> {
        self.bump().map(|(_, c)| c)
    }

    #[inline(always)]
    fn current_byte_pos(&mut self) -> usize {
        self.chars.peek().map_or(self.input.len(), |&(pos, _)| pos)
    }
    
    #[inline(always)]
    fn match_mext_if_else(&mut self, expected: char, if_t: Token<'a>, els_t: Token<'a>) -> Token<'a> {
        if self.match_next(expected) {
            if_t
        } else {
            els_t
        }
    }

    #[inline(always)]
    fn match_next(&mut self, expected: char) -> bool {
        if let Some(&(_, c)) = self.chars.peek() && c == expected {
            self.bump();
            return true;
        }
        false
    }

    pub fn next_token(&mut self) -> Token<'a> {
        self.skip_whitespace();

        let (start_pos, ch) = match self.bump() {
            Some(res) => res,
            None => return Token::Eof,
        };

        if ch == '"' {
            return self.read_string(start_pos);
        } else if ch == '\'' {
            if let Some(c) = self.next() && self.match_next('\'') {
                return Token::Char(c);
            }
            panic!("'.+' - means Char type, brackers need to contain 1 char");
        }

        if ch.is_ascii_digit() {
            return self.read_number(start_pos, false);
        }

        if is_ident_start(ch) {
            return self.read_ident_or_keyword(start_pos);
        }

        match ch {
            '/' => {
                if self.match_next('/') {
                    while let Some((_, c)) = self.bump() {
                        if c == '\n' { break; }
                    }
                    self.next_token()
                } else if self.match_next('*') {
                    let mut prev_star = false;
                    while let Some((_, c)) = self.bump() {
                        if prev_star && c == '/' { break; }
                        prev_star = c == '*';
                    }
                    self.next_token()
                } else {
                    self.match_mext_if_else('=', Token::AssignOper(consts::ASSIGN_DIV), Token::Div)
                }
            }
            ':' => Token::Colon,
            '=' => {
                if self.match_next('=') {
                    Token::Equal
                }
                else if self.match_next('>') {
                    Token::FatArrow
                }
                else {Token::Assign}
            }
            '+' => {
                if self.match_next('=') { Token::AssignOper(consts::ASSIGN_ADD) }
                else if self.match_next('+') { Token::Inc }
                else { Token::Plus }
            }
            '_' => Token::UnderScope,
            '|' => self.match_mext_if_else('|', Token::Or, Token::ArifmOr),
            '[' => Token::LBracket,
            ']' => Token::RBracket,
            '&' => self.match_mext_if_else('&', Token::LogicalAnd, Token::ArifmAnd),
            '<' => self.match_mext_if_else('=', Token::LessOrEqual, Token::Less),
            '^' => Token::Pow,
            '>' => self.match_mext_if_else('=', Token::GreaterOrEqual, Token::Greater),
            '{' => Token::Begin,
            '}' => Token::End,
            '%' => Token::Mod,
            '(' => Token::LParen,
            ')' => Token::RParen,
            ',' => Token::Comma,
            ';' => Token::Semicolon,
            '*' => {
                if self.match_next('=') { Token::AssignOper(consts::ASSIGN_MUL) }
                else if self.match_next('*') { Token::Pow }
                else { Token::Mult }
            }
            '-' => {
                if self.match_next('=') { Token::AssignOper(consts::ASSIGN_SUB) }
                else if self.match_next('-') { Token::Dec }
                else if self.match_next('>') {Token::Arrow}
                else { Token::Minus }
            }
            '!' => self.match_mext_if_else('=', Token::NotEqual, Token::Not),
            '.' => self.match_mext_if_else('.', Token::DotDot, Token::Dot),
            _ => panic!("unknown symbol: {}", ch),
        }
    }

    #[inline(always)]
    fn skip_whitespace(&mut self) {
        while let Some(&(_, c)) = self.chars.peek() {
            if c.is_whitespace() {
                self.bump();
            } else {
                break;
            }
        }
    }

    fn read_number(&mut self, start_pos: usize, sign: bool) -> Token<'a> {
        let sign_mult = if sign { -1 } else { 1 };
        let mut is_float = false;
        let mut has_underscores = false;

        while let Some(&(_, ch)) = self.chars.peek() {
            if ch.is_ascii_digit() {
                self.bump();
            } else if ch == '.' {
                if is_float { break; }
                let mut lookahead = self.chars.clone();
                lookahead.next(); 
                if let Some(&(_, next_ch)) = lookahead.peek() {
                    if next_ch.is_ascii_digit() {
                        is_float = true;
                        self.bump(); 
                    } else { break; }
                } else { break; }
            } else if ch == '_' {
                has_underscores = true;
                self.bump();
            } else { break; }
        }

        let end_pos = self.current_byte_pos();
        let raw_str = &self.input[start_pos..end_pos];

        let s = if has_underscores {
            Cow::Owned(raw_str.replace('_', ""))
        } else {
            Cow::Borrowed(raw_str)
        };

        if is_float {
            if let Ok(f) = s.parse::<f64>() {
                return Token::Float(f * sign_mult as f64);
            }
        } else {
            if let Ok(p) = s.parse::<i64>() {
                return Token::Number(p * sign_mult);
            }
        }
        
        Token::Number(0)
    }

    fn read_string(&mut self, start_pos: usize) -> Token<'a> {
        let mut skip_next = false;
        
        while let Some((_, c)) = self.bump() {
            if skip_next {
                skip_next = false;
            } else if c == '\\' {
                skip_next = true;
            } else if c == '"' {
                let end_pos = self.current_byte_pos();
                return Token::String(&self.input[start_pos + 1..end_pos - 1]);
            }
        }
        
        Token::String(&self.input[start_pos + 1..self.current_byte_pos()])
    }

    fn read_ident_or_keyword(&mut self, start_pos: usize) -> Token<'a> {
        while let Some(&(_, c)) = self.chars.peek() {
            if is_ident_part(c) {
                self.bump();
            } else {
                break;
            }
        }

        let s = &self.input[start_pos..self.current_byte_pos()];
        
        match s {
            "let" => Token::Let,
            "Ok" => Token::Ok,
            "match" => Token::Match,
            "Some" => Token::Some,
            "Err" => Token::Err,
            "None" => Token::None,
            "if" => Token::If,
            "else" => Token::Else,

            "while" => Token::While,
            "for" => Token::For,
            "fn" => Token::Func,
            "in" => Token::In,

            "Number" | "Num" => Token::TypeNumber,
            "Float" => Token::TypeFloat,
            "Bool" => Token::TypeBool,
            "Char" => Token::TypeChar,
            "Str" => Token::TypeStr,

            "true" | "True" => Token::Bool(true),
            "false" | "False" => Token::Bool(false),

            ".." => Token::DotDot,

            _ => Token::Ident(s),
        }
    }
}

#[inline(always)]
fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

#[inline(always)]
fn is_ident_part(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}
