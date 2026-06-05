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
            _ => unreachable!("VM Error: Attempted to patch a non-jump instruction!"),
        }
    }

    fn expect(&mut self, token: Token) -> Result<(), String> {
        if !self.next_if(token) {
            return Err(format!("Expected token {:?}", token)); 
        } 
        Ok(())
    }

    fn parse_expression(&mut self) -> Result<(), String> {
        self.parse_logical_or()
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
        self.parse_arifm_or()?;

        while self.current_token == Token::LogicalAnd { 
            self.advance_token();

            let jump_false_1 = self.add_plug(Op::JumpIfFalse(0));
            
            self.parse_arifm_or()?;
            
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
            Token::Inc => {
                self.advance_token();
                let name = match self.current_token {
                    Token::Ident(n) => n,
                    _ => return Err("Expected identifier after prefix '++'".to_string()),
                };
                self.advance_token();
                let var_id = *self.variables.get(name).ok_or_else(|| format!("Unknown variable: {}", name))?;
                
                self.code.push(Op::LoadLocal(var_id));
                self.code.push(Op::PushNumber(1));
                self.code.push(Op::Plus);
                self.code.push(Op::Dup);
                self.code.push(Op::StoreLocal(var_id));
            }
            Token::Dec => {
                self.advance_token();
                let name = match self.current_token {
                    Token::Ident(n) => n,
                    _ => return Err("Expected identifier after prefix '--'".to_string()),
                };
                self.advance_token();
                let var_id = *self.variables.get(name).ok_or_else(|| format!("Unknown variable: {}", name))?;
                
                self.code.push(Op::LoadLocal(var_id));
                self.code.push(Op::PushNumber(1));
                self.code.push(Op::Sub);
                self.code.push(Op::Dup);
                self.code.push(Op::StoreLocal(var_id));
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
            Token::Number(n) => {
                self.advance_token();
                self.code.push(Op::PushNumber(n));
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
        self.expect(Token::Semicolon)?;

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

        self.expect(Token::End)?; 

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

    pub fn parse_ident(&mut self, name: &'a str) -> Result<(), String> {
        self.advance_token(); 
        
        if self.next_if(Token::LParen) {
            let mut arg_count = 0;
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
            return Ok(());
        }

        let var_id = *self.variables.get(name).ok_or_else(|| format!("Unknown variable: {}", name))?;

        match self.current_token {
            Token::Assign => {
                self.advance_token();
                self.parse_expression()?; 
                self.code.push(Op::StoreLocal(var_id));
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
                
                self.code.push(Op::LoadLocal(var_id)); 
                self.parse_expression()?;              
                self.code.push(op);                    
                self.code.push(Op::StoreLocal(var_id));
                self.code.push(Op::PushVoid);
            }
            Token::Inc => {
                self.advance_token();
                self.code.push(Op::LoadLocal(var_id));
                self.code.push(Op::Dup); 
                self.code.push(Op::PushNumber(1));
                self.code.push(Op::Plus);
                self.code.push(Op::StoreLocal(var_id));
            }
            Token::Dec => {
                self.advance_token();
                self.code.push(Op::LoadLocal(var_id));
                self.code.push(Op::Dup);
                self.code.push(Op::PushNumber(1));
                self.code.push(Op::Sub);
                self.code.push(Op::StoreLocal(var_id));
            }
            _ => {
                self.code.push(Op::LoadLocal(var_id));
            }
        }
        
        Ok(())
    }    
    pub fn compile(mut self) -> Result<Vec<Op<'a>>, String> {
        self.advance_token(); 

        while self.current_token != Token::Eof {
            match self.current_token {
                Token::Let => self.parse_let()?,
                Token::Begin => {
                    self.parse_block()?;
                    self.code.push(Op::Pop);
                }
                Token::Ident(name) => {
                    self.parse_ident(name)?; 
                    self.next_if(Token::Semicolon);
                }
                _ => return Err(format!("Unexpected token: {:?}", self.current_token)),
            }
        }
         
        Ok(self.code)
    }
}
