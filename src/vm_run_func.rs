use std::io::Write;
use crate::{
    consts, 
    op::Op, 
    value::Value, 
    vm::{CallFrame, VM}
};

impl<'a> VM {
    pub fn run_func(&mut self, funcname: &str, args: Vec<Value>, code: &[Op<'a>]) -> Result<(), String> {
         match funcname {
            "len" => {
                self.need_args(funcname, 1, args.len())?;
                let res = match &args[0] {
                    Value::Str(s) => Value::Number(s.chars().count() as i64),
                    Value::Set(s) => Value::Number(s.len() as i64),
                    Value::Ref(idx) => match &self.frame[*idx] {
                        Value::Str(s) => Value::Number(s.chars().count() as i64),
                        Value::Set(s) => Value::Number(s.len() as i64),
                        unk => return Err(format!("can't get len: {}", unk)),
                    }
                    _ => return Err("can't get len".to_string()),
                };
                self.stack.push(res);
            }
            "starts_with" => {
                self.need_args(funcname, 2, args.len())?;
                let res = match &args[0] {
                    Value::Str(s) => s,
                    Value::Ref(idx) => match &self.frame[*idx] {
                        Value::Str(s) => s,
                        unk => return Err(format!("can't check starts_with: {}", unk)),
                    }
                    unk => return Err(format!("can't check starts_with: {}", unk)),
                };

                let res = match &args[1] {
                    Value::Char(c) => res.starts_with(*c),
                    Value::Str(c) => res.starts_with(c),
                    unk => res.starts_with(&unk.to_string()) 
                };
                self.stack.push(Value::Bool(res));
            }
            "is_some" => {
                self.need_args(funcname, 1, args.len())?;
                let res = match &args[0] {
                    Value::Cat(s) => Value::Bool(s.is_some()),
                    Value::Ref(idx) => match &self.frame[*idx] {
                        Value::Cat(s) => Value::Bool(s.is_some()),
                        unk => return Err(format!("can't get len: {}", unk)),
                    }
                    _ => return Err("can't get len".to_string()),
                };
                self.stack.push(res);
            }
            "push" => {
                self.need_args(funcname, 1, args.len())?;
                let id = match &args[0] {
                    Value::Ref(idx) => *idx,
                    _ => return Ok(()),
                };

                match &mut self.frame[id] {
                    Value::Set(set) => set.push(args[1].clone()),
                    _ => return Err("can't push".to_string()),
                }
                self.stack.push(Value::Void);
            }
            "readln" => {
                for (i, arg) in args.iter().enumerate() {
                    print!("{}", arg);
                    if i < args.len() - 1 {
                        print!(" ");
                    }
                }
                let _ = std::io::stdout().flush();
                let mut s = String::new();
                let _ = std::io::stdin().read_line(&mut s);
                self.stack.push(Value::Str(s.trim().to_string()));
            }
            "parse" => {
                self.need_args(funcname, 1, args.len())?;
                let res = match &args[0] {
                    Value::Str(s) => s.parse(),
                    Value::Ref(idx) => match &self.frame[*idx] {
                        Value::Str(s) => s.parse(),
                        unk => return Err(format!("can't parse: {}", unk)),
                    }
                    _ => return Err("can't get parse".to_string()),
                };
                let res = match res {
                    Ok(num) => Value::Result(Box::new(Ok(Value::Number(num)))),
                    Err(e) => Value::Result(Box::new(Err(Value::Str(e.to_string())))),
                };
                self.stack.push(res);
            }
            "step" => {
                self.need_args(funcname, 1, args.len())?;
                let mut arg = if let Value::Range(i) = &args[0] {
                    i.clone()
                } else {
                    return Err("Step only for ranges".to_string());
                };
                arg.step = args[1].expect_number()?;
                self.stack.push(Value::Range(arg));
            }
            "writeln" => {
                for (i, arg) in args.iter().enumerate() {
                    print!("{}", arg);
                    if i < args.len() - 1 {
                        print!(" ");
                    }
                }
                println!();
                self.stack.push(Value::Void);
            }
            "filter_map" => {
                self.need_args(funcname, 2, args.len())?;
                let set = match &args[0] {
                    Value::Set(s) => s.clone(),
                    Value::Ref(idx) => match &self.frame[*idx] {
                        Value::Set(s) => s.clone(),
                        _ => return Err("map requires a set".to_string()),
                    },
                    _ => return Err("map requires a set".to_string()),
                };
                
                let lambda_ip = match args[1] {
                    Value::Fn(ip) => ip,
                    _ => return Err("map requires a lambda".to_string()),
                };

                let mut result_set = Vec::new();
                
                for item in set {
                    self.run_lambda(code, lambda_ip, vec![item.clone()])?;
                    
                    let result = self.stack.pop().ok_or("VM Error: Expected bool from lambda")?;
                    match result {
                        Value::Cat(res) => {
                            if let Some(res) =  res {
                                result_set.push(*res)
                            }
                        }
                        _ => return Err("lambda in filter_map need to return Cat<Option<Value>>".to_string()), 
                    }
                }
                
                self.stack.push(Value::Set(result_set));

            }
            "map" => {
                self.need_args(funcname, 2, args.len())?;
                let set = match &args[0] {
                    Value::Set(s) => s.clone(),
                    Value::Ref(idx) => match &self.frame[*idx] {
                        Value::Set(s) => s.clone(),
                        _ => return Err("map requires a set".to_string()),
                    },
                    _ => return Err("map requires a set".to_string()),
                };
                
                let lambda_ip = match args[1] {
                    Value::Fn(ip) => ip,
                    _ => return Err("map requires a lambda".to_string()),
                };

                let mut result_set = Vec::new();
                
                for item in set {
                    self.run_lambda(code, lambda_ip, vec![item.clone()])?;
                    
                    let result = self.stack.pop().ok_or("VM Error: Expected bool from lambda")?;
                    result_set.push(result);
                }
                
                self.stack.push(Value::Set(result_set));

            }
            "filter" => {
                self.need_args(funcname, 2, args.len())?;
                let set = match &args[0] {
                    Value::Set(s) => s.clone(),
                    Value::Ref(idx) => match &self.frame[*idx] {
                        Value::Set(s) => s.clone(),
                        _ => return Err("filter requires a set".to_string()),
                    },
                    _ => return Err("filter requires a set".to_string()),
                };
                
                let lambda_ip = match args[1] {
                    Value::Fn(ip) => ip,
                    _ => return Err("filter requires a lambda".to_string()),
                };

                let mut result_set = Vec::new();
                
                for item in set {
                    self.run_lambda(code, lambda_ip, vec![item.clone()])?;
                    
                    let cond = self.stack.pop().ok_or("VM Error: Expected bool from lambda")?;
                    if cond.is_truthy() {
                        result_set.push(item);
                    }
                }
                
                self.stack.push(Value::Set(result_set));
            }
            _ => return Err(format!("VM Error: Unknown function '{}'", funcname)),
        }
        Ok(())
    } 

    #[inline(always)]
    pub fn need_args(&mut self, funcname: &str, need: usize, have: usize) -> Result<(), String> {
        if need > have {
            return Err(format!("function: {} need at least: {} args, have: {}", funcname, need, have)) 
        }
        Ok(())
    }

    pub fn run_lambda(&mut self, code: &[Op<'a>], target_ip: usize, args: Vec<Value>) -> Result<(), String> {
        self.call_stack.push(CallFrame {
            return_ip: consts::STOP_FLAG, 
            old_frame: self.now_frame,
        });

        self.now_frame = self.frame.len();
        self.frame.extend(args);

        let mut ip = target_ip;
        while ip != consts::STOP_FLAG && ip < code.len() {
            self.step(&code, &mut ip)?;
        }

        Ok(())
    }
}
