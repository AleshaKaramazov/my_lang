use crate::{consts, lexer::{Lexer, Token}, op::Op};
use rustc_hash::FxHashMap;

pub struct Compiler<'a> {
    code: Vec<Op<'a>>,
    current_token: Token<'a>,
    lexer: Lexer<'a>,
    variables: FxHashMap<&'a str, (usize, usize)>,
    next_slot: usize,
    scope_depth: usize,
    scope_changes: Vec<(&'a str, Option<(usize, usize)>)>,
}

impl<'a> Compiler<'a> {
    pub fn new(source: &'a str) -> Self {
        let lexer = Lexer::new(source);
        Self {
            code: vec![],
            current_token: Token::Begin, 
            lexer, 
            variables: FxHashMap::default(),
            next_slot: 0,
            scope_depth: 0,
            scope_changes: Vec::new(),
        }
    }

    pub fn advance_token(&mut self) {
        self.current_token = self.lexer.next_token(); 
    }

    pub fn next_if(&mut self, token: Token) -> bool {
        if self.current_token == token {
            self.advance_token();
            true
        } else {
            false
        }
    }

    pub fn add_plug(&mut self, op: Op<'a>) -> usize {
        let code = self.code.len();
        self.code.push(op);
        code
    }

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

    pub fn parse_for(&mut self) -> Result<(), String> {
        self.advance_token(); 
        
        let loop_var = match self.current_token {
            Token::Ident(name) => name,
            _ => return Err("Expected identifier after 'for'".to_string()),
        };
        self.advance_token();
        
        self.expect(Token::In)?; 
        
        self.parse_expression()?;     
        self.code.push(Op::MakeIter); 
        
        let loop_start = self.code.len();
        let exit_jump = self.add_plug(Op::IterNext(0)); 
        
        let var_id = self.next_slot;
        self.next_slot += 1;
        let old_val = self.variables.insert(loop_var, (var_id, self.scope_depth));
        
        if self.scope_depth == 0 {
            self.code.push(Op::StoreGlobal(var_id));
        } else {
            self.code.push(Op::StoreLocal(var_id));
        }
        
        self.parse_block()?;
        
        self.code.push(Op::Pop); 
        self.code.push(Op::Jump(loop_start));
        self.patch_plug(exit_jump);
        
        self.code.push(Op::Pop);
        if let Some(prev) = old_val {
            self.variables.insert(loop_var, prev);
        } else {
            self.variables.remove(loop_var);
        }
        self.next_slot -= 1;
        
        Ok(())
    }

    fn expect(&mut self, token: Token) -> Result<(), String> {
        if !self.next_if(token) {
            return Err(format!("Expected token {:?}", token)); 
        } 
        Ok(())
    }

    fn parse_expression(&mut self) -> Result<(), String> {
        self.parse_range()
    }

    fn parse_range(&mut self) -> Result<(), String> {
        self.parse_logical_or()?;
        
        if self.current_token == Token::DotDot {
            self.advance_token();
            let incl = self.next_if(Token::Assign);
            self.parse_logical_or()?;
            self.code.push(Op::MakeRange(incl));
        }
        Ok(())
    }

    fn parse_logical_or(&mut self) -> Result<(), String> {
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

    fn parse_logical_and(&mut self) -> Result<(), String> {
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

    fn parse_equality(&mut self) -> Result<(), String> {
        self.parse_relational()?;

        while self.current_token == Token::Equal { 
            self.advance_token();
            self.parse_relational()?;
            self.code.push(Op::Equal);
        }
        Ok(())
    }

    fn parse_relational(&mut self) -> Result<(), String> {
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
    fn parse_arifm_or(&mut self) -> Result<(), String> {
        self.parse_arifm_and()?;
        while self.current_token == Token::ArifmOr {
            self.advance_token();
            self.parse_arifm_and()?;
            self.code.push(Op::ArifmOr);
        }
        Ok(())
    }

    fn parse_arifm_and(&mut self) -> Result<(), String> {
        self.parse_term()?;
        while self.current_token == Token::ArifmAnd {
            self.advance_token();
            self.parse_term()?;
            self.code.push(Op::ArifmAnd);
        }
        Ok(())
    }

    fn parse_term(&mut self) -> Result<(), String> {
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

    fn parse_factor(&mut self) -> Result<(), String> {
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

    fn parse_power(&mut self) -> Result<(), String> {
        self.parse_unary()?;

        if self.current_token == Token::Pow || self.current_token == Token::Mod {
            let oper = if self.current_token == Token::Pow {Op::Pow} else {Op::Mod};
            self.advance_token();
            self.parse_power()?;
            self.code.push(oper);
        }
        Ok(())
    }

    fn parse_unary(&mut self) -> Result<(), String> {
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
                    _ => return Err(format!("Expected identifier after prefix '{}'", if is_inc { "++" } else { "--" })),
                };
                self.advance_token();
                
                let (var_id, var_depth) = *self.variables.get(name).ok_or_else(|| format!("Unknown variable: {}", name))?;
                let is_global = var_depth == 0;
                
                let store_var = if self.current_token == Token::LBracket {
                    let mut deep: usize = 0;
                    if is_global {
                        self.code.push(Op::LoadGlobal(var_id));
                    } else {
                        self.code.push(Op::LoadLocal(var_id));
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
                        self.code.push(Op::StoreLocal(temp_slot));
                    }
                    
                    self.code.push(Op::StoreIndex(store_var));
                    if is_global {
                        self.code.push(Op::StoreGlobal(var_id));
                    } else {
                        self.code.push(Op::StoreLocal(var_id));
                    }  
                    
                    if self.scope_depth == 0 {
                        self.code.push(Op::LoadGlobal(temp_slot));
                    } else {
                        self.code.push(Op::LoadLocal(temp_slot));
                    }
                } else {
                    if is_global {
                        self.code.push(Op::LoadGlobal(var_id));
                    } else {
                        self.code.push(Op::LoadLocal(var_id));
                    }
                    self.code.push(Op::PushNumber(1));
                    self.code.push(op);
                    self.code.push(Op::Dup);
                    if is_global {
                        self.code.push(Op::StoreGlobal(var_id));
                    } else {
                        self.code.push(Op::StoreLocal(var_id));
                    }
                }
            }
            _ => {
                self.parse_primary()?;
            }
        }
        Ok(())
    }

   
    fn parse_primary(&mut self) -> Result<(), String> {
        match self.current_token {
            Token::String(s) => {
                self.advance_token();
                self.code.push(Op::PushStr(s));
            }
            Token::ArifmOr | Token::Or => {
                self.parse_fn(None, true)?;
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
                    _ => return Err("Expected Ok, Some or Err".to_string()),
                };
                self.advance_token();
                self.expect(Token::LParen)?;
                let name = match self.current_token {
                    Token::Ident(n) => n,
                    _ => return Err("Expected identifier".to_string()),
                };
                self.advance_token();
                self.expect(Token::RParen)?;
                self.expect(Token::Assign)?;
                self.parse_expression()?;

                let fail_jump = self.code.len();
                if is_right {
                    self.code.push(Op::SafeUnwR(0));
                } else {
                    self.code.push(Op::SafeUnwL(0));
                }

                let var_id = self.next_slot;
                self.next_slot += 1;

                let old_val = self.variables.insert(name, (var_id, self.scope_depth));
                self.scope_changes.push((name, old_val));

                if self.scope_depth == 0 {
                    self.code.push(Op::StoreGlobal(var_id));
                } else {
                    self.code.push(Op::StoreLocal(var_id));
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
                self.parse_expression()?;
                self.expect(Token::RParen)?;
            }
            _ => return Err(format!("Expected expression, got: {:?}", self.current_token)),
        }
        if self.next_if(Token::LBracket) {
            self.parse_expression()?;
            self.expect(Token::RBracket)?;
            let mut arg_count = 1;
            while self.next_if(Token::LBracket) {
                arg_count+=1; 
                self.parse_expression()?;
                self.expect(Token::RBracket)?;
            }
            self.code.push(Op::LoadIndex(arg_count));
        }
        while self.next_if(Token::Dot) {
            let method_name = if let Token::Ident(n) = self.current_token {
                self.advance_token();
                n 
            } else {
                return Err(format!("Expected NAME, after DOT(.) got: {:?}", self.current_token));
            };
            self.expect(Token::LParen)?;
            self.parse_func_call(method_name, true)?;
        }
        Ok(())
    }

    fn parse_let(&mut self) -> Result<(), String> {
        self.advance_token(); 
        let name = match self.current_token {
            Token::Ident(name) => name,
            _ => return Err("Expected identifier after 'let'".to_string()),
        };
        self.advance_token(); 

        self.expect(Token::Assign)?;
        self.parse_expression()?;
        self.next_if(Token::Semicolon);

        let var_id = self.next_slot;
        self.next_slot += 1;

        let old_value = self.variables.insert(name, (var_id, self.scope_depth));
        self.scope_changes.push((name, old_value));

        if self.scope_depth == 0 {
            self.code.push(Op::StoreGlobal(var_id));
        } else {
            self.code.push(Op::StoreLocal(var_id));
        }

        Ok(())
    }

    pub fn parse_fn(&mut self, func_name: Option<&'a str>, is_anonymous: bool) -> Result<(), String> {
        let func_id = if let Some(name) = func_name {
            let id = self.next_slot;
            self.next_slot += 1;
            let old_func_val = self.variables.insert(name, (id, self.scope_depth));
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
        
        if is_anonymous {
            if self.current_token == Token::ArifmOr {
                self.advance_token();
                while self.current_token != Token::ArifmOr {
                    let arg_name = if let Token::Ident(name) = self.current_token {
                        self.advance_token();
                        name
                    } else {
                        return Err("Need name after '|'".to_string());
                    }; 
                    
                    let arg_slot = self.next_slot;
                    self.next_slot += 1;
                    
                    old_vals.push(self.variables.insert(arg_name, (arg_slot, self.scope_depth)));
                    old_names.push(arg_name);
                    self.next_if(Token::Comma);
                }
                self.expect(Token::ArifmOr)?;
            } else if self.current_token == Token::Or {
                self.advance_token();
            }
        } else {
            self.expect(Token::LParen)?;
            while !self.next_if(Token::RParen) {
                let arg_name = if let Token::Ident(name) = self.current_token {
                    self.advance_token();
                    name
                } else {
                    return Err("Need name after '(..'".to_string());
                }; 
                
                let arg_slot = self.next_slot;
                self.next_slot += 1;
                
                old_vals.push(self.variables.insert(arg_name, (arg_slot, self.scope_depth)));
                old_names.push(arg_name);
                self.next_if(Token::Comma);
            }
        }
        
        self.parse_block()?;

        self.code.push(Op::Return);

        for (pos, old_val) in old_vals.iter().enumerate() {
            if let Some(prev) = old_val {
                self.variables.insert(old_names[pos], *prev);
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
                self.code.push(Op::StoreLocal(id));
            }
        }
        
        Ok(())
    }

    pub fn parse_block(&mut self) -> Result<(), String> {
        let was_open = self.next_if(Token::Begin);

        let old_next_slot = self.next_slot;
        let start_change_idx = self.scope_changes.len();

        let mut has_expression_value = false;

        while self.current_token != Token::End && self.current_token != Token::Eof {
            match self.current_token {
                Token::Let => {
                    self.parse_let()?;
                    has_expression_value = false;
                }
                Token::If => {
                    self.parse_if(false)?;
                    has_expression_value = true;
                }
                Token::For => {
                    self.parse_for()?;
                    has_expression_value = false;
                }
                Token::Func => {
                    self.advance_token();
                    let func_name = if let Token::Ident(name) = self.current_token {
                        self.advance_token();
                        Some(name)
                    } else {
                        return Err("Need name after 'fn'".to_string());
                    };
                    self.parse_fn(func_name, false)?;
                    has_expression_value = false;
                }
                Token::Match => {
                    self.parse_match()?;
                    has_expression_value = true;
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
                _ => {
                    self.parse_expression()?;
                    
                    if self.next_if(Token::Semicolon) {
                        self.code.push(Op::Pop);
                        has_expression_value = false;
                    } else {
                        has_expression_value = true;
                        if !matches!(self.current_token, Token::End | Token::Eof) && was_open {
                            return Err(format!("Expected ';' after expression or end of block, got: {:?}", self.current_token));
                        }
                        break;
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
                if let Some(prev_slot) = old_val {
                    self.variables.insert(name, prev_slot); 
                } else {
                    self.variables.remove(name); 
                }
            }
        }

        self.next_slot = old_next_slot;

        Ok(())
    }

    pub fn parse_match(&mut self) -> Result<(), String> {
        self.advance_token();
        self.parse_expression()?;
        self.expect(Token::Begin)?;

        let mut end_jumps = Vec::new();

        while self.current_token != Token::End && self.current_token != Token::Eof {
            let mut next_arm_jumps = Vec::new();
            let start_change_idx = self.scope_changes.len();
            let old_next_slot = self.next_slot;

            match self.current_token {
                Token::Ok | Token::Some | Token::Err => {
                    let is_right = matches!(self.current_token, Token::Ok | Token::Some);
                    self.advance_token();
                    self.expect(Token::LParen)?;
                    
                    let name = match self.current_token {
                        Token::Ident(n) => n,
                        _ => return Err("Expected identifier".to_string()),
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

                    let old_val = self.variables.insert(name, (var_id, self.scope_depth));
                    self.scope_changes.push((name, old_val));

                    if self.scope_depth == 0 {
                        self.code.push(Op::StoreGlobal(var_id));
                    } else {
                        self.code.push(Op::StoreLocal(var_id));
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
                            _ => return Err(format!("Expected literal pattern, got {:?}", self.current_token)),
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
                _ => return Err(format!("Expected pattern, got {:?}", self.current_token)),
            }

            if self.next_if(Token::If) {
                self.parse_expression()?;
                next_arm_jumps.push(self.code.len());
                self.code.push(Op::JumpIfFalse(0));
            }

            self.expect(Token::FatArrow)?;

            self.code.push(Op::Pop);
            self.parse_expression()?;
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

        Ok(())
    }

    pub fn parse_func_call(&mut self, name: &'a str, first_arg: bool) -> Result<(), String> {
        let mut arg_count = 0;
        if first_arg {
            arg_count += 1;
            let op = self.code.last_mut().unwrap();
            match op {
                Op::LoadLocal(i) => *op = Op::PushRefLocal(*i),   
                Op::LoadGlobal(i) => *op = Op::PushRefGlobal(*i), 
                _ => {},
            } 
        }
        while !self.next_if(Token::RParen) {
            self.parse_expression()?;
            arg_count += 1;
            self.next_if(Token::Comma);
        }

        if let Some(&(id, depth)) = self.variables.get(name) {
            if depth == 0 {
                self.code.push(Op::LoadGlobal(id));
            } else {
                self.code.push(Op::LoadLocal(id));
            }
        } else {
            self.code.push(Op::PushStr(name));
        }
        
        self.code.push(Op::CallFunc(arg_count));
        Ok(())
    }

    pub fn parse_ident(&mut self, name: &'a str) -> Result<(), String> {
        self.advance_token(); 
        
        if self.next_if(Token::LParen) {
            self.parse_func_call(name, false)?;
            return Ok(());
        }

        let (var_id, var_depth) = *self.variables.get(name).ok_or_else(|| format!("Unknown variable: {}", name))?;
        let is_global = var_depth == 0;

        let store_var = if self.current_token == Token::LBracket {
            let mut deep: usize = 0;
            if is_global {
                self.code.push(Op::LoadGlobal(var_id));
            } else {
                self.code.push(Op::LoadLocal(var_id));
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
                if store_var != 0 {
                    self.code.push(Op::StoreIndex(store_var));
                    if is_global {
                        self.code.push(Op::StoreGlobal(var_id));
                    } else {
                        self.code.push(Op::StoreLocal(var_id));
                    }
                } else {
                    if is_global {
                        self.code.push(Op::StoreGlobal(var_id));
                    } else {
                        self.code.push(Op::StoreLocal(var_id));
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
                    self.code.push(Op::StoreIndex(store_var));
                    if is_global {
                        self.code.push(Op::StoreGlobal(var_id));
                    } else {
                        self.code.push(Op::StoreLocal(var_id));
                    }
                } else {
                    if is_global {
                        self.code.push(Op::LoadGlobal(var_id));
                    } else {
                        self.code.push(Op::LoadLocal(var_id));
                    }
                    self.parse_expression()?;              
                    self.code.push(op);                    
                    if is_global {
                        self.code.push(Op::StoreGlobal(var_id));
                    } else {
                        self.code.push(Op::StoreLocal(var_id));
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
                        self.code.push(Op::StoreLocal(var_id));
                    }
                    
                    self.code.push(Op::LoadIndex(store_var));
                } else {
                    if is_global {
                        self.code.push(Op::LoadGlobal(var_id));
                    } else {
                        self.code.push(Op::LoadLocal(var_id));
                    }
                    self.code.push(Op::Dup); 
                    self.code.push(Op::PushNumber(1));
                    self.code.push(op);
                    if is_global {
                        self.code.push(Op::StoreGlobal(var_id));
                    } else {
                        self.code.push(Op::StoreLocal(var_id));
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
                        self.code.push(Op::LoadLocal(var_id));
                    }
                }
            }
        }
        
        Ok(())
    }
    
    pub fn parse_while(&mut self) -> Result<(), String> {
        self.advance_token();
        
        let jump_index = self.code.len();
        self.parse_expression()?;
        let plug = self.add_plug(Op::JumpIfFalse(0));

        self.parse_block()?;
        self.code.push(Op::Pop);
        self.code.push(Op::Jump(jump_index));

        self.patch_plug(plug);
        Ok(()) 
    }

    pub fn parse_if(&mut self, need_else: bool) -> Result<(), String> {
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
                return Err("need else branch".to_string())
            } else {
                self.code.push(Op::PushVoid);
            }
        }

        for i in vec.iter() {
            self.patch_plug(*i);
        }  

        Ok(())
    }

    pub fn parse_if_branch(&mut self) -> Result<(), String> {
        self.parse_expression()?;
        let plug = self.add_plug(Op::JumpIfFalse(0));

        self.parse_block()?;
        self.code[plug] = Op::JumpIfFalse(self.code.len() + 1);
        Ok(())
    }

    pub fn compile(mut self) -> Result<Vec<Op<'a>>, String> {
        self.parse_block()?;
        Ok(self.code)
    }
}
