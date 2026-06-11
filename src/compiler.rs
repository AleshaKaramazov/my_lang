use crate::op::Op;
use crate::consts;
use crate::errors::CompilerError;
use crate::types::Type;
use crate::lexer::{Lexer, Token};

use rustc_hash::FxHashMap;

pub struct Compiler<'a> {
    source: &'a str,
    code: Vec<Op<'a>>,
    current_token: Token<'a>,
    lexer: Lexer<'a>,
    variables: FxHashMap<&'a str, (usize, usize, Option<Type>)>,
    next_slot: usize,
    scope_depth: usize,
    scope_changes: Vec<(&'a str, Option<(usize, usize)>)>,
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
        
        self.parse_expression()?;     
        self.code.push(Op::MakeIter); 
        
        let loop_start = self.code.len();
        let exit_jump = self.add_plug(Op::IterNext(0)); 
        
        self.loop_contexts.push((loop_start, Vec::new()));
        
        let mut old_vals = Vec::new();

        if loop_vars.len() > 1 {
            self.code.push(Op::UnpackTuple(loop_vars.len()));
            for name in loop_vars.iter().rev() {
                let var_id = self.next_slot;
                self.next_slot += 1;
                let old_val = self.variables.insert(name, (var_id, self.scope_depth, None));
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
            let old_val = self.variables.insert(name, (var_id, self.scope_depth, None));
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
    fn parse_expression(&mut self) -> Result<(), CompilerError> {
        self.parse_range()
    }

    #[inline(always)]
    fn parse_range(&mut self) -> Result<(), CompilerError> {
        self.parse_logical_or()?;
        
        if self.current_token == Token::DotDot {
            self.advance_token();
            let incl = self.next_if(Token::Assign);
            if self.current_token == Token::RBracket {
                self.code.push(Op::PushNumber(i64::MAX)); 
            } else {
                self.parse_logical_or()?;
            }
            self.code.push(Op::MakeRange(incl));
        }
        Ok(())
    }

    fn parse_logical_or(&mut self) -> Result<(), CompilerError> {
        self.parse_logical_and()?;

        while self.current_token == Token::Or {
            self.advance_token();

            let jump_true_1 = self.add_plug(Op::JumpIfTrue(0));
            
            self.parse_logical_and()?;
            let jump_true_2 = self.add_plug(Op::JumpIfTrue(0));

            self.code.push(Op::PushBool(false));
            let jump_end = self.add_plug(Op::Jump(0));

            self.patch_plug(jump_true_1);
            self.patch_plug(jump_true_2);
            self.code.push(Op::PushBool(true));

            self.patch_plug(jump_end);
        }
        Ok(())
    }

    fn parse_logical_and(&mut self) -> Result<(), CompilerError> {
        self.parse_equality()?; 

        while self.current_token == Token::LogicalAnd { 
            self.advance_token();

            let jump_false_1 = self.add_plug(Op::JumpIfFalse(0));
            
            self.parse_equality()?; 
            
            let jump_false_2 = self.add_plug(Op::JumpIfFalse(0));

            self.code.push(Op::PushBool(true));
            let jump_end = self.add_plug(Op::Jump(0));

            self.patch_plug(jump_false_1);
            self.patch_plug(jump_false_2);
            self.code.push(Op::PushBool(false));

            self.patch_plug(jump_end);
        }
        Ok(())
    }

    fn parse_equality(&mut self) -> Result<(), CompilerError> {
        self.parse_relational()?;

        while self.current_token == Token::Equal || self.current_token == Token::NotEqual { 
            let op = if self.current_token == Token::Equal {Op::Equal} else {Op::NotEqual};
            self.advance_token();
            self.parse_relational()?;
            self.code.push(op);
        }
        Ok(())
    }

    fn parse_relational(&mut self) -> Result<(), CompilerError> {
        self.parse_arifm_or()?;
        loop {
            match self.current_token {
                Token::Greater => { 
                    self.advance_token();
                    self.parse_arifm_or()?;
                    self.code.push(Op::Greater);
                }
                Token::Less => { 
                    self.advance_token();
                    self.parse_arifm_or()?;
                    self.code.push(Op::Less);
                }
                Token::GreaterOrEqual => { 
                    self.advance_token();
                    self.parse_arifm_or()?;
                    self.code.push(Op::GreaterEq);
                }
                Token::LessOrEqual => { 
                    self.advance_token();
                    self.parse_arifm_or()?;
                    self.code.push(Op::LessEq);
                }
                _ => break,
            }
        }
        Ok(())
    }

    fn parse_arifm_or(&mut self) -> Result<(), CompilerError> {
        self.parse_arifm_and()?;
        while self.current_token == Token::ArifmOr {
            self.advance_token();
            self.parse_arifm_and()?;
            self.code.push(Op::ArifmOr);
        }
        Ok(())
    }

    fn parse_arifm_and(&mut self) -> Result<(), CompilerError> {
        self.parse_term()?;
        while self.current_token == Token::ArifmAnd {
            self.advance_token();
            self.parse_term()?;
            self.code.push(Op::ArifmAnd);
        }
        Ok(())
    }

    fn parse_term(&mut self) -> Result<(), CompilerError> {
        self.parse_factor()?;
        while self.current_token == Token::Plus || self.current_token == Token::Minus {
            let is_plus = self.current_token == Token::Plus;
            self.advance_token(); 
            self.parse_factor()?; 

            if is_plus {
                self.code.push(Op::Plus);
            } else {
                self.code.push(Op::Sub);
            }
        }
        Ok(())
    }

    fn parse_factor(&mut self) -> Result<(), CompilerError> {
        self.parse_power()?;

        while self.current_token == Token::Mult || self.current_token == Token::Div {
            let is_star = self.current_token == Token::Mult;
            self.advance_token();
            self.parse_power()?;

            if is_star {
                self.code.push(Op::Mult);
            } else {
                self.code.push(Op::Div);
            }
        }
        Ok(())
    }

    fn parse_power(&mut self) -> Result<(), CompilerError> {
        self.parse_unary()?;

        if self.current_token == Token::Pow || self.current_token == Token::Mod {
            let oper = if self.current_token == Token::Pow {Op::Pow} else {Op::Mod};
            self.advance_token();
            self.parse_power()?;
            self.code.push(oper);
        }
        Ok(())
    }

    fn parse_unary(&mut self) -> Result<(), CompilerError> {
        match self.current_token {
            Token::Not => {
                self.advance_token();
                self.parse_unary()?; 
                self.code.push(Op::Not);
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
                
                let (var_id, var_depth, _) = *self.variables.get(name).ok_or(CompilerError::UnexpectedArg)?;
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
            }
            _ => {
                self.parse_primary()?;
            }
        }
        Ok(())
    }

   
    fn parse_primary(&mut self) -> Result<(), CompilerError> {
        match self.current_token {
            Token::String(s) => {
                self.advance_token();
                self.code.push(Op::PushStr(s));
            }
            Token::ArifmOr | Token::Or => {
                self.parse_fn(None, None)?;
            }
            Token::DotDot => {
                self.advance_token();
                self.code.push(Op::PushNumber(0));
                let incl = self.next_if(Token::Assign);
                if self.current_token == Token::RBracket {
                    self.code.push(Op::PushNumber(i64::MAX)); 
                } else {
                    self.parse_expression()?;
                }
                self.code.push(Op::MakeRange(incl));
            }
            Token::Ok | Token::Some | Token::Err => {
                let oper = match self.current_token {
                    Token::Ok => Op::MakeOk,
                    Token::Err => Op::MakeErr,
                    Token::Some => Op::MakeSome,
                    _ => unreachable!()
                };
                self.advance_token();
                self.expect(Token::LParen)?;
                self.parse_expression()?;
                self.code.push(oper);
                self.expect(Token::RParen)?;
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

                        let old_val = self.variables.insert(name, (var_id, self.scope_depth, None)).map(|(one, two, _)| (one, two));
                        self.scope_changes.push((name, old_val));

                        self.parse_equality()?;

                        if self.scope_depth == 0 {
                            self.code.push(Op::StoreGlobal(var_id));
                        } else {
                            self.code.push(Op::StoreLocal(var_id, 0));
                        }

                        self.code.push(Op::PushBool(true));
                        let end_jump = self.add_plug(Op::Jump(0));

                        self.code.push(Op::PushBool(false));
                        self.patch_plug(end_jump);

                        return Ok(()) 
                    } 
                    _ => return Err(CompilerError::UnexpectedArg),
                };
                self.advance_token();
                self.expect(Token::LParen)?;
                let name = match self.current_token {
                    Token::Ident(n) => n,
                    _ => return Err(CompilerError::UnexpectedArg),
                };
                self.advance_token();
                self.expect(Token::RParen)?;
                self.expect(Token::Assign)?;

                self.parse_equality()?;

                let fail_jump = self.code.len();
                if is_right {
                    self.code.push(Op::SafeUnwR(0));
                } else {
                    self.code.push(Op::SafeUnwL(0));
                }

                let var_id = self.next_slot;
                self.next_slot += 1;

                let old_val = self.variables.insert(name, (var_id, self.scope_depth, None)).map(|(one, two, _)| (one, two));
                self.scope_changes.push((name, old_val));

                if self.scope_depth == 0 {
                    self.code.push(Op::StoreGlobal(var_id));
                } else {
                    self.code.push(Op::StoreLocal(var_id, 0));
                }

                self.code.push(Op::PushBool(true));
                let end_jump = self.add_plug(Op::Jump(0));

                let target = self.code.len();
                if is_right {
                    self.code[fail_jump] = Op::SafeUnwR(target);
                } else {
                    self.code[fail_jump] = Op::SafeUnwL(target);
                }

                self.code.push(Op::PushBool(false));
                self.patch_plug(end_jump);
            }
            Token::None => {
                self.advance_token();
                self.code.push(Op::None)
            }
            Token::Char(c) => {
                self.advance_token();
                self.code.push(Op::PushChar(c));
            }
            Token::LBracket => {
                self.advance_token();
                let mut arg_count = 0;
                while !self.next_if(Token::RBracket) {
                    arg_count += 1;
                    self.parse_expression()?;
                    self.next_if(Token::Comma);
                }
                self.code.push(Op::MakeSet(arg_count))
            }
            Token::Number(n) => {
                self.advance_token();
                self.code.push(Op::PushNumber(n));
            }
            Token::Float(f) => {
                self.advance_token();
                self.code.push(Op::PushFLoat(f));
            }
            Token::Bool(b) => {
                self.advance_token();
                self.code.push(Op::PushBool(b));
            }
            Token::If => {
                self.parse_if(true)?;
            }
            Token::Match => {
                self.parse_match()?;
            }
            Token::Ident(name) => self.parse_ident(name)?,
            Token::Begin => self.parse_block()?,
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
            }
            _ => return Err(CompilerError::UnexpectedArg),
        }
        loop {
            if self.next_if(Token::LBracket) {
                self.parse_expression()?;
                self.expect(Token::RBracket)?;
                let mut arg_count = 1;
                while self.next_if(Token::LBracket) {
                    arg_count += 1;
                    self.parse_expression()?;
                    self.expect(Token::RBracket)?;
                }
                self.code.push(Op::LoadIndex(arg_count));
            } else if self.next_if(Token::Dot) {
                let method_name = if let Token::Ident(n) = self.current_token {
                    self.advance_token();
                    n 
                } else {
                    return Err(CompilerError::UnexpectedArg);
                };
                self.expect(Token::LParen)?;
                self.parse_func_call(method_name, true)?;
            } else if self.next_if(Token::Query) {
                self.code.push(Op::Try);
            } else {
                break;
            }
        }
        Ok(())
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

        if self.next_if(Token::Colon) {
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

        self.parse_expression()?; 
        self.next_if(Token::Semicolon);

        if is_tuple {
            self.code.push(Op::UnpackTuple(names.len()));
            
            for (i, name) in names.into_iter().enumerate().rev() {
                let tp = &types[i];
                if let Some(t) = tp {
                    self.code.push(Op::ExpectType(t.clone()));
                }

                let var_id = self.next_slot;
                self.next_slot += 1;
                let old_value = self.variables.insert(name, (var_id, self.scope_depth, tp.clone())).map(|(a, b, _)| (a, b));
                self.scope_changes.push((name, old_value));

                if self.scope_depth == 0 {
                    self.code.push(Op::StoreGlobal(var_id));
                } else {
                    self.code.push(Op::StoreLocal(var_id, 0));
                }
            }
        } else {
            let name = names[0];
            let tp = &types[0];
            if let Some(t) = tp {
                self.code.push(Op::ExpectType(t.clone()));
            }
            let var_id = self.next_slot;
            self.next_slot += 1;
            let old_value = self.variables.insert(name, (var_id, self.scope_depth, tp.clone())).map(|(a, b, _)| (a, b));
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
            let old_func_val = self.variables.insert(name, (id, self.scope_depth, None)).map(|(one, two, _)| (one, two));
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

        let exp = if let Some(tp) = exp {
            tp
        } else {
            self.expect(Token::Arrow)?;
            self.parse_type()?
        };

        for (slot, ty) in arg_types {
            if let Some(t) = ty {
                self.code.push(Op::LoadLocal(slot, 0));
                self.code.push(Op::ExpectType(t));
                self.code.push(Op::Pop);
            }
        }

        self.parse_block()?;        
        self.code.push(Op::ExpectType(exp));
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
                if let Some(prev_slot) = old_val.map(|(x, y)| (x, y, None)) {
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
        Ok(())
    }

    pub fn parse_block(&mut self) -> Result<(), CompilerError> {
        self.next_if(Token::FatArrow);
        let was_open = self.next_if(Token::Begin);

        let old_next_slot = self.next_slot;
        let start_change_idx = self.scope_changes.len();

        let mut has_expression_value = false;

        while self.current_token != Token::End && self.current_token != Token::Eof {
            if has_expression_value {
                self.code.push(Op::Pop);
                has_expression_value = false;
            }
            match self.current_token {
                Token::If => {
                    self.parse_if(false)?;
                    has_expression_value = true;
                }
                Token::For => {
                    self.parse_for()?;
                    has_expression_value = false;
                }
                Token::Match => {
                    self.parse_match()?;
                    has_expression_value = true;
                }
                Token::Loop => {
                    self.parse_loop()?;
                    has_expression_value = false;
                }
                Token::While => {
                    self.parse_while()?;
                    has_expression_value = false;
                }
                Token::Begin => {
                    self.parse_block()?;
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
                    has_expression_value = true; 
                }
                Token::Return => {
                    self.advance_token();
                    self.parse_expression()?;
                    self.next_if(Token::Semicolon);
                    self.code.push(Op::Return);
                }
                _ => {
                    self.parse_expression()?;
                    
                    if self.next_if(Token::Semicolon) {
                        self.code.push(Op::Pop);
                        has_expression_value = false;
                    } else {
                        if matches!(self.current_token, Token::End | Token::Eof) || !was_open {
                            has_expression_value = true;
                            break;
                        } else {
                            self.code.push(Op::Pop);
                            has_expression_value = false;
                        }
                    }
                }
            }

            if !was_open {break;}
        }

        if was_open && self.current_token != Token::Eof {self.expect(Token::End)?};

        if !has_expression_value {
            self.code.push(Op::PushVoid);
        }

        while self.scope_changes.len() > start_change_idx {
            if let Some((name, old_val)) = self.scope_changes.pop() {
                if let Some(prev_slot) = old_val.map(|(x, y)| (x, y, None)) {
                    self.variables.insert(name, prev_slot); 
                } else {
                    self.variables.remove(name); 
                }
            }
        }

        self.next_slot = old_next_slot;

        Ok(())
    }

    pub fn parse_match(&mut self) -> Result<(), CompilerError> {
        self.advance_token();
        if self.next_if(Token::UnderScope) {
            self.code.push(Op::PushVoid);
        } else {
            self.parse_expression()?;
        }
        self.expect(Token::Begin)?;

        let mut end_jumps = Vec::new();

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

                    let old_val = self.variables.insert(name, (var_id, self.scope_depth, None)).map(|(x, y, _)| (x, y));
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

                    let var_id = self.next_slot;
                    self.next_slot += 1;

                    let old_val = self.variables.insert(name, (var_id, self.scope_depth, None)).map(|(x, y, _)| (x, y));
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
                self.parse_expression()?;
                next_arm_jumps.push(self.code.len());
                self.code.push(Op::JumpIfFalse(0));
            }

            self.expect(Token::FatArrow)?;

            self.code.push(Op::Pop);
            self.parse_block()?;
            self.next_if(Token::Comma);

            end_jumps.push(self.add_plug(Op::Jump(0)));

            while self.scope_changes.len() > start_change_idx {
                if let Some((name, old_val)) = self.scope_changes.pop() {
                    if let Some(prev_slot) = old_val.map(|(x, y)| (x, y, None)) {
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

        Ok(())
    }

    pub fn parse_func_call(&mut self, name: &'a str, first_arg: bool) -> Result<(), CompilerError> {
        let mut arg_count = 0;
        if first_arg {
            arg_count += 1;
            let op = self.code.last_mut().unwrap();
            match op {
                Op::LoadLocal(i, depth_delta) => *op = Op::PushRefLocal(*i, *depth_delta),   
                Op::LoadGlobal(i) => *op = Op::PushRefGlobal(*i), 
                _ => {},
            }
        }
        while !self.next_if(Token::RParen) {
            self.parse_expression()?;
            arg_count += 1;
            self.next_if(Token::Comma);
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

    pub fn parse_ident(&mut self, name: &'a str) -> Result<(), CompilerError> {
        self.advance_token(); 
        
        if self.next_if(Token::LParen) {
            self.parse_func_call(name, false)?;
            return Ok(());
        }

        let is_known = self.variables.contains_key(name);
        if self.current_token == Token::Colon || !is_known && (self.current_token == Token::Comma || self.current_token == Token::Assign) {
            return self.parse_let(name);
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
                self.parse_expression()?;
                self.expect(Token::RBracket)?;
            }
            deep
        } else {
            0
        };

        match self.current_token {
            Token::Assign => {
                self.advance_token();
                self.parse_expression()?;

                if let Some(ty) = var_type {
                    self.code.push(Op::ExpectType(ty.clone()));
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
                }
                self.code.push(Op::PushVoid);
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
                
                if store_var != 0 {
                    self.code.push(Op::DupTarget(store_var));
                    self.code.push(Op::LoadIndex(store_var));
                    self.parse_expression()?;              
                    self.code.push(op);

                    if let Some(ref ty) = var_type {
                        self.code.push(Op::ExpectType(ty.clone()));
                    }

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
                    self.parse_expression()?;              
                    self.code.push(op);                    
                    if let Some(ref ty) = var_type {
                        self.code.push(Op::ExpectType(ty.clone()));
                    }

                    if is_global {
                        self.code.push(Op::StoreGlobal(var_id));
                    } else {
                        self.code.push(Op::StoreLocal(var_id, depth_delta));
                    }
                }
                self.code.push(Op::PushVoid);
            }
            Token::Inc | Token::Dec => {
                let is_inc = self.current_token == Token::Inc;
                let op = if is_inc { Op::Plus } else { Op::Sub };
                self.advance_token();

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
            }
            _ => {
                if store_var != 0 {
                    self.code.push(Op::LoadIndex(store_var));
                } else {
                    if is_global {
                        self.code.push(Op::LoadGlobal(var_id));
                    } else {
                        self.code.push(Op::LoadLocal(var_id, depth_delta));
                    }
                }
            }
        }
        
        Ok(())
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

        self.parse_expression()?;
        let plug = self.add_plug(Op::JumpIfFalse(0));

        self.parse_block()?;
        self.code.push(Op::Pop);
        self.code.push(Op::Jump(jump_index));

        self.patch_plug(plug);
        
        let (_, break_plugs) = self.loop_contexts.pop().unwrap();
        for b in break_plugs {
            self.patch_plug(b);
        }

        Ok(()) 
    }

    pub fn parse_if(&mut self, need_else: bool) -> Result<(), CompilerError> {
        self.advance_token();
        self.parse_if_branch()?;
        
        let mut has_else = false;
        let mut vec = vec![self.add_plug(Op::Jump(0))];
        while self.next_if(Token::Else) {
            if self.next_if(Token::If) {
                self.parse_if_branch()?;
                vec.push(self.add_plug(Op::Jump(0)))
            }
            else {
                has_else = true; 
                self.parse_block()?;
            }
        }

        if !has_else {
            if need_else && self.current_token == Token::End {
                return Err(CompilerError::UnexpectedArg)
            } else {
                self.code.push(Op::PushVoid);
            }
        }

        for i in vec.iter() {
            self.patch_plug(*i);
        }  

        Ok(())
    }

    pub fn parse_if_branch(&mut self) -> Result<(), CompilerError> {
        let start_change_idx = self.scope_changes.len();
        let old_next_slot = self.next_slot;

        self.parse_expression()?; 
        let plug = self.add_plug(Op::JumpIfFalse(0));

        self.parse_block()?;

        while self.scope_changes.len() > start_change_idx {
            if let Some((name, old_val)) = self.scope_changes.pop() {
                if let Some(prev_slot) = old_val.map(|(x, y)| (x, y, None)) {
                    self.variables.insert(name, prev_slot);
                } else {
                    self.variables.remove(name);
                }
            }
        }
        self.next_slot = old_next_slot;

        self.code[plug] = Op::JumpIfFalse(self.code.len() + 1);
        Ok(())
    }

    pub fn compile(mut self) -> Result<Vec<Op<'a>>, CompilerError> {
        self.parse_block()?;
        Ok(self.code)
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
