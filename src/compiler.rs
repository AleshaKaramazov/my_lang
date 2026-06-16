use crate::op::Op;
use crate::consts;
use crate::errors::CompilerError;
use crate::types::Type;
use crate::lexer::{Lexer, Token};

use rustc_hash::FxHashMap;

type ChangedScope = Option<(usize, usize, Option<Type>)>;
pub struct Compiler<'a> {
    source: &'a str,
    code: Vec<Op<'a>>,
    current_token: Token<'a>,
    lexer: Lexer<'a>,
    variables: FxHashMap<&'a str, (usize, usize, Option<Type>)>,
    functions_args: FxHashMap<&'a str, Vec<Type>>,
    next_slot: usize,
    scope_depth: usize,
    scope_changes: Vec<(&'a str, ChangedScope)>,
    loop_contexts: Vec<(usize, Vec<usize>)>,
}

impl<'a> Compiler<'a> {
    pub fn new(source: &'a str) -> Self {
        let lexer = Lexer::new(source);
        Self {
            source,
            code: Vec::with_capacity(512),
            current_token: Token::Begin, 
            lexer, 
            variables: FxHashMap::default(),
            functions_args: FxHashMap::default(),
            next_slot: 0,
            scope_depth: 0,
            scope_changes: Vec::with_capacity(64),
            loop_contexts: Vec::with_capacity(16),
        }
    }

    #[inline(always)]
    pub fn advance_token(&mut self) {
        self.current_token = self.lexer.next_token(); 
    }

    #[inline(always)]
    pub fn next_if(&mut self, token: Token) -> bool {
        if self.current_token == token {
            self.advance_token();
            true
        } else {
            false
        }
    }

    #[inline(always)]
    pub fn add_plug(&mut self, op: Op<'a>) -> usize {
        let code = self.code.len();
        self.code.push(op);
        code
    }

    pub fn throw_error<T>(&self, err: CompilerError, msg: &str) -> Result<T, CompilerError> {
        let (line_num, col_pos) = self.lexer.get_pos();

        let line_index = line_num;
        let line_text = self.source.lines().nth(line_index).unwrap_or("");

        let display_line_num = line_num + 1;
        let prefix = format!(" {} | ", display_line_num);

        let line_char_count = line_text.chars().count();
        let rel_char_pos = col_pos.min(line_char_count);

        let padding = " ".repeat(prefix.chars().count() + rel_char_pos);

        eprintln!("\n[Compile error]: {}", msg);
        eprintln!("{}{}", prefix, line_text);
        eprintln!("{}^^--\n", padding);

        Err(err)
    }    

    #[inline(always)]
    pub fn patch_plug(&mut self, index: usize) {
        let target = self.code.len();
        match self.code[index] {
            Op::JumpIfFalse(_) => self.code[index] = Op::JumpIfFalse(target),
            Op::JumpIfTrue(_) => self.code[index] = Op::JumpIfTrue(target),
            Op::Jump(_) => self.code[index] = Op::Jump(target),
            Op::IterNext(_) => self.code[index] = Op::IterNext(target),
            _ => unreachable!("VM Error: Attempted to patch a non-jump instruction!"),
        }
    }

    pub fn types_match(expected: &Type, actual: &Type) -> bool {
        match (expected, actual) {
            (Type::Infer, _) | (_, Type::Infer) => true,
            (Type::Set(a), Type::Set(b)) => Self::types_match(a, b),
            (Type::Iter(a), Type::Iter(b)) => Self::types_match(a, b),
            (Type::Cat(a), Type::Cat(b)) => Self::types_match(a, b),
            (Type::Result(a), Type::Void) if a.0 == Type::Void  => true,
            (Type::Result(a), Type::Result(b)) => Self::types_match(&a.0, &b.0) && Self::types_match(&a.1, &b.1),
            (a, b) => a == b,
        }
    }

    fn check_std_func_args(&self, name: &str, args: &[Type]) -> Result<(), CompilerError> {
        let is_valid = match name {
            "len" | "enumerate" | "is_ok" | "is_empty" | "is_some" | "collect" => args.len() == 1,
            "readch" | "read" => args.len() == 1,
            "readln" | "clear_console" => true,
            "format" | "write" | "writeln" | "print" | "println" => true,
            "open" => 
                args.len() > 0 && Self::types_match(&Type::Str, &args[0]),
            "create" | "truncate" | "parse" | "lines" | "split_whitespace" | "to_lower" | "to_upper" => {
                args.len() == 1 && Self::types_match(&Type::Str, &args[0])
            },
            "starts_with" | "split" => {
                args.len() == 2 && Self::types_match(&Type::Str, &args[0]) && (Self::types_match(&Type::Str, &args[1]) || Self::types_match(&Type::Char, &args[1]))
            },
            "contains" => args.len() > 1,
            "push" | "step" | "filter_map" | "map" | "filter" => args.len() == 2,
            "nth" => args.len() == 2 && Self::types_match(&Type::Number, &args[1]),
            _ => true,
        };
        if !is_valid {
            return self.throw_error(CompilerError::UnexpectedArg, "Invalid arguments for standard function");
        }
        Ok(())
    }

    pub fn parse_for(&mut self) -> Result<(), CompilerError> {
        self.advance_token(); 
        
        let has_parens = self.next_if(Token::LParen);
        
        let mut loop_vars = Vec::new();
        while let Token::Ident(name) = self.current_token {
            loop_vars.push(name);
            self.advance_token();
            if !self.next_if(Token::Comma) { break; }
        }
        
        if has_parens {
            self.expect(Token::RParen)?;
        }
        
        self.expect(Token::In)?;
        
        let iter_type = self.parse_expression()?;     

        if !matches!(iter_type, Type::Iter(_) | Type::Set(_) | Type::Infer) {
            return self.throw_error(CompilerError::UnexpectedArg, "Expected iterable type in for loop");
        }

        self.code.push(Op::MakeIter); 
        
        let elem_type = match iter_type {
            Type::Iter(boxed) => *boxed,
            Type::Set(boxed) => *boxed,
            _ => Type::Infer,
        };
        
        let loop_start = self.code.len();
        let exit_jump = self.add_plug(Op::IterNext(0)); 
        
        self.loop_contexts.push((loop_start, Vec::new()));
        
        let mut old_vals = Vec::new();

        if loop_vars.len() > 1 {
            self.code.push(Op::UnpackTuple);
            for name in loop_vars.iter().rev() {
                let var_id = self.next_slot;
                self.next_slot += 1;
                let old_val = self.variables.insert(name, (var_id, self.scope_depth, Some(Type::Infer)));
                old_vals.push((*name, old_val));

                if self.scope_depth == 0 {
                    self.code.push(Op::StoreGlobal(var_id));
                } else {
                    self.code.push(Op::StoreLocal(var_id, 0));
                }
            }
        } else if loop_vars.len() == 1 {
            let name = loop_vars[0];
            let var_id = self.next_slot;
            self.next_slot += 1;
            let old_val = self.variables.insert(name, (var_id, self.scope_depth, Some(elem_type)));
            old_vals.push((name, old_val));

            if self.scope_depth == 0 {
                self.code.push(Op::StoreGlobal(var_id));
            } else {
                self.code.push(Op::StoreLocal(var_id, 0));
            }
        } else {
            self.code.push(Op::Pop);
        }

        self.parse_block()?;
        
        self.code.push(Op::Pop);
        self.code.push(Op::Jump(loop_start));
        self.patch_plug(exit_jump);
        
        let (_, break_plugs) = self.loop_contexts.pop().unwrap();
        for b in break_plugs {
            self.patch_plug(b);
        }
        
        self.code.push(Op::Pop); 

        for (name, old_val) in old_vals {
            if let Some(prev) = old_val {
                self.variables.insert(name, prev);
            } else {
                self.variables.remove(name);
            }
            self.next_slot -= 1;
        }
        Ok(())
    }    

    #[inline(always)]
    fn expect(&mut self, token: Token) -> Result<(), CompilerError> {
        if !self.next_if(token) {
            return self.throw_error(
                CompilerError::ExpectedToken, 
                &format!("Expected {:?}, got {:?}", token, self.current_token)
            );
        } 
        Ok(())
    }

    #[inline(always)]
    fn parse_expression(&mut self) -> Result<Type, CompilerError> {
        self.parse_range()
    }

    #[inline(always)]
    fn parse_range(&mut self) -> Result<Type, CompilerError> {
        let mut tp = self.parse_logical_or()?;
        
        if self.current_token == Token::DotDot {
            if !Self::types_match(&tp, &Type::Number) {
                return self.throw_error(CompilerError::UnexpectedArg, "Expected number in range");
            }
            tp = Type::Iter(Box::new(Type::Number));
            self.advance_token();
            let incl = self.next_if(Token::Assign);
            if self.current_token == Token::RBracket {
                self.code.push(Op::PushNumber(i64::MAX)); 
            } else {
                let oth_tp = self.parse_logical_or()?;
                if !Self::types_match(&oth_tp, &Type::Number) {
                    return self.throw_error(CompilerError::UnexpectedArg, "Expected number in range end");
                }
            }
            self.code.push(Op::MakeRange(incl));
        }
        Ok(tp)
    }

    fn parse_logical_or(&mut self) -> Result<Type, CompilerError> {
        let mut tp = self.parse_logical_and()?;

        while self.current_token == Token::Or {
            if !Self::types_match(&tp, &Type::Bool) && tp != Type::Infer {
                return self.throw_error(CompilerError::UnexpectedArg, "Expected bool in logical OR");
            }
            self.advance_token();

            let jump_true_1 = self.add_plug(Op::JumpIfTrue(0));
            
            let oth_tp = self.parse_logical_and()?;
            if !Self::types_match(&oth_tp, &Type::Bool) && oth_tp != Type::Infer {
                return self.throw_error(CompilerError::UnexpectedArg, "Expected bool in logical OR");
            }
            let jump_true_2 = self.add_plug(Op::JumpIfTrue(0));

            self.code.push(Op::PushBool(false));
            let jump_end = self.add_plug(Op::Jump(0));

            self.patch_plug(jump_true_1);
            self.patch_plug(jump_true_2);
            self.code.push(Op::PushBool(true));

            self.patch_plug(jump_end);
            tp = Type::Bool;
        }
        Ok(tp)
    }

    fn parse_logical_and(&mut self) -> Result<Type, CompilerError> {
        let mut tp = self.parse_equality()?; 

        while self.current_token == Token::LogicalAnd { 
            if !Self::types_match(&tp, &Type::Bool) && tp != Type::Infer {
                return self.throw_error(CompilerError::UnexpectedArg, "Expected bool in logical AND");
            }
            self.advance_token();

            let jump_false_1 = self.add_plug(Op::JumpIfFalse(0));
            
            let oth_tp = self.parse_equality()?; 
            if !Self::types_match(&oth_tp, &Type::Bool) && oth_tp != Type::Infer {
                return self.throw_error(CompilerError::UnexpectedArg, "Expected bool in logical AND");
            }
            
            let jump_false_2 = self.add_plug(Op::JumpIfFalse(0));

            self.code.push(Op::PushBool(true));
            let jump_end = self.add_plug(Op::Jump(0));

            self.patch_plug(jump_false_1);
            self.patch_plug(jump_false_2);
            self.code.push(Op::PushBool(false));

            self.patch_plug(jump_end);
            tp = Type::Bool;
        }
        Ok(tp)
    }

    fn parse_equality(&mut self) -> Result<Type, CompilerError> {
        let mut tp = self.parse_relational()?;

        while self.current_token == Token::Equal || self.current_token == Token::NotEqual { 
            let op = if self.current_token == Token::Equal {Op::Equal} else {Op::NotEqual};
            self.advance_token();
            let oth_tp = self.parse_relational()?;
            if !Self::types_match(&tp, &oth_tp) {
                return self.throw_error(CompilerError::UnexpectedArg, "Type mismatch in equality operator");
            }
            self.code.push(op);
            tp = Type::Bool;
        }
        Ok(tp)
    }

    fn parse_relational(&mut self) -> Result<Type, CompilerError> {
        let mut tp = self.parse_arifm_or()?;
        while matches!(self.current_token, Token::Greater | Token::Less | Token::GreaterOrEqual | Token::LessOrEqual ) {
            let op = match self.current_token {
                Token::Greater => Op::Greater,
                Token::Less => Op::Less,
                Token::GreaterOrEqual => Op::GreaterEq,
                Token::LessOrEqual => Op::LessEq,
                _ => unreachable!()
            };
            self.advance_token();
            let oth_tp = self.parse_arifm_or()?;
            if !Self::types_match(&tp, &oth_tp) {
                return self.throw_error(CompilerError::UnexpectedArg, "Type mismatch in relational operator");
            }
            self.code.push(op);
            tp = Type::Bool;
        }
        Ok(tp)
    }

    fn parse_arifm_or(&mut self) -> Result<Type, CompilerError> {
        let mut tp = self.parse_arifm_and()?;
        while self.current_token == Token::ArifmOr {
            self.advance_token();
            let oth_tp = self.parse_arifm_and()?;
            if !Self::types_match(&tp, &Type::Number) && tp != Type::Infer {
                return self.throw_error(CompilerError::UnexpectedArg, "Expected number in bitwise OR");
            }
            if !Self::types_match(&oth_tp, &Type::Number) && oth_tp != Type::Infer {
                return self.throw_error(CompilerError::UnexpectedArg, "Expected number in bitwise OR");
            }
            self.code.push(Op::ArifmOr);
            tp = Type::Number;
        }
        Ok(tp)
    }

    fn parse_arifm_and(&mut self) -> Result<Type, CompilerError> {
        let mut tp = self.parse_term()?;
        while self.current_token == Token::ArifmAnd {
            self.advance_token();
            let oth_tp = self.parse_term()?;
            if !Self::types_match(&tp, &Type::Number) && tp != Type::Infer {
                return self.throw_error(CompilerError::UnexpectedArg, "Expected number in bitwise AND");
            }
            if !Self::types_match(&oth_tp, &Type::Number) && oth_tp != Type::Infer {
                return self.throw_error(CompilerError::UnexpectedArg, "Expected number in bitwise AND");
            }
            self.code.push(Op::ArifmAnd);
            tp = Type::Number;
        }
        Ok(tp)
    }

    fn parse_term(&mut self) -> Result<Type, CompilerError> {
        let mut tp = self.parse_factor()?;
        while self.current_token == Token::Plus || self.current_token == Token::Minus {
            let is_plus = self.current_token == Token::Plus;
            self.advance_token(); 
            let oth_tp = self.parse_factor()?;  

            if is_plus && (tp == Type::Str || oth_tp == Type::Str) {
                if !Self::types_match(&tp, &Type::Str) && tp != Type::Infer {
                    return self.throw_error(CompilerError::UnexpectedArg, "Type mismatch in string concatenation");
                }
                if !Self::types_match(&oth_tp, &Type::Str) && oth_tp != Type::Infer {
                    return self.throw_error(CompilerError::UnexpectedArg, "Type mismatch in string concatenation");
                }
                tp = Type::Str;
            } else if tp == Type::Float || oth_tp == Type::Float {
                if !Self::types_match(&tp, &Type::Float) && tp != Type::Infer {
                    return self.throw_error(CompilerError::UnexpectedArg, "Expected float");
                }
                if !Self::types_match(&oth_tp, &Type::Float) && oth_tp != Type::Infer {
                    return self.throw_error(CompilerError::UnexpectedArg, "Expected float");
                }
                tp = Type::Float;
            } else {
                if !Self::types_match(&tp, &Type::Number) && tp != Type::Infer {
                    return self.throw_error(CompilerError::UnexpectedArg, "Expected number");
                }
                if !Self::types_match(&oth_tp, &Type::Number) && oth_tp != Type::Infer {
                    return self.throw_error(CompilerError::UnexpectedArg, "Expected number");
                }
                tp = Type::Number;
            }

            if is_plus {
                self.code.push(Op::Plus);
            } else {
                self.code.push(Op::Sub);
            }
        }
        Ok(tp)
    }

    fn parse_factor(&mut self) -> Result<Type, CompilerError> {
        let mut tp = self.parse_power()?;

        while self.current_token == Token::Mult || self.current_token == Token::Div {
            let is_star = self.current_token == Token::Mult;
            self.advance_token();
            let oth_tp = self.parse_power()?;

            if !is_star {
                match self.code.last() {
                    Some(Op::PushNumber(0)) => return self.throw_error(CompilerError::UnexpectedArg, "Division by zero is not allowed"),
                    Some(Op::PushFLoat(f)) if *f == 0.0 => return self.throw_error(CompilerError::UnexpectedArg, "Division by zero is not allowed"),
                    _ => {}
                }
            }

            if tp == Type::Float || oth_tp == Type::Float {
                if !Self::types_match(&tp, &Type::Float) && tp != Type::Infer {
                    return self.throw_error(CompilerError::UnexpectedArg, "Expected float");
                }
                if !Self::types_match(&oth_tp, &Type::Float) && oth_tp != Type::Infer {
                    return self.throw_error(CompilerError::UnexpectedArg, "Expected float");
                }
                tp = Type::Float;
            } else {
                if !Self::types_match(&tp, &Type::Number) && tp != Type::Infer {
                    return self.throw_error(CompilerError::UnexpectedArg, "Expected number");
                }
                if !Self::types_match(&oth_tp, &Type::Number) && oth_tp != Type::Infer {
                    return self.throw_error(CompilerError::UnexpectedArg, "Expected number");
                }
                tp = Type::Number;
            }

            if is_star {
                self.code.push(Op::Mult);
            } else {
                self.code.push(Op::Div);
            }
        }
        Ok(tp)
    }

    fn parse_power(&mut self) -> Result<Type, CompilerError> {
        let mut tp = self.parse_unary()?;

        if self.current_token == Token::Pow || self.current_token == Token::Mod {
            let oper = if self.current_token == Token::Pow {Op::Pow} else {Op::Mod};
            self.advance_token();
            let oth_tp = self.parse_power()?;
            
            if matches!(oper, Op::Pow) {
                match self.code.last() {
                    Some(Op::PushNumber(n)) if *n < 0 => return self.throw_error(CompilerError::UnexpectedArg, "Power must be positive"),
                    Some(Op::PushFLoat(f)) if *f < 0.0 => return self.throw_error(CompilerError::UnexpectedArg, "Power must be positive"),
                    _ => {}
                }
            }

            if !Self::types_match(&tp, &Type::Number) && tp != Type::Infer {
                return self.throw_error(CompilerError::UnexpectedArg, "Expected number in power/mod operation");
            }
            if !Self::types_match(&oth_tp, &Type::Number) && oth_tp != Type::Infer {
                return self.throw_error(CompilerError::UnexpectedArg, "Expected number in power/mod operation");
            }

            tp = Type::Number;
            self.code.push(oper);
        }
        Ok(tp)
    }

    fn parse_unary(&mut self) -> Result<Type, CompilerError> {
        match self.current_token {
            Token::Not => {
                self.advance_token();
                let tp = self.parse_unary()?; 
                if !Self::types_match(&tp, &Type::Bool) && tp != Type::Infer {
                    return self.throw_error(CompilerError::UnexpectedArg, "Expected bool in logical NOT");
                }
                self.code.push(Op::Not);
                Ok(Type::Bool) 
            }
            Token::Inc | Token::Dec => {
                let is_inc = self.current_token == Token::Inc;
                let op = if is_inc { Op::Plus } else { Op::Sub };
                self.advance_token();
                
                let name = match self.current_token {
                    Token::Ident(n) => n,
                    _ => return Err(CompilerError::UnexpectedArg),
                };
                self.advance_token();
                
                let (var_id, var_depth, var_type) = if let Some((vid, vd, tp)) = self.variables.get(name) {
                    (*vid, *vd, tp.clone())
                } else {
                    return Err(CompilerError::UnexpectedArg)
                };
                
                if !Self::types_match(&var_type.unwrap_or(Type::Infer), &Type::Number) {
                    return self.throw_error(CompilerError::UnexpectedArg, "Expected number for inc/dec operation");
                }

                let is_global = var_depth == 0;
                let depth_delta = self.scope_depth - var_depth;

                let store_var = if self.current_token == Token::LBracket {
                    let mut deep: usize = 0;
                    if is_global {
                        self.code.push(Op::LoadGlobal(var_id));
                    } else {
                        self.code.push(Op::LoadLocal(var_id, depth_delta));
                    }
                    while self.next_if(Token::LBracket) {
                        deep += 1;
                        self.parse_expression()?;
                        self.expect(Token::RBracket)?;
                    }
                    deep
                } else {
                    0
                };

                if store_var != 0 {
                    self.code.push(Op::DupTarget(store_var)); 
                    self.code.push(Op::LoadIndex(store_var)); 
                    
                    self.code.push(Op::PushNumber(1));
                    self.code.push(op);                       
                    let temp_slot = self.next_slot;
                    self.next_slot += 1;
                    
                    self.code.push(Op::Dup);                  
                    if self.scope_depth == 0 {
                        self.code.push(Op::StoreGlobal(temp_slot));
                    } else {
                        self.code.push(Op::StoreLocal(temp_slot, depth_delta));
                    }
                    
                    self.code.push(Op::StoreIndex(store_var));
                    if is_global {
                        self.code.push(Op::StoreGlobal(var_id));
                    } else {
                        self.code.push(Op::StoreLocal(var_id, depth_delta));
                    }  
                    
                    if self.scope_depth == 0 {
                        self.code.push(Op::LoadGlobal(temp_slot));
                    } else {
                        self.code.push(Op::LoadLocal(temp_slot, depth_delta));
                    }

                    self.next_slot -= 1;
                } else {
                    if is_global {
                        self.code.push(Op::LoadGlobal(var_id));
                    } else {
                        self.code.push(Op::LoadLocal(var_id, depth_delta));
                    }
                    self.code.push(Op::PushNumber(1));
                    self.code.push(op);
                    self.code.push(Op::Dup);
                    if is_global {
                        self.code.push(Op::StoreGlobal(var_id));
                    } else {
                        self.code.push(Op::StoreLocal(var_id, depth_delta));
                    }
                }
                Ok(Type::Number) 
            }
            _ => {
                self.parse_primary()
            }
        }
    }

   
    fn parse_primary(&mut self) -> Result<Type, CompilerError> {
        let mut tp = match self.current_token {
            Token::String(s) => {
                self.advance_token();
                self.code.push(Op::PushStr(s));
                Type::Str
            }
            Token::Char(c) => {
                self.advance_token();
                self.code.push(Op::PushChar(c));
                Type::Char
            }
            Token::Number(n) => {
                self.advance_token();
                self.code.push(Op::PushNumber(n));
                Type::Number
            }
            Token::Float(f) => {
                self.advance_token();
                self.code.push(Op::PushFLoat(f));
                Type::Float
            }
            Token::Bool(b) => {
                self.advance_token();
                self.code.push(Op::PushBool(b));
                Type::Bool
            }
            Token::ArifmOr | Token::Or => {
                self.parse_fn(None, None)?;
                Type::Infer
            }
            Token::DotDot => {
                self.advance_token();
                self.code.push(Op::PushNumber(0));
                let incl = self.next_if(Token::Assign);
                if self.current_token == Token::RBracket {
                    self.code.push(Op::PushNumber(i64::MAX)); 
                } else {
                    let end_tp = self.parse_expression()?;
                    if !Self::types_match(&end_tp, &Type::Number) {
                        return self.throw_error(CompilerError::UnexpectedArg, "Expected number in range");
                    }
                }
                self.code.push(Op::MakeRange(incl));
                Type::Iter(Box::new(Type::Number))
            }
            Token::Ok | Token::Some | Token::Err => {
                let oper = match self.current_token {
                    Token::Ok => Op::MakeOk,
                    Token::Err => Op::MakeErr,
                    Token::Some => Op::MakeSome,
                    _ => unreachable!()
                };
                let is_some = self.current_token == Token::Some;
                let is_ok = self.current_token == Token::Ok;
                self.advance_token();
                self.expect(Token::LParen)?;
                let inner_tp = self.parse_expression()?;
                self.code.push(oper);
                self.expect(Token::RParen)?;
                if is_some {
                    Type::Cat(Box::new(inner_tp))
                } else if is_ok {
                    Type::Result(Box::new((inner_tp, Type::Infer)))
                } else {
                    Type::Result(Box::new((Type::Infer, inner_tp)))
                }
            }
            Token::Let => {
                self.advance_token();
                let is_right = match self.current_token {
                    Token::Ok | Token::Some => true,
                    Token::Err => false,
                    Token::Ident(name) => {
                        self.advance_token();
                        self.expect(Token::Assign)?;
                        let var_id = self.next_slot;
                        self.next_slot += 1;

                        let expr_type = self.parse_equality()?;
                        let old_val = self.variables.insert(name, (var_id, self.scope_depth, Some(expr_type)));
                        self.scope_changes.push((name, old_val));

                        if self.scope_depth == 0 {
                            self.code.push(Op::StoreGlobal(var_id));
                        } else {
                            self.code.push(Op::StoreLocal(var_id, 0));
                        }

                        self.code.push(Op::PushBool(true));
                        let end_jump = self.add_plug(Op::Jump(0));

                        self.code.push(Op::PushBool(false));
                        self.patch_plug(end_jump);

                        return Ok(Type::Bool) 
                    } 
                    _ => return Err(CompilerError::UnexpectedArg),
                };
                let token_kind = self.current_token;
                self.advance_token();
                self.expect(Token::LParen)?;
                let name = match self.current_token {
                    Token::Ident(n) => n,
                    _ => return Err(CompilerError::UnexpectedArg),
                };
                self.advance_token();
                self.expect(Token::RParen)?;
                self.expect(Token::Assign)?;
                let expr_type = self.parse_equality()?;

                self.code.push(Op::Dup); 

                let fail_jump = self.code.len();
                if is_right {
                    self.code.push(Op::SafeUnwR(0));
                } else {
                    self.code.push(Op::SafeUnwL(0));
                }

                let inner_type = match &expr_type {
                    Type::Result(boxed) => {
                        if token_kind == Token::Ok { boxed.0.clone() } else { boxed.1.clone() }
                    }
                    Type::Cat(boxed) => {
                        if token_kind == Token::Some { *boxed.clone() } else { Type::Infer }
                    }
                    _ => Type::Infer,
                };

                let var_id = self.next_slot;
                self.next_slot += 1;

                let old_val = self.variables.insert(name, (var_id, self.scope_depth, Some(inner_type)));
                self.scope_changes.push((name, old_val));

                if self.scope_depth == 0 {
                    self.code.push(Op::StoreGlobal(var_id));
                } else {
                    self.code.push(Op::StoreLocal(var_id, 0));
                }

                self.code.push(Op::Pop); 

                self.code.push(Op::PushBool(true));
                let end_jump = self.add_plug(Op::Jump(0));

                let target = self.code.len();
                if is_right {
                    self.code[fail_jump] = Op::SafeUnwR(target);
                } else {
                    self.code[fail_jump] = Op::SafeUnwL(target);
                }

                self.code.push(Op::Pop); 
                
                self.code.push(Op::PushBool(false));
                self.patch_plug(end_jump);
                Type::Bool
            }
            Token::None => {
                self.advance_token();
                self.code.push(Op::None);
                Type::Cat(Box::new(Type::Infer))
            }
            Token::LBracket => {
                self.advance_token();
                let mut arg_count = 0;
                let mut elem_type = Type::Infer;
                while !self.next_if(Token::RBracket) {
                    arg_count += 1;
                    let inner_tp = self.parse_expression()?;
                    if elem_type == Type::Infer {
                        elem_type = inner_tp;
                    } else if inner_tp != Type::Infer && !Self::types_match(&elem_type, &inner_tp) {
                        return self.throw_error(CompilerError::UnexpectedArg, "Type mismatch in array elements");
                    }
                    self.next_if(Token::Comma);
                }
                self.code.push(Op::MakeSet(arg_count));
                Type::Set(Box::new(elem_type))
            } 
            Token::If => {
                self.parse_if(true)?
            }
            Token::Match => {
                self.parse_match()?
            }
            Token::Ident(name) => {
                self.parse_ident(name)?
            }
            Token::Begin => {
                self.parse_block()?
            }
            Token::LParen => {
                self.advance_token();
                if self.next_if(Token::RParen) {
                    self.code.push(Op::PushVoid);
                } else {
                    self.parse_expression()?;
                    
                    let mut count = 1;
                    let mut is_tuple = false;

                    while self.next_if(Token::Comma) {
                        is_tuple = true;
                        if self.current_token == Token::RParen { break; } 
                        self.parse_expression()?;
                        count += 1;
                    }

                    self.expect(Token::RParen)?;
                    
                    if is_tuple {
                        self.code.push(Op::MakeTuple(count));
                    }
                }
                Type::Infer
            }
            _ => return Err(CompilerError::UnexpectedArg),
        };
        loop {
            if self.next_if(Token::LBracket) {
                let idx_type = self.parse_expression()?;
                
                if let Some(Op::PushNumber(n)) = self.code.last() 
                    && *n < 0 {
                        return self.throw_error(CompilerError::UnexpectedArg, "Index cannot be negative");
                }

                if !Self::types_match(&idx_type, &Type::Number) && !Self::types_match(&idx_type, &Type::Iter(Box::new(Type::Number))) {
                    return self.throw_error(CompilerError::UnexpectedArg, &format!("Expected number for index, finded: {:?}", idx_type));
                }
                self.expect(Token::RBracket)?;
                
                let mut arg_count = 1;
                while self.next_if(Token::LBracket) {
                    arg_count += 1;
                    let inner_idx_type = self.parse_expression()?;

                    if let Some(Op::PushNumber(n)) = self.code.last()
                        && *n < 0 {
                            return self.throw_error(CompilerError::UnexpectedArg, "Index cannot be negative");
                    }

                    if !Self::types_match(&inner_idx_type, &Type::Number) && !Self::types_match(&inner_idx_type, &Type::Iter(Box::new(Type::Number))) {
                        return self.throw_error(CompilerError::UnexpectedArg, &format!("Expected number for index, finded: {:?}", idx_type));
                    }
                    self.expect(Token::RBracket)?;
                }
                self.code.push(Op::LoadIndex(arg_count));
                for _ in 0..arg_count {
                    tp = match tp {
                        Type::Set(boxed) => *boxed,
                        Type::Iter(boxed) => *boxed,
                        _ => Type::Infer,
                    };
                }
            } else if self.next_if(Token::Dot) {
                let method_name = if let Token::Ident(n) = self.current_token {
                    self.advance_token();
                    n 
                } else {
                    return Err(CompilerError::UnexpectedArg);
                };
                self.expect(Token::LParen)?;
                self.parse_func_call(method_name, Some(tp.clone()))?;
                tp = Self::func_return_type(method_name);
            } else if self.next_if(Token::Query) {
                self.code.push(Op::Try);
                tp = match tp {
                    Type::Result(boxed) => boxed.0.clone(),
                    Type::Cat(boxed) => *boxed,
                    _ => Type::Infer,
                };
            } else {
                break;
            }
        }
        Ok(tp)
    }

    fn parse_type(&mut self) -> Result<Type, CompilerError> {
        let res = match self.current_token {
            Token::TypeNumber => Type::Number,
            Token::TypeStr => Type::Str,
            Token::TypeBool => Type::Bool,
            Token::TypeChar => Type::Char,
            Token::TypeFile => Type::File,
            Token::TypeFloat => Type::Float,
            Token::LParen => {
                self.advance_token();
                self.expect(Token::RParen)?;
                return Ok(Type::Void);
            }
            Token::TypeSet => {
                self.advance_token();
                self.expect(Token::Less)?;
                let tp = self.parse_type()?;
                self.expect(Token::Greater)?;
                return Ok(Type::Set(Box::new(tp)))
            }
            Token::TypeCat => {
                self.advance_token();
                self.expect(Token::Less)?;
                let tp = self.parse_type()?;
                self.expect(Token::Greater)?;
                return Ok(Type::Cat(Box::new(tp)))
            }
            Token::TypeResult => {
                self.advance_token();
                self.expect(Token::Less)?;
                let ok_tp = self.parse_type()?;
                self.expect(Token::Comma)?;
                let err_tp = self.parse_type()?;
                self.expect(Token::Greater)?;
                return Ok(Type::Result(Box::new((ok_tp, err_tp))))
            }
            _ => return Err(CompilerError::UnexpectedArg),
        };
        self.advance_token();
        Ok(res)
    }

    fn parse_let(&mut self, first_name: &'a str) -> Result<(), CompilerError> {
        let mut names = vec![first_name];
        let mut types = vec![];

        if self.next_if(Token::Colon) && self.current_token != Token::Assign {
            types.push(Some(self.parse_type()?));
        } else {
            types.push(None);
        }

        let mut is_tuple = false;
        while self.next_if(Token::Comma) {
            is_tuple = true;
            let next_name = match self.current_token {
                Token::Ident(n) => n,
                _ => return Err(CompilerError::UnexpectedArg),
            };
            self.advance_token();
            names.push(next_name);

            if self.next_if(Token::Colon) {
                types.push(Some(self.parse_type()?));
            } else {
                types.push(None);
            }
        }

        self.expect(Token::Assign)?;

        if names.len() == 1 && matches!(self.current_token, Token::ArifmOr | Token::Or) {
            self.parse_fn(Some(names[0]), types[0].clone())?;
            self.code.push(Op::PushVoid); 
            return Ok(());
        } 

        let expr_type = self.parse_expression()?; 
        self.next_if(Token::Semicolon);

        if !is_tuple 
            && let Some(ref t) = types[0] 
                && !Self::types_match(t, &expr_type) {
                    return self.throw_error(CompilerError::UnexpectedArg, "Type mismatch in variable declaration");
        }

        if is_tuple {
            self.code.push(Op::UnpackTuple);
            
            for (i, name) in names.into_iter().enumerate().rev() {
                let tp = types[i].clone().unwrap_or(Type::Infer);

                let var_id = self.next_slot;
                self.next_slot += 1;
                let old_value = self.variables.insert(name, (var_id, self.scope_depth, Some(tp)));
                self.scope_changes.push((name, old_value));

                if self.scope_depth == 0 {
                    self.code.push(Op::StoreGlobal(var_id));
                } else {
                    self.code.push(Op::StoreLocal(var_id, 0));
                }
            }
        } else {
            let name = names[0];
            let tp = types[0].clone().unwrap_or(expr_type);
            
            let var_id = self.next_slot;
            self.next_slot += 1;
            let old_value = self.variables.insert(name, (var_id, self.scope_depth, Some(tp)));
            self.scope_changes.push((name, old_value));

            if self.scope_depth == 0 {
                self.code.push(Op::StoreGlobal(var_id));
            } else {
                self.code.push(Op::StoreLocal(var_id, 0));
            }
        }

        self.code.push(Op::PushVoid);
        Ok(())
    }

    pub fn parse_fn(&mut self, func_name: Option<&'a str>, exp: Option<Type>) -> Result<(), CompilerError> {
        let func_id = if let Some(name) = func_name {
            let id = self.next_slot;
            self.next_slot += 1;
            let old_func_val = self.variables.insert(name, (id, self.scope_depth, None));
            self.scope_changes.push((name, old_func_val));
            Some(id)
        } else {
            None
        };

        let old_next_slot = self.next_slot;
        let start_change_idx = self.scope_changes.len();

        let jump = self.add_plug(Op::Jump(0));
        let func_entry = self.code.len(); 

        self.next_slot = 0;
        self.scope_depth += 1;
        
        let mut old_vals = vec![];
        let mut old_names = vec![];
        let mut arg_types = vec![]; 
        
        if self.current_token == Token::ArifmOr {
            self.advance_token();
            while self.current_token != Token::ArifmOr {
                let arg_name = if let Token::Ident(name) = self.current_token {
                    self.advance_token();
                    name
                } else {
                    return Err(CompilerError::UnexpectedArg);
                }; 
                
                let arg_type = if self.next_if(Token::Colon) {
                    Some(self.parse_type()?)
                } else {
                    None
                };

                let arg_slot = self.next_slot;
                self.next_slot += 1;

                old_vals.push(self.variables.insert(arg_name, (arg_slot, self.scope_depth, arg_type.clone())));
                old_names.push(arg_name);
                arg_types.push((arg_slot, arg_type)); 
                
                self.next_if(Token::Comma);
            }
            self.expect(Token::ArifmOr)?;
        } else if self.current_token == Token::Or {
            self.advance_token();
        }

        let mut expected_args = Vec::new();
        for (_, ty) in &arg_types {
            expected_args.push(ty.clone().unwrap_or(Type::Infer));
        }
        if let Some(name) = func_name {
            self.functions_args.insert(name, expected_args);
        }

        let mut explicit_ret = true;
        let exp = if let Some(tp) = exp {
            tp
        } else if self.next_if(Token::Arrow) {
            self.parse_type()?
        } else {
            explicit_ret = false;
            Type::Infer
        };

        for (slot, ty) in arg_types {
            if ty.is_some() {
                self.code.push(Op::LoadLocal(slot, 0));
                self.code.push(Op::Pop);
            }
        }

        let block_type = self.parse_block()?;        
        
        if explicit_ret && !Self::types_match(&exp, &block_type) {
            return self.throw_error(CompilerError::UnexpectedArg, &format!("Return type mismatch: {:?} -- {:?}", exp, block_type));
        }

        let final_return_type = if explicit_ret { exp } else { block_type };
        self.code.push(Op::Return);

        for (pos, old_val) in old_vals.iter().enumerate() {
            if let Some(prev) = old_val {
                self.variables.insert(old_names[pos], prev.clone());
            } else {
                self.variables.remove(old_names[pos]);
            }
        }

        while self.scope_changes.len() > start_change_idx {
            if let Some((name, old_val)) = self.scope_changes.pop() {
                if let Some(prev_slot) = old_val {
                    self.variables.insert(name, prev_slot); 
                } else {
                    self.variables.remove(name); 
                }
            }
        }

        self.scope_depth -= 1;
        self.next_slot = old_next_slot;
        self.patch_plug(jump);

        self.code.push(Op::PushFn(func_entry));
        
        if let Some(id) = func_id {
            if self.scope_depth == 0 {
                self.code.push(Op::StoreGlobal(id));
            } else {
                self.code.push(Op::StoreLocal(id, 0));
            }
        }
        if let Some(name) = func_name 
            && let Some((_, _, var_type)) = self.variables.get_mut(name) {
                *var_type = Some(final_return_type);
        }
        Ok(())
    }

    pub fn parse_block(&mut self) -> Result<Type, CompilerError> {
        self.next_if(Token::FatArrow);
        let was_open = self.next_if(Token::Begin);

        let old_next_slot = self.next_slot;
        let start_change_idx = self.scope_changes.len();

        let mut has_expression_value = false;
        let mut block_type = Type::Void;

        while self.current_token != Token::End && self.current_token != Token::Eof {
            if has_expression_value {
                self.code.push(Op::Pop);
                has_expression_value = false;
            }
            match self.current_token {
                Token::If => {
                    block_type = self.parse_if(false)?;
                    has_expression_value = true;
                }
                Token::For => {
                    self.parse_for()?;
                    block_type = Type::Void;
                    has_expression_value = false;
                }
                Token::Match => {
                    block_type = self.parse_match()?;
                    has_expression_value = true;
                }
                Token::Loop => {
                    self.parse_loop()?;
                    block_type = Type::Void;
                    has_expression_value = false;
                }
                Token::While => {
                    self.parse_while()?;
                    block_type = Type::Void;
                    has_expression_value = false;
                }
                Token::Begin => {
                    block_type = self.parse_block()?;
                    self.code.push(Op::Pop);
                    has_expression_value = true;
                }
                Token::Break => {
                    self.advance_token();
                    let plug = self.add_plug(Op::Jump(0));
                    if let Some((_, break_plugs)) = self.loop_contexts.last_mut() {
                        break_plugs.push(plug);
                    } else {
                        return Err(CompilerError::UnexpectedArg);
                    }
                    self.next_if(Token::Semicolon);
                    block_type = Type::Void;
                    has_expression_value = true; 
                }
                Token::Continue => {
                    self.advance_token();
                    if let Some(&(continue_target, _)) = self.loop_contexts.last() {
                        self.code.push(Op::Jump(continue_target));
                    } else {
                        return Err(CompilerError::UnexpectedArg);
                    }
                    self.next_if(Token::Semicolon);
                    block_type = Type::Void;
                    has_expression_value = true; 
                }
                Token::Return => {
                    self.advance_token();
                    block_type = self.parse_expression()?;
                    self.next_if(Token::Semicolon);
                    self.code.push(Op::Return);
                }
                _ => {
                    block_type = self.parse_expression()?;
                    
                    if self.next_if(Token::Semicolon) {
                        self.code.push(Op::Pop);
                        has_expression_value = false;
                        block_type = Type::Void;
                    } else {
                        if matches!(self.current_token, Token::End | Token::Eof) || !was_open {
                            has_expression_value = true;
                            break;
                        } else {
                            self.code.push(Op::Pop);
                            has_expression_value = false;
                            block_type = Type::Void;
                        }
                    }
                }
            }

            if !was_open {break;}
        }

        if was_open && self.current_token != Token::Eof {self.expect(Token::End)?};

        if !has_expression_value {
            self.code.push(Op::PushVoid);
            block_type = Type::Void;
        }

        while self.scope_changes.len() > start_change_idx {
            if let Some((name, old_val)) = self.scope_changes.pop() {
                if let Some(prev_slot) = old_val {
                    self.variables.insert(name, prev_slot); 
                } else {
                    self.variables.remove(name); 
                }
            }
        }

        self.next_slot = old_next_slot;

        Ok(block_type)
    }

    pub fn parse_match(&mut self) -> Result<Type, CompilerError> {
        self.advance_token();
        let match_type = if self.next_if(Token::UnderScope) {
            self.code.push(Op::PushVoid);
            Type::Void
        } else {
            self.parse_expression()?
        };
        self.expect(Token::Begin)?;

        let mut end_jumps = Vec::new();
        let mut final_type = Type::Infer;

        while self.current_token != Token::End && self.current_token != Token::Eof {
            let mut next_arm_jumps = Vec::new();
            let start_change_idx = self.scope_changes.len();
            let old_next_slot = self.next_slot;

            match self.current_token {
                Token::Ident(name) => {
                    self.advance_token();
                    self.code.push(Op::Dup);
                    let var_id = self.next_slot;
                    self.next_slot += 1;

                    let old_val = self.variables.insert(name, (var_id, self.scope_depth, Some(match_type.clone())));
                    self.scope_changes.push((name, old_val));

                    if self.scope_depth == 0 {
                        self.code.push(Op::StoreGlobal(var_id));
                    } else {
                        self.code.push(Op::StoreLocal(var_id, 0));
                    }

                }
                Token::None => {
                    self.advance_token();
                    self.code.push(Op::Dup);        
                    self.code.push(Op::None);       
                    self.code.push(Op::Equal);      

                    let fail_jump = self.code.len();
                    self.code.push(Op::JumpIfFalse(0)); 
                    next_arm_jumps.push(fail_jump);
                }
                Token::Ok | Token::Some | Token::Err => {
                    let is_right = matches!(self.current_token, Token::Ok | Token::Some);
                    let token_kind = self.current_token;
                    self.advance_token();
                    self.expect(Token::LParen)?;
                    
                    let name = match self.current_token {
                        Token::Ident(n) => n,
                        _ => return Err(CompilerError::UnexpectedArg),
                    };
                    self.advance_token();
                    self.expect(Token::RParen)?;

                    self.code.push(Op::Dup);

                    let fail_jump = self.code.len();
                    if is_right {
                        self.code.push(Op::SafeUnwR(0));
                    } else {
                        self.code.push(Op::SafeUnwL(0));
                    }
                    next_arm_jumps.push(fail_jump);

                    let inner_type = match &match_type {
                        Type::Result(boxed) => {
                            if token_kind == Token::Ok { boxed.0.clone() } else { boxed.1.clone() }
                        }
                        Type::Cat(boxed) => {
                            if token_kind == Token::Some { *boxed.clone() } else { Type::Infer }
                        }
                        _ => Type::Infer,
                    };

                    let var_id = self.next_slot;
                    self.next_slot += 1;

                    let old_val = self.variables.insert(name, (var_id, self.scope_depth, Some(inner_type)));
                    self.scope_changes.push((name, old_val));

                    if self.scope_depth == 0 {
                        self.code.push(Op::StoreGlobal(var_id));
                    } else {
                        self.code.push(Op::StoreLocal(var_id, 0));
                    }
                }
                Token::UnderScope => {
                    self.advance_token();
                }
                Token::Number(_) | Token::String(_) | Token::Char(_) | Token::Bool(_) | Token::Minus => {
                    let mut or_match_jumps = Vec::new();
                    loop {
                        self.code.push(Op::Dup);
                        
                        let is_neg = self.next_if(Token::Minus);
                        
                        match self.current_token {
                            Token::Number(n) => {
                                self.code.push(Op::PushNumber(if is_neg { -n } else { n }));
                                self.advance_token();
                            }
                            Token::String(s) if !is_neg => {
                                self.code.push(Op::PushStr(s));
                                self.advance_token();
                            }
                            Token::Char(c) if !is_neg => {
                                self.code.push(Op::PushChar(c));
                                self.advance_token();
                            }
                            Token::Bool(b) if !is_neg => {
                                self.code.push(Op::PushBool(b));
                                self.advance_token();
                            }
                            _ => return Err(CompilerError::UnexpectedArg),
                        }
                        
                        self.code.push(Op::Equal);
                        or_match_jumps.push(self.add_plug(Op::JumpIfTrue(0)));

                        if self.next_if(Token::ArifmOr) {
                            continue;
                        } else {
                            break;
                        }
                    }
                    
                    next_arm_jumps.push(self.code.len());
                    self.code.push(Op::Jump(0));

                    for jump in or_match_jumps {
                        self.patch_plug(jump);
                    }
                }
                _ => return Err(CompilerError::UnexpectedArg),
            }

            if self.next_if(Token::If) {
                let cond_type = self.parse_expression()?;
                if !Self::types_match(&cond_type, &Type::Bool) {
                    return self.throw_error(CompilerError::UnexpectedArg, &format!("Expected bool in if condition: {:?}", cond_type));
                }
                next_arm_jumps.push(self.code.len());
                self.code.push(Op::JumpIfFalse(0));
            }

            self.expect(Token::FatArrow)?;

            self.code.push(Op::Pop);
            let arm_type = self.parse_block()?;
            
            if final_type == Type::Infer || final_type == Type::Void {
                final_type = arm_type;
            } else if arm_type != Type::Infer && arm_type != Type::Void && !Self::types_match(&final_type, &arm_type) {
                return self.throw_error(CompilerError::UnexpectedArg, "Type mismatch in match arms");
            }

            self.next_if(Token::Comma);

            end_jumps.push(self.add_plug(Op::Jump(0)));

            while self.scope_changes.len() > start_change_idx {
                if let Some((name, old_val)) = self.scope_changes.pop() {
                    if let Some(prev_slot) = old_val {
                        self.variables.insert(name, prev_slot);
                    } else {
                        self.variables.remove(name);
                    }
                }
            }
            self.next_slot = old_next_slot;

            for jump in next_arm_jumps {
                let target = self.code.len();
                match self.code[jump] {
                    Op::JumpIfFalse(_) => self.code[jump] = Op::JumpIfFalse(target),
                    Op::Jump(_) => self.code[jump] = Op::Jump(target),
                    Op::SafeUnwR(_) => self.code[jump] = Op::SafeUnwR(target),
                    Op::SafeUnwL(_) => self.code[jump] = Op::SafeUnwL(target),
                    _ => unreachable!(),
                }
            }
        }

        self.expect(Token::End)?;
        self.code.push(Op::Pop);
        self.code.push(Op::PushVoid);

        for jump in end_jumps {
            self.patch_plug(jump);
        }

        if final_type == Type::Infer {
            final_type = Type::Void;
        }

        Ok(final_type)
    }

    pub fn parse_func_call(&mut self, name: &'a str, first_arg_type: Option<Type>) -> Result<(), CompilerError> {
        let mut arg_count = 0;
        let mut actual_arg_types = Vec::new();

        if let Some(t) = first_arg_type {
            arg_count += 1;
            actual_arg_types.push(t);
            let op = self.code.last_mut().unwrap();
            match op {
                Op::LoadLocal(i, depth_delta) => *op = Op::PushRefLocal(*i, *depth_delta),   
                Op::LoadGlobal(i) => *op = Op::PushRefGlobal(*i), 
                _ => {},
            }
        }
        while !self.next_if(Token::RParen) {
            actual_arg_types.push(self.parse_expression()?);
            arg_count += 1;
            self.next_if(Token::Comma);
        }

        if let Some(expected_args) = self.functions_args.get(name) {
            if expected_args.len() != actual_arg_types.len() {
                return self.throw_error(CompilerError::UnexpectedArg, "Argument count mismatch");
            }
            for (exp, act) in expected_args.iter().zip(actual_arg_types.iter()) {
                if !Self::types_match(exp, act) {
                    return self.throw_error(CompilerError::UnexpectedArg, "Type mismatch in function arguments");
                }
            }
        } else {
            Self::check_std_func_args(self, name, &actual_arg_types)?;
        }

        if let Some(&(id, depth, _)) = self.variables.get(name) {
            if depth == 0 {
                self.code.push(Op::LoadGlobal(id));
            } else {
                let depth_delta = self.scope_depth - depth;
                self.code.push(Op::LoadLocal(id, depth_delta));
            }
        } else {
            self.code.push(Op::PushNumber(Self::func_code(name)?));
        }
        
        self.code.push(Op::CallFunc(arg_count));
        Ok(())
    }

    pub fn parse_ident(&mut self, name: &'a str) -> Result<Type, CompilerError> {
        self.advance_token(); 
        
        if self.next_if(Token::LParen) {
            self.parse_func_call(name, None)?;
            if let Some((_, _, var_type)) = self.variables.get(name) 
                && let Some(tp) = var_type {
                    return Ok(tp.clone());
            }
            return Ok(Self::func_return_type(name));
        }

        let is_known = self.variables.contains_key(name);
        if self.current_token == Token::Colon || !is_known && self.current_token == Token::Comma {
            self.parse_let(name)?;
            return Ok(Type::Void);
        }

        let (var_id, var_depth, var_type) = match self.variables.get(name){
            Some((vid, vd, var_type)) => (*vid, *vd, var_type.clone()),
            None => return Err(CompilerError::UnfindedVar),
        };
        let is_global = var_depth == 0;
        let depth_delta = self.scope_depth - var_depth;

        let store_var = if self.current_token == Token::LBracket {
            let mut deep: usize = 0;
            if is_global {
                self.code.push(Op::LoadGlobal(var_id));
            } else {
                self.code.push(Op::LoadLocal(var_id, depth_delta));
            }
            while self.next_if(Token::LBracket) {
                deep += 1;
                let idx_type = self.parse_expression()?;
                if !Self::types_match(&idx_type, &Type::Number) && !Self::types_match(&idx_type, &Type::Iter(Box::new(Type::Number))) {
                    return self.throw_error(CompilerError::UnexpectedArg, "Expected number for index");
                }
                self.expect(Token::RBracket)?;
            }
            deep
        } else {
            0
        };

        let mut final_type = var_type.clone().unwrap_or(Type::Infer);
        let mut target_type = final_type.clone();

        if store_var != 0 {
            for _ in 0..store_var {
                target_type = match target_type {
                    Type::Set(boxed) => *boxed,
                    Type::Iter(boxed) => *boxed,
                    _ => Type::Infer,
                };
            }
        }

        match self.current_token {
            Token::Assign => {
                self.advance_token();
                let expr_type = self.parse_expression()?;

                if !Self::types_match(&target_type, &expr_type) {
                    return self.throw_error(CompilerError::UnexpectedArg, "Type mismatch in assignment");
                }

                if store_var != 0 {
                    self.code.push(Op::StoreIndex(store_var));
                    if is_global {
                        self.code.push(Op::StoreGlobal(var_id));
                    } else {
                        self.code.push(Op::StoreLocal(var_id, depth_delta));
                    }
                } else {
                    if is_global {
                        self.code.push(Op::StoreGlobal(var_id));
                    } else {
                        self.code.push(Op::StoreLocal(var_id, depth_delta));
                    }
                    if let Some((_, _, var_type_ref)) = self.variables.get_mut(name) 
                        && (var_type_ref.is_none() || *var_type_ref == Some(Type::Infer)) {
                            *var_type_ref = Some(expr_type);
                    }
                }
                self.code.push(Op::PushVoid);
                final_type = Type::Void;
            }
            Token::AssignOper(id) => {
                let op = match id {
                    consts::ASSIGN_ADD => Op::Plus,
                    consts::ASSIGN_SUB => Op::Sub,
                    consts::ASSIGN_MUL => Op::Mult,
                    consts::ASSIGN_DIV => Op::Div,
                    consts::ASSIGN_POW => Op::Pow,
                    _ => unreachable!(),
                };
                self.advance_token();
                let expr_type = self.parse_expression()?;              

                if !Self::types_match(&target_type, &expr_type) {
                    return self.throw_error(CompilerError::UnexpectedArg, "Type mismatch in assignment operator");
                }

                if store_var != 0 {
                    self.code.push(Op::DupTarget(store_var));
                    self.code.push(Op::LoadIndex(store_var));
                    self.code.push(op);

                    self.code.push(Op::StoreIndex(store_var));
                    if is_global {
                        self.code.push(Op::StoreGlobal(var_id));
                    } else {
                        self.code.push(Op::StoreLocal(var_id, depth_delta));
                    }
                } else {
                    if is_global {
                        self.code.push(Op::LoadGlobal(var_id));
                    } else {
                        self.code.push(Op::LoadLocal(var_id, depth_delta));
                    }
                    self.code.push(op);                    

                    if is_global {
                        self.code.push(Op::StoreGlobal(var_id));
                    } else {
                        self.code.push(Op::StoreLocal(var_id, depth_delta));
                    }
                }
                self.code.push(Op::PushVoid);
                final_type = Type::Void;
            }
            Token::Inc | Token::Dec => {
                let is_inc = self.current_token == Token::Inc;
                let op = if is_inc { Op::Plus } else { Op::Sub };
                self.advance_token();

                if !Self::types_match(&target_type, &Type::Number) {
                    return self.throw_error(CompilerError::UnexpectedArg, "Expected number for inc/dec operation");
                }

                if store_var != 0 {
                    self.code.push(Op::DupTarget(store_var));
                    self.code.push(Op::DupTarget(store_var));
                    
                    self.code.push(Op::LoadIndex(store_var)); 
                    
                    self.code.push(Op::PushNumber(1));
                    self.code.push(op);
                    
                    self.code.push(Op::StoreIndex(store_var));
                    if is_global {
                        self.code.push(Op::StoreGlobal(var_id));
                    } else {
                        self.code.push(Op::StoreLocal(var_id, depth_delta));
                    }
                    
                    self.code.push(Op::LoadIndex(store_var));
                } else {
                    if is_global {
                        self.code.push(Op::LoadGlobal(var_id));
                    } else {
                        self.code.push(Op::LoadLocal(var_id, depth_delta));
                    }
                    self.code.push(Op::Dup); 
                    self.code.push(Op::PushNumber(1));
                    self.code.push(op);
                    if is_global {
                        self.code.push(Op::StoreGlobal(var_id));
                    } else {
                        self.code.push(Op::StoreLocal(var_id, depth_delta));
                    }
                }
                final_type = Type::Void;
            }
            _ => {
                if store_var != 0 {
                    self.code.push(Op::LoadIndex(store_var));
                    final_type = target_type;
                } else {
                    if is_global {
                        self.code.push(Op::LoadGlobal(var_id));
                    } else {
                        self.code.push(Op::LoadLocal(var_id, depth_delta));
                    }
                }
            }
        }
        
        Ok(final_type)
    }

    pub fn parse_loop(&mut self) -> Result<(), CompilerError> {
        self.advance_token();
        
        let jump_index = self.code.len();
        self.loop_contexts.push((jump_index, Vec::new()));
        self.parse_block()?;
        self.code.push(Op::Pop);
        self.code.push(Op::Jump(jump_index));

        let (_, break_plugs) = self.loop_contexts.pop().unwrap();
        for b in break_plugs {
            self.patch_plug(b);
        }

        Ok(()) 
    }
    
    pub fn parse_while(&mut self) -> Result<(), CompilerError> {
        self.advance_token();
        
        let jump_index = self.code.len();
        self.loop_contexts.push((jump_index, Vec::new()));

        let start_change_idx = self.scope_changes.len();
        let old_next_slot = self.next_slot;

        let cond_type = self.parse_expression()?;
        if !Self::types_match(&cond_type, &Type::Bool) {
            return self.throw_error(CompilerError::UnexpectedArg, "Expected bool in while condition");
        }

        let plug = self.add_plug(Op::JumpIfFalse(0));

        self.parse_block()?;
        self.code.push(Op::Pop);
        self.code.push(Op::Jump(jump_index));

        self.patch_plug(plug);

        while self.scope_changes.len() > start_change_idx {
            if let Some((name, old_val)) = self.scope_changes.pop() {
                if let Some(prev_slot) = old_val {
                    self.variables.insert(name, prev_slot);
                } else {
                    self.variables.remove(name);
                }
            }
        }
        self.next_slot = old_next_slot;
        
        let (_, break_plugs) = self.loop_contexts.pop().unwrap();
        for b in break_plugs {
            self.patch_plug(b);
        }

        Ok(()) 
    }

    pub fn parse_if(&mut self, need_else: bool) -> Result<Type, CompilerError> {
        self.advance_token();
        let mut final_type = self.parse_if_branch()?;
        
        let mut has_else = false;
        let mut vec = vec![self.add_plug(Op::Jump(0))];
        while self.next_if(Token::Else) {
            if self.next_if(Token::If) {
                let branch_type = self.parse_if_branch()?;
                if final_type == Type::Infer || final_type == Type::Void { 
                    final_type = branch_type; 
                } else if branch_type != Type::Infer && branch_type != Type::Void && !Self::types_match(&final_type, &branch_type) {
                    return self.throw_error(CompilerError::UnexpectedArg, "Type mismatch in if branches");
                }
                vec.push(self.add_plug(Op::Jump(0)))
            }
            else {
                has_else = true; 
                let branch_type = self.parse_block()?;
                if final_type == Type::Infer || final_type == Type::Void { 
                    final_type = branch_type; 
                } else if branch_type != Type::Infer && branch_type != Type::Void && !Self::types_match(&final_type, &branch_type) {
                    return self.throw_error(CompilerError::UnexpectedArg, "Type mismatch in if branches");
                }
            }
        }

        if !has_else {
            if need_else && self.current_token == Token::End {
                return Err(CompilerError::UnexpectedArg)
            } else {
                self.code.push(Op::PushVoid);
                final_type = Type::Void;
            }
        }

        for i in vec.iter() {
            self.patch_plug(*i);
        }  

        Ok(final_type)
    }

    pub fn parse_if_branch(&mut self) -> Result<Type, CompilerError> {
        let start_change_idx = self.scope_changes.len();
        let old_next_slot = self.next_slot;

        let cond_type = self.parse_expression()?; 
        if !Self::types_match(&cond_type, &Type::Bool) {
            return self.throw_error(CompilerError::UnexpectedArg, &format!("Expected bool in if condition: {:?}", cond_type));
        }

        let plug = self.add_plug(Op::JumpIfFalse(0));

        let branch_type = self.parse_block()?;

        while self.scope_changes.len() > start_change_idx {
            if let Some((name, old_val)) = self.scope_changes.pop() {
                if let Some(prev_slot) = old_val {
                    self.variables.insert(name, prev_slot);
                } else {
                    self.variables.remove(name);
                }
            }
        }
        self.next_slot = old_next_slot;

        self.code[plug] = Op::JumpIfFalse(self.code.len() + 1);
        Ok(branch_type)
    }

    pub fn compile(mut self) -> Result<Vec<Op<'a>>, CompilerError> {
        self.parse_block()?;
        Ok(self.code)
    }

    pub fn func_return_type(func_name: &str) -> Type {
        match func_name {
            "len" => Type::Number,
            "starts_with" => Type::Bool,
            "readch" | "read" => Type::Result(Box::new((Type::Str, Type::Str))),
            "format" => Type::Str,
            "enumerate" => Type::Iter(Box::new(Type::Number)),
            "create" | "truncate" | "open" => Type::Result(Box::new((Type::File, Type::Str))),
            "is_ok" | "is_empty" | "is_some" => Type::Bool,
            "push" => Type::Void,
            "readln" => Type::Str,
            "parse" => Type::Result(Box::new((Type::Number, Type::Str))),
            "step" => Type::Iter(Box::new(Type::Number)),
            "lines" | "split_whitespace" | "split" => Type::Iter(Box::new(Type::Str)),
            "nth" => Type::Cat(Box::new(Type::Infer)),
            "collect" => Type::Set(Box::new(Type::Infer)),
            "contains" => Type::Bool,
            "to_lower" | "to_upper" => Type::Str,
            "write" | "writeln" | "print" | "println" => Type::Result(Box::new((Type::Void, Type::Str))),
            "clear_console" => Type::Void,
            "filter_map" | "map" | "filter" => Type::Set(Box::new(Type::Infer)),
            _ => Type::Infer,
        }
    }

    pub fn func_code(func_name: &'a str) -> Result<i64, CompilerError> {
        let code: i64 = match func_name {
            "len" => 1,
            "starts_with" => 2,
            "readch" => 3,
            "format" => 4,
            "enumerate" => 5,
            "read" => 6,
            "create" | "truncate" => 7,
            "open" => 8,
            "is_ok" => 9,
            "is_empty" => 10,
            "is_some" => 11,
            "push" => 12,
            "readln" => 13,
            "parse" => 14,
            "step" => 15,
            "lines" => 16,
            "split_whitespace" => 17,
            "split" => 18,
            "nth" => 19,
            "collect" => 20, 
            "contains" => 21,
            "to_lower" => 22,
            "to_upper" => 23,
            "write" => 24,
            "writeln" => 25,
            "print" => 26,
            "println" => 27,
            "filter_map" => 28,
            "map" => 29,
            "clear_console" => 30,
            "filter" => 31,
            _ => return Err(CompilerError::UnknownFunc)   
        };
        Ok(code)
    }
}
