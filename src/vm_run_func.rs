use std::{fs, io::{Read, Write}, rc::Rc};
use crate::op::Op;
use crate::consts;
use crate::errors::VMError;
use crate::value::{Iterator, Value};
use crate:: vm::{CallFrame, VM};

impl<'a> VM {
    fn format(args: &[Value]) -> Result<String, VMError> {
        if args.is_empty() {
            return Err(VMError::NeedMoreArgs);
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
                        let value = values.next().ok_or(VMError::NeedMoreArgs)?;
                        output.push_str(&value.to_string());
                    }
                    _ => return Err(VMError::BadArgument),
                },
                '}' => {
                    if chars.peek() == Some(&'}') {
                        chars.next();
                        output.push('}');
                    } else {
                        return Err(VMError::BadArgument)
                    }
                }
                _ => output.push(c),
            }
        }

        if values.next().is_some() {
            return Err(VMError::TooManyArgs);
        }

        Ok(output)
    }

    fn write<W: Write>(mut file: W, args: &[Value], newline: bool) -> Result<(), VMError> {
        let output = Self::format(args)?;
        file.write_all(output.as_bytes()).map_err(|_| VMError::WriteError)?;
        if newline {
            file.write_all(b"\n").map_err(|_| VMError::WriteError)?;
        }
        file.flush().map_err(|_| VMError::WriteError)?;
        Ok(())
    }

    pub fn run_func(&mut self, funcname: &str, mut args: Vec<Value>, code: &[Op<'a>]) -> Result<(), VMError> {
         match funcname {
            "len" => {
                self.need_args(1, args.len())?;
                let arg = self.deref(&mut args[0]);
                let res = match arg {
                    Value::Str(s) => Value::Number(s.chars().count() as i64),
                    Value::Set(s) => Value::Number(s.len() as i64),
                    _ => return Err(VMError::BadArgument),
                };
                self.push(res);
            }
            "starts_with" => {
                self.need_args(2, args.len())?;
                let res = match &args[0] {
                    Value::Str(s) => s,
                    Value::Ref(idx) => match &self.frame[*idx] {
                        Value::Str(s) => s,
                        _ => return Err(VMError::BadArgument),
                    }
                    _ => return Err(VMError::BadArgument),
                };

                let res = match &args[1] {
                    Value::Char(c) => res.starts_with(*c),
                    Value::Str(c) => res.starts_with(&**c),
                    unk => res.starts_with(&unk.to_string()) 
                };
                self.push(Value::Bool(res));
            }
            "readch" => {
                self.need_args(1, args.len())?;
                let val = self.deref(&mut args[0]);
                
                let res = match val {
                    Value::File(f) => {
                        let mut file_ref = f.file.try_borrow_mut()
                            .map_err(|_| VMError::FuncErr)?;
                            
                        let mut buffer = vec![0u8; consts::READ_AT_ONCE];
                        
                        

                        file_ref.read(&mut buffer)
                            .map_err(|e| format!("Error while trying read the file ({}): {}", f, e)).map(|bb| String::from_utf8_lossy(&buffer[..bb])
                                    .into_owned())
                            .map(|x| Value::Str(Rc::new(x)))
                    },
                    _ => return Err(VMError::BadArgument)
                };
                self.push(Value::new_control(res));
            }

            "format" => {
                let format: String = Self::format(&args)?;
                self.push(Value::Str(Rc::new(format)));
            }
            "enumerate" => {
                self.need_args(1, args.len())?;
                
                let target = self.deref(&mut args[0]).clone().make_iter()?;
                self.push(Value::Iter(Box::new(crate::value::Iterator::Enumerate(Box::new(target), 0))));
            }
            "read" => {
                self.need_args(1, args.len())?;
                let val = self.deref(&mut args[0]);
                match val {
                    Value::File(f) => {
                        let q = f.read();
                        self.push(q)
                    }
                    Value::Str(filename) => {
                        let res = fs::read_to_string(&**filename).map(|x| Value::Str(Rc::new(x))).map_err(|x| x.to_string());
                        self.push(Value::new_control(res));
                    }
                    _ => return Err(VMError::BadArgument)
                }
            }
            "create" | "truncate" => {
                self.need_args(1, args.len())?;
                let filename = args[0].eval_str()?;
                let file = Value::open_file(filename);
                self.push(file); 
            }
            "open" => {
                self.need_args(1, args.len())?;
                let filename = args[0].eval_str()?;
                let opt = if let Some(i) = args.get(1) {
                    i.expect_number()?
                } else {
                    consts::ALL_FLAGS
                };
                let res = Value::new_control(Value::new_file(filename, opt).map_err(|_| format!("error with open: {}", filename)));
                self.push(res); 
            }
            "is_ok" => {
                self.need_args(1, args.len())?;
                let val = self.deref(&mut args[0]);
                let res = match val {
                    Value::Result(s) => Value::Bool(s.is_ok()),
                    _ => return Err(VMError::BadArgument),
                };
                self.push(res);

            }
            "is_empty" => {
                self.need_args(1, args.len())?;
                let val = self.deref(&mut args[0]);
                let res = match val {
                    Value::Str(s) => Value::Bool(s.is_empty()),
                    Value::Set(s) => Value::Bool(s.is_empty()),
                    _ => return Err(VMError::BadArgument),
                };
                self.push(res);

            }
            "is_some" => {
                self.need_args(1, args.len())?;
                let val = self.deref(&mut args[0]);
                let res = match val {
                    Value::Cat(s) => Value::Bool(s.is_some()),
                    _ => return Err(VMError::BadArgument),
                };
                self.push(res);
            }
            "push" => {
                self.need_args(1, args.len())?;
                let id = match &args[0] {
                    Value::Ref(idx) => *idx,
                    _ => return Ok(()),
                };

                match &mut self.frame[id] {
                    Value::Set(set) =>{
                        let set = Rc::make_mut(set);
                        set.push(args[1].clone())
                    }
                    _ => return Err(VMError::BadArgument),
                }
                self.push(Value::Void);
            }
            "readln" => {
                if !args.is_empty() {
                    let format = Self::format(&args)?;
                    print!("{}", format);
                    let _ = std::io::stdout().flush();
                }
                let mut s = String::new();
                let _ = std::io::stdin().read_line(&mut s);
                self.push(Value::Str(Rc::new(s.trim().to_string())));
            }
            "parse" => {
                self.need_args(1, args.len())?;
                let arg = self.deref(&mut args[0]).eval_str()?;
                let res = match arg.parse() {
                    Ok(num) => Value::Result(Box::new(Ok(Value::Number(num)))),
                    Err(e) => Value::Result(Box::new(Err(Value::Str(Rc::new(e.to_string()))))),
                };
                self.push(res);
            }
            "step" => {
                self.need_args(1, args.len())?;
                let mut arg = if let Value::Iter(i) = &args[0]
                    && let Iterator::Range(r) = **i {
                        r
                } else {
                    return Err(VMError::BadArgument)
                };
                arg.step = args[1].expect_number()?;
                self.push(Value::Iter(Box::new(Iterator::Range(arg))));
            }
            "lines" => {
                self.need_args(1, args.len())?;
                let arg = self.deref(&mut args[0]).eval_str()?;
                
                let res = Value::Iter(Box::new(crate::value::Iterator::Lines(crate::value::LinesIter {
                    source: arg.to_string(),
                    offset: 0,
                })));
                
                self.push(res);
            }
            "split_whitespace" => {
                self.need_args(1, args.len())?;
                let arg = self.deref(&mut args[0]).eval_str()?;
                
                let res = Value::Iter(Box::new(crate::value::Iterator::SplitWhitespace(crate::value::SplitWhitespaceIter {
                    source: arg.to_string(),
                    offset: 0,
                })));
                self.push(res);
            }
            "split" => {
                self.need_args(2, args.len())?;
                let delimiter = args[1].eval_str()?.to_string();
                
                let source = match &args[0] {
                    Value::Ref(i) => self.frame[*i].eval_str()?,
                    Value::Str(arg) => arg.as_str(),
                    _ => return Err(VMError::BadArgument),
                }.to_string();
                
                let res = Value::Iter(Box::new(crate::value::Iterator::Split(crate::value::SplitIter {
                    source,
                    delimiter,
                    offset: 0,
                })));
                self.push(res);
            }
            "nth" => {
                self.need_args(2, args.len())?;
                let n = args[1].expect_number()?;
                if n < 0 {
                    return Err(VMError::BadArgument);
                }

                let mut result = None;
                {
                    let iter_ref = self.deref(&mut args[0]);
                    
                    for _ in 0..=n {
                        match iter_ref.next()? {
                            Some(val) => result = Some(val),
                            None => {
                                result = None;  
                                break;
                            }
                        }
                    }
                }

                let res = match result {
                    Some(val) => Value::Cat(Some(Box::new(val))),
                    None => Value::Cat(None),
                };
                self.push(res);
            }
            "collect" => {
                self.need_args(1, args.len())?;
                let mut result_set = Vec::new();
                
                {
                    let iter_ref = self.deref(&mut args[0]);
                    while let Some(val) = iter_ref.next()? {
                        result_set.push(val);
                    }
                }
                
                self.push(Value::Set(Rc::new(result_set)));
            }
            "contains" => {
                self.need_args(2, args.len())?;
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
                self.push(Value::Bool(res));
            }
            "to_lower" => {
                self.need_args(1, args.len())?;
                let arg = self.deref(&mut args[0]).eval_str()?;
                let res = Value::Str(Rc::new(arg.to_lowercase()));
                self.push(res);
            }
            "to_upper" => {
                self.need_args(1, args.len())?;
                let arg = self.deref(&mut args[0]).eval_str()?;
                let res = Value::Str(Rc::new(arg.to_uppercase()));
                self.push(res);
            }
            i if i.starts_with("write") => {
                self.need_args(1, args.len())?;
                let new_line = i.ends_with("ln");

                let res = if let Some((first, rest)) = args.split_first_mut() {
                    Self::write(first, rest, new_line).map(|_| Value::Void).map_err(|_| "error with write".to_string())
                } else {unreachable!()};

                self.push(Value::new_control(res));
            }
            i if i.starts_with("print") => {
                let new_line = i.ends_with("ln"); 
                let res = 
                    Self::write(std::io::stdout(), &args, new_line).map(|_| Value::Void).map_err(|_| "error with write into: stdout".to_string());
                self.push(Value::new_control(res));
            }
            "filter_map" => {
                self.need_args(2, args.len())?;
                let set = match &args[0] {
                    Value::Set(s) => s.clone(),
                    Value::Ref(idx) => match &self.frame[*idx] {
                        Value::Set(s) => s.clone(),
                        _ => return Err(VMError::BadArgument),
                    },
                    _ => return Err(VMError::BadArgument),
                };
                
                let (lambda_ip, stk) = match args[1] {
                    Value::Fn(ip, stk) => (ip as usize, stk as usize),
                    _ => return Err(VMError::BadArgument),
                };

                let mut result_set = Vec::new();
                
                for item in set.iter() {
                    self.run_lambda(code, lambda_ip,vec![item.clone()], stk)?;
                    
                    let result = self.pop();
                    match result {
                        Value::Cat(res) => {
                            if let Some(res) =  res {
                                result_set.push(*res)
                            }
                        }
                        _ => return Err(VMError::BadArgument), 
                    }
                }
                self.push(Value::Set(Rc::new(result_set)));

            }
            "map" => {
                self.need_args(2, args.len())?;
                let set = match &args[0] {
                    Value::Set(s) => s.clone(),
                    Value::Ref(idx) => match &self.frame[*idx] {
                        Value::Set(s) => s.clone(),
                        _ => return Err(VMError::BadArgument),
                    },
                    _ => return Err(VMError::BadArgument),
                };
                
                let (lambda_ip, stk) = match args[1] {
                    Value::Fn(ip, stk) => (ip as usize, stk as usize),
                    _ => return Err(VMError::BadArgument),
                };

                let mut result_set = Vec::new();
                
                for item in set.iter() {
                    self.run_lambda(code, lambda_ip, vec![item.clone()], stk)?;
                    
                    let result = self.pop();
                    result_set.push(result);
                }
                
                self.push(Value::Set(Rc::new(result_set)));

            }
            "clear_console" => {
                print!("{esc}[2J{esc}[1;1H", esc = 27 as char);
                let _ = std::io::stdout().flush();
                self.push(Value::Void)
            },
            "filter" => {
                self.need_args(2, args.len())?;
                let set = match &args[0] {
                    Value::Set(s) => s.clone(),
                    Value::Ref(idx) => match &self.frame[*idx] {
                        Value::Set(s) => s.clone(),
                        _ => return Err(VMError::BadArgument),
                    },
                    _ => return Err(VMError::BadArgument),
                };
                
                let (lambda_ip , stk) = match args[1] {
                    Value::Fn(ip, stk) => (ip as usize, stk as usize),
                    _ => return Err(VMError::BadArgument),
                };

                let mut result_set = Vec::new();
                
                for item in set.iter() {
                    self.run_lambda(code, lambda_ip, vec![item.clone()], stk)?;
                    
                    let cond = self.pop();
                    if cond.is_truthy() {
                        result_set.push(item.clone());
                    }
                }
                
                self.push(Value::Set(Rc::new(result_set)));
            }
            _ => return Err(VMError::UnknownFunc),
        }
        Ok(())
    } 

    #[inline(always)]
    pub fn need_args(&mut self, need: usize, have: usize) -> Result<(), VMError> {
        if need > have {
            return Err(VMError::NeedMoreArgs) 
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
   
    pub fn run_lambda(&mut self, code: &[Op<'a>], target_ip: usize, args: Vec<Value>, static_link: usize) -> Result<(), VMError> {
        self.call_stack.push(CallFrame {
            return_ip: consts::STOP_FLAG, 
            old_frame: self.now_frame,
            static_link,
            frame_idx: self.frame.len()
        });

        self.now_frame = self.frame.len();
        self.frame.extend(args);

        self.run(code, target_ip)?;
        Ok(())
    }
}
