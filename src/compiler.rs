use crate::{consts, lexer::{Lexer, Token}, op::Op};
use rustc_hash::FxHashMap;

pub struct Compiler<'a> {
    code: Vec<Op<'a>>,
    current_token: Token<'a>,
    lexer: Lexer<'a>,
    variables: FxHashMap<&'a str, usize>,
    next_slot: usize,
    scope_changes: Vec<(&'a str, Option<usize>)>,
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
        let old_val = self.variables.insert(loop_var, var_id);
        
        self.code.push(Op::StoreLocal(var_id));
        
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

        if self.current_token == Token::Pow {
            self.advance_token();
            self.parse_power()?;
            self.code.push(Op::Pow);
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
                
                let var_id = *self.variables.get(name).ok_or_else(|| format!("Unknown variable: {}", name))?;
                
                let store_var = if self.current_token == Token::LBracket {
                    let mut deep: usize = 0;
                    self.code.push(Op::LoadLocal(var_id));
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
                    self.code.push(Op::StoreLocal(temp_slot));
                    
                    self.code.push(Op::StoreIndex(store_var));
                    self.code.push(Op::StoreLocal(var_id));  
                    
                    self.code.push(Op::LoadLocal(temp_slot)); 
                } else {
                    self.code.push(Op::LoadLocal(var_id));
                    self.code.push(Op::PushNumber(1));
                    self.code.push(op);
                    self.code.push(Op::Dup);
                    self.code.push(Op::StoreLocal(var_id));
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

        let old_value = self.variables.insert(name, var_id);
        self.scope_changes.push((name, old_value));

        self.code.push(Op::StoreLocal(var_id));

        Ok(())
    }

    pub fn parse_block(&mut self) -> Result<(), String> {
        self.expect(Token::Begin)?; 

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
                Token::While => {
                    self.parse_while()?;
                    has_expression_value = false;
                }
                Token::Begin => {
                    self.parse_block()?;
                    self.code.push(Op::Pop);
                    has_expression_value = true;
                }
                Token::Ident(name) => {
                    self.parse_ident(name)?; 
                    self.next_if(Token::Semicolon);
                    has_expression_value = true;
                }
                _ => {
                    self.parse_expression()?;
                    
                    if self.next_if(Token::Semicolon) {
                        self.code.push(Op::Pop);
                        has_expression_value = false;
                    } else {
                        has_expression_value = true;
                        if self.current_token != Token::End {
                            return Err("Expected ';' after expression or end of block".to_string());
                        }
                        break;
                    }
                }
            }
        }

        self.next_if(Token::End);

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

    pub fn parse_func_call(&mut self, name: &'a str, first_arg: bool) -> Result<(), String> {
        let mut arg_count = 0;
        if first_arg {
            arg_count += 1;
            let op = self.code.last_mut().unwrap();
            match op {
                Op::LoadLocal(i) => *op = Op::PushRef(*i),
                _ => {},
            } 
        }
        if self.current_token != Token::RParen {
            loop {
                self.parse_expression()?;
                arg_count += 1;
                if !self.next_if(Token::Comma) {
                    break;
                }
            }
        }
        self.expect(Token::RParen)?;
        self.code.push(Op::PushStr(name));
        self.code.push(Op::CallFunc(arg_count));
        Ok(())
    }

    pub fn parse_ident(&mut self, name: &'a str) -> Result<(), String> {
        self.advance_token(); 
        
        if self.next_if(Token::LParen) {
            self.parse_func_call(name, false)?;
            return Ok(());
        }

        let var_id = *self.variables.get(name).ok_or_else(|| format!("Unknown variable: {}", name))?;

        let store_var = if self.current_token == Token::LBracket {
            let mut deep: usize = 0;
            self.code.push(Op::LoadLocal(var_id));
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
                    self.code.push(Op::StoreLocal(var_id));
                } else {
                    self.code.push(Op::StoreLocal(var_id));
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
                    self.code.push(Op::StoreLocal(var_id));
                } else {
                    self.code.push(Op::LoadLocal(var_id)); 
                    self.parse_expression()?;              
                    self.code.push(op);                    
                    self.code.push(Op::StoreLocal(var_id));
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
                    self.code.push(Op::StoreLocal(var_id));
                    
                    self.code.push(Op::LoadIndex(store_var));
                } else {
                    self.code.push(Op::LoadLocal(var_id));
                    self.code.push(Op::Dup); 
                    self.code.push(Op::PushNumber(1));
                    self.code.push(op);
                    self.code.push(Op::StoreLocal(var_id));
                }
            }
            _ => {
                if store_var != 0 {
                    self.code.push(Op::LoadIndex(store_var));
                } else {
                    self.code.push(Op::LoadLocal(var_id));
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
                self.parse_expression()?;
            }
        }
        for i in vec.iter() {
            self.patch_plug(*i);
        }       
        if (need_else && !has_else) || (!has_else && self.current_token == Token::End) {
            return Err("need else branch".to_string())
        }
        Ok(())
    }

    pub fn parse_if_branch(&mut self) -> Result<(), String> {
        self.parse_expression()?;
        let plug = self.add_plug(Op::JumpIfFalse(0));

        self.parse_expression()?;
        self.code[plug] = Op::JumpIfFalse(self.code.len() + 1);
        Ok(())
    }

    pub fn compile(mut self) -> Result<Vec<Op<'a>>, String> {
        self.parse_block()?;
        Ok(self.code)
    }
}
