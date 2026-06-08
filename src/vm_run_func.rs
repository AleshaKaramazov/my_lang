use std::{fs, io::{Read, Write}};
use crate::{
    consts, 
    op::Op, 
    value::Value, 
    vm::{CallFrame, VM}
};

impl<'a> VM {
    fn format(args: &[Value]) -> Result<String, String> {
        if args.is_empty() {
            return Err("Arguments are empty".to_string());
        }

        let format_str = args[0].to_string();
        let mut values = args.iter().skip(1);
        let mut output = String::with_capacity(format_str.len());
        let mut chars = format_str.chars().peekable();

        while let Some(c) = chars.next() {
            match c {
                '{' => match chars.peek() {
                    Some(&'{') => {
                        chars.next();
                        output.push('{');
                    }
                    Some(&'}') => {
                        chars.next();
                        let value = values.next().ok_or_else(|| "Missing argument".to_string())?;
                        output.push_str(&value.to_string());
                    }
                    _ => return Err("Invalid format string".to_string()),
                },
                '}' => {
                    if chars.peek() == Some(&'}') {
                        chars.next();
                        output.push('}');
                    } else {
                        return Err("Invalid format string".to_string());
                    }
                }
                _ => output.push(c),
            }
        }

        if values.next().is_some() {
            return Err("Too many arguments".to_string());
        }

        Ok(output)
    }

    fn write<W: Write>(mut file: W, args: &[Value], newline: bool) -> Result<(), String> {
        let output = Self::format(args)?;
        file.write_all(output.as_bytes()).map_err(|e| e.to_string())?;
        if newline {
            file.write_all(b"\n").map_err(|e| e.to_string())?;
        }
        file.flush().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn run_func(&mut self, funcname: &str, mut args: Vec<Value>, code: &[Op<'a>]) -> Result<(), String> {
         match funcname {
            "len" => {
                self.need_args(funcname, 1, args.len())?;
                let arg = self.deref(&mut args[0]);
                let res = match arg {
                    Value::Str(s) => Value::Number(s.chars().count() as i64),
                    Value::Set(s) => Value::Number(s.len() as i64),
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
            "readch" => {
                self.need_args(funcname, 1, args.len())?;
                let val = self.deref(&mut args[0]);
                match val {
                    Value::File(f) => {
                        let mut buffer = [0u8; consts::READ_AT_ONCE];
                        let res = match f.file.borrow_mut().read(&mut buffer) {
                            Err(e) => Err(
                                format!("Error while trying read the file:({}): {}", f, e)),
                            Ok(bb) => match 
                                String::from_utf8(buffer[..bb].to_vec()) {
                                    Ok(s) => Ok(Value::Str(s)),
                                    Err(e) => Err(e.to_string()),
                                }
                        };
                        self.stack.push(Value::new_control(res))
                    },
                    _ => return Err("read_chunk is only for files".to_string())
                };
            }
            "format" => {
                let format: String = Self::format(&args)?;
                self.stack.push(Value::Str(format));
            }
            "read" => {
                self.need_args(funcname, 1, args.len())?;
                let val = self.deref(&mut args[0]);
                match val {
                    Value::File(f) => {
                        let q = f.read();
                        self.stack.push(q)
                    }
                    Value::Str(filename) => {
                        let res = fs::read_to_string(filename).map(|x| Value::Str(x)).map_err(|x| x.to_string());
                        self.stack.push(Value::new_control(res));
                    }
                    _ => return Err("can't eval str or file fo read".to_string())
                }
            }
            "create" | "truncate" => {
                self.need_args(funcname, 1, args.len())?;
                let filename = args[0].eval_str()?;
                let file = Value::open_file(filename);
                self.stack.push(file); 
            }
            "open" => {
                self.need_args(funcname, 1, args.len())?;
                let filename = args[0].eval_str()?;
                let opt = if let Some(i) = args.get(1) {
                    i.expect_number()?
                } else {
                    consts::ALL_FLAGS
                };
                let res = Value::new_control(Value::new_file(filename, opt));
                self.stack.push(res); 
            }
            "is_ok" => {
                self.need_args(funcname, 1, args.len())?;
                let val = self.deref(&mut args[0]);
                let res = match val {
                    Value::Result(s) => Value::Bool(s.is_ok()),
                    _ => return Err("can't get result".to_string()),
                };
                self.stack.push(res);

            }
            "is_empty" => {
                self.need_args(funcname, 1, args.len())?;
                let val = self.deref(&mut args[0]);
                let res = match val {
                    Value::Str(s) => Value::Bool(s.is_empty()),
                    Value::Set(s) => Value::Bool(s.is_empty()),
                    _ => return Err("can't get result".to_string()),
                };
                self.stack.push(res);

            }
            "is_some" => {
                 self.need_args(funcname, 1, args.len())?;
                let val = self.deref(&mut args[0]);
                let res = match val {
                    Value::Cat(s) => Value::Bool(s.is_some()),
                    _ => return Err("can't eval cat".to_string()),
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
                let format = Self::format(&args)?;
                print!("{}", format);
                let _ = std::io::stdout().flush();
                let mut s = String::new();
                let _ = std::io::stdin().read_line(&mut s);
                self.stack.push(Value::Str(s.trim().to_string()));
            }
            "parse" => {
                self.need_args(funcname, 1, args.len())?;
                let arg = self.deref(&mut args[0]).eval_str()?;
                let res = match arg.parse() {
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
            "lines" => {
                self.need_args(funcname, 1, args.len())?;
                let arg = self.deref(&mut args[0]).eval_str()?;
                let res = Value::Set(arg.lines().map(|x| Value::Str(x.to_string())).collect());
                self.stack.push(res);
            }
            "split_whitespace" => {
                self.need_args(funcname, 1, args.len())?;
                let arg = self.deref(&mut args[0]).eval_str()?;
                let res = Value::Set(arg.split_whitespace().map(|x| Value::Str(x.to_string())).collect());
                self.stack.push(res);
            }
            "split" => {
                self.need_args(funcname, 2, args.len())?;
                let spliter = args[1].eval_str()?;
                let res = match &args[0] {
                    Value::Ref(i) => match &self.frame[*i] {
                        Value::Str(arg) => Value::Set(arg.split(spliter).map(|x| Value::Str(x.to_string())).collect()),
                        _ => return Err("can't split".to_string())
                    }
                    Value::Str(arg) => Value::Set(arg.split(spliter).map(|x| Value::Str(x.to_string())).collect()),
                    _ => return Err("can't split".to_string())
                };
                self.stack.push(res);
            }
            "contains" => {
                self.need_args(funcname, 2, args.len())?;
                let arg = self.deref(&mut args[0]).to_string();
                let pattern = args[1].to_string();

                let res = if let Some(Value::Bool(i)) = args.get(2) && *i {
                    if let Some(Value::Bool(i)) = args.get(3) && *i {
                        arg.to_lowercase().contains(&pattern) 
                    } else {
                        arg.to_lowercase().contains(&pattern.to_lowercase()) 
                    }
                } else {
                    arg.contains(&pattern) 
                };
                self.stack.push(Value::Bool(res));
            }
            "to_lower" => {
                self.need_args(funcname, 1, args.len())?;
                let arg = self.deref(&mut args[0]).eval_str()?;
                let res = Value::Str(arg.to_lowercase());
                self.stack.push(res);
            }
            "to_upper" => {
                self.need_args(funcname, 1, args.len())?;
                let arg = self.deref(&mut args[0]).eval_str()?;
                let res = Value::Str(arg.to_uppercase());
                self.stack.push(res);
            }
            i if i.starts_with("write") => {
                self.need_args(funcname, 1, args.len())?;
                let new_line = i.ends_with("ln");

                let res = if let Some((first, rest)) = args.split_first_mut() {
                    Self::write(first, rest, new_line).map(|_| Value::Void)
                } else {unreachable!()};

                self.stack.push(Value::new_control(res));
            }
            i if i.starts_with("print") => {
                let new_line = i.ends_with("ln"); 
                let res = Self::write(std::io::stdout(), &args, new_line).map(|_| Value::Void);
                self.stack.push(Value::new_control(res));
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
            "clear_console" => {
                print!("{esc}[2J{esc}[1;1H", esc = 27 as char);
                let _ = std::io::stdout().flush();
                self.stack.push(Value::Void)
            },
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

    #[inline(always)]
    pub fn deref(&'a mut self, arg: &'a mut Value) -> &'a mut Value {
        match arg {
            Value::Ref(i) => &mut self.frame[*i],
            _ => arg,
        }
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
