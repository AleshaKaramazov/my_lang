use std::{cell::RefCell, fs, io::{Read, Seek, Write}, rc::Rc};
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

    pub fn run_func(&mut self, func: i64, args_count: usize, code: &[Op<'a>]) -> Result<(), VMError> {
         match func { 
            1 => {
                let arg = self.deref(&self.stack[self.sp]);
                let res = match arg {
                    Value::Str(s) => Value::Number(s.borrow().chars().count() as i64),
                    Value::Set(s) => Value::Number(s.borrow().len() as i64),
                    _ => return Err(VMError::BadArgument),
                };
                self.push(res);
            }
            2 => {
                let res = {
                    let s_val = match &self.stack[self.sp] {
                        Value::Str(s) => s,
                        Value::Ref(idx) => match &self.frame[*idx] {
                            Value::Str(s) => s,
                            _ => return Err(VMError::BadArgument),
                        }
                        _ => return Err(VMError::BadArgument),
                    };
                    let res_str = s_val.borrow();

                    let pattern_val = self.deref(&self.stack[self.sp + 1]);
                    match pattern_val {
                        Value::Char(c) => res_str.starts_with(*c),
                        Value::Str(c) => res_str.starts_with(&*c.borrow()),
                        unk => res_str.starts_with(&unk.to_string()) 
                    }
                };
                self.push(Value::Bool(res));
            }
            3 => {
                let val = self.deref(&self.stack[self.sp]);
                
                let res = match val {
                    Value::File(f) => {
                        let mut file_ref = f.file.try_borrow_mut()
                            .map_err(|_| VMError::FuncErr)?;
                            
                        let mut buffer = vec![0u8; consts::READ_AT_ONCE];
                        
                        

                        file_ref.read(&mut buffer)
                            .map_err(|e| format!("Error while trying read the file ({}): {}", f, e)).map(|bb| String::from_utf8_lossy(&buffer[..bb])
                                    .into_owned())
                            .map(|x| Value::Str(Rc::new(RefCell::new(x))))
                    },
                    _ => return Err(VMError::BadArgument)
                };
                self.push(Value::new_control(res));
            }
            4 => {
                let format: String = Self::format(&self.stack[self.sp..self.sp + args_count])?;
                self.push(Value::Str(Rc::new(RefCell::new(format))));
            }
            5  => {
                let target = self.deref(&self.stack[self.sp]).clone().make_iter()?;
                self.push(Value::Iter(Box::new(crate::value::Iterator::Enumerate(Box::new(target), 0))));
            }
            6 => {
                let val = self.deref(&self.stack[self.sp]);
                let res = match val {
                    Value::File(f) => {
                        if let Err(e) = f.file.borrow_mut().seek(std::io::SeekFrom::Start(0)) {
                            Value::Result(Box::new(Err(Value::new_str(
                                format!("Error while trying seek the file({}): {}", f.path.display(), e)))))
                        } else {
                            let mut buffer = String::new();
                            let val = if let Err(e) = f.file.borrow_mut().read_to_string(&mut buffer) {
                                Err(format!("Error while trying read the file({}): {}", f.path.display(), e))
                            } else {
                                Ok(Value::new_str(buffer))
                            };
                            Value::new_control(val)
                        }
                    },
                    Value::Str(filename) => {
                        let res = fs::read_to_string(&*filename.borrow()).map(Value::new_str).map_err(|x| x.to_string());
                        Value::new_control(res)
                    }
                    _ => return Err(VMError::BadArgument)
                };
                self.push(res);
            }
            7 => {
                let filename = self.stack[self.sp].eval_str();
                let file = Value::open_file(&filename);
                self.push(file); 
            }
            8 => {
                let filename = self.stack[self.sp].eval_str();
                let opt = if args_count > 1 {
                    self.stack[self.sp + 1].expect_number()?
                } else {
                    consts::ALL_FLAGS
                };
                let res = Value::new_control(Value::new_file(&filename, opt).map_err(|_| format!("error with open: {}", filename)));
                self.push(res); 
            }
            9 => {
                let val = self.deref(&self.stack[self.sp]);
                let res = match val {
                    Value::Result(s) => Value::Bool(s.is_ok()),
                    _ => return Err(VMError::BadArgument),
                };
                self.push(res);

            }
            10 => {
                let val = self.deref(&self.stack[self.sp]);
                let res = match val {
                    Value::Str(s) => Value::Bool(s.borrow().is_empty()),
                    Value::Set(s) => Value::Bool(s.borrow().is_empty()),
                    _ => return Err(VMError::BadArgument),
                };
                self.push(res);

            }
            11 => {
                let val = self.deref(&self.stack[self.sp]);
                let res = match val {
                    Value::Cat(s) => Value::Bool(s.is_some()),
                    _ => return Err(VMError::BadArgument),
                };
                self.push(res);
            }
            12 => {
                let id = match &self.stack[self.sp] {
                    Value::Ref(idx) => *idx,
                    _ => return Ok(()),
                };

                match &mut self.frame[id] {
                    Value::Set(set) =>{
                        set.borrow_mut().push(self.stack[self.sp + 1].clone())
                    }
                    _ => return Err(VMError::BadArgument),
                }
                self.push(Value::Void);
            }
            13 => {
                if args_count > 0 {
                    let format = Self::format(&self.stack[self.sp..self.sp + args_count])?;
                    print!("{}", format);
                    let _ = std::io::stdout().flush();
                }
                let mut s = String::new();
                let _ = std::io::stdin().read_line(&mut s);
                self.push(Value::new_str(s.trim()));
            }
            14 => {
                let arg = self.deref(&self.stack[self.sp]).eval_str();
                let res = match arg.parse() {
                    Ok(num) => Value::Result(Box::new(Ok(Value::Number(num)))),
                    Err(e) => Value::Result(Box::new(Err(Value::new_str(e.to_string())))),
                };
                self.push(res);
            }
            15 => {
                let mut arg = if let Value::Iter(i) = &self.stack[self.sp]
                    && let Iterator::Range(r) = **i {
                        r
                } else {
                    return Err(VMError::BadArgument)
                };
                arg.step = self.stack[self.sp + 1].expect_number()?;
                self.push(Value::Iter(Box::new(Iterator::Range(arg))));
            }
            16 => {
                let arg = self.deref(&self.stack[self.sp]).eval_str();
                
                let res = Value::Iter(Box::new(crate::value::Iterator::Lines(crate::value::LinesIter {
                    source: arg.to_string(),
                    offset: 0,
                })));
                
                self.push(res);
            }
            17 => {
                let arg = self.deref(&self.stack[self.sp]).eval_str();
                let res = Value::Iter(Box::new(crate::value::Iterator::SplitWhitespace(crate::value::SplitWhitespaceIter {
                    source: arg.to_string(),
                    offset: 0,
                })));
                self.push(res);
            }
            18 => {
                let delimiter = self.stack[self.sp + 1].eval_str();
                let source = match &self.stack[self.sp] {
                    Value::Ref(i) => self.frame[*i].eval_str(),
                    Value::Str(arg) => arg.borrow().to_string(),
                    _ => return Err(VMError::BadArgument),
                };
                
                let res = Value::Iter(Box::new(crate::value::Iterator::Split(crate::value::SplitIter {
                    source,
                    delimiter,
                    offset: 0,
                })));
                self.push(res);
            }
            19 => {
                let n = self.stack[self.sp + 1].expect_number()?;
                if n < 0 {
                    return Err(VMError::BadArgument);
                }

                let mut result = None;
                {
                    let iter_ref = match &mut self.stack[self.sp] {
                        Value::Ref(i) => &mut self.frame[*i],
                        arg => arg,
                    };
                    
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
            20 => {
                let mut result_set = Vec::new();
                {
                    let iter_ref = match &mut self.stack[self.sp] {
                        Value::Ref(i) => &mut self.frame[*i],
                        arg => arg,
                    };
                    while let Some(val) = iter_ref.next()? {
                        result_set.push(val);
                    }
                }
                self.push(Value::Set(Rc::new(RefCell::new(result_set))));
            }
            21 => {
                let arg = self.deref(&self.stack[self.sp]).to_string();
                let pattern = self.stack[self.sp + 1].to_string();

                let ignore_case = args_count > 2 && self.stack[self.sp + 2].is_truthy();
                let exact_pattern = args_count > 3 && self.stack[self.sp + 3].is_truthy();
                
                let res = if ignore_case {
                    if exact_pattern {
                        arg.to_lowercase().contains(&pattern) 
                    } else {
                        arg.to_lowercase().contains(&pattern.to_lowercase()) 
                    }
                } else {
                    arg.contains(&pattern) 
                };
                self.push(Value::Bool(res));
            }
            22 => {
                let arg = self.deref(&self.stack[self.sp]).eval_str();
                let res = Value::new_str(arg.to_lowercase());
                self.push(res);
            }
            23 => {
                let arg = match &mut self.stack[self.sp] {
                    Value::Ref(i) => &mut self.frame[*i],
                    arg => arg,
                }.eval_str();
                let res = Value::new_str(arg.to_uppercase());
                self.push(res);
            }
            24 | 25 => {
                let new_line = func == 25;

                let res = Self::write(self.stack[self.sp].clone(), &self.stack[self.sp + 1..self.sp + args_count], new_line)
                    .map(|_| Value::Void).map_err(|_| "error with write".to_string());
                self.push(Value::new_control(res));
            }
            26 | 27 => {
                let new_line = func == 27;
                let res = 
                    Self::write(std::io::stdout(), &self.stack[self.sp..self.sp + args_count], new_line).map(|_| Value::Void).map_err(|_| "error with write into: stdout".to_string());
                self.push(Value::new_control(res));
            }
            28 => {
                let set = match &self.stack[self.sp] {
                    Value::Set(s) => s.clone(),
                    Value::Ref(idx) => match &self.frame[*idx] {
                        Value::Set(s) => s.clone(),
                        _ => return Err(VMError::BadArgument),
                    },
                    _ => return Err(VMError::BadArgument),
                };
                
                let (lambda_ip, stk) = match self.stack[self.sp + 1] {
                    Value::Fn(ip, stk) => (ip as usize, stk as usize),
                    _ => return Err(VMError::BadArgument),
                };

                let mut result_set = Vec::new();
                
                for item in set.borrow().iter() {
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
                self.push(Value::Set(Rc::new(RefCell::new(result_set))));

            }
            29 => {
                let set = match &self.stack[self.sp] {
                    Value::Set(s) => s.clone(),
                    Value::Ref(idx) => match &self.frame[*idx] {
                        Value::Set(s) => s.clone(),
                        _ => return Err(VMError::BadArgument),
                    },
                    _ => return Err(VMError::BadArgument),
                };
                
                let (lambda_ip, stk) = match self.stack[self.sp + 1] {
                    Value::Fn(ip, stk) => (ip as usize, stk as usize),
                    _ => return Err(VMError::BadArgument),
                };

                let mut result_set = Vec::new();
                
                for item in set.borrow().iter() {
                    self.run_lambda(code, lambda_ip, vec![item.clone()], stk)?;
                    
                    let result = self.pop();
                    result_set.push(result);
                }
                
                self.push(Value::Set(Rc::new(RefCell::new(result_set))));
            }
            30 => {
                print!("{esc}[2J{esc}[1;1H", esc = 27 as char);
                let _ = std::io::stdout().flush();
                self.push(Value::Void)
            },
            31 => {
                let set = match &self.stack[self.sp] {
                    Value::Set(s) => s.clone(),
                    Value::Ref(idx) => match &self.frame[*idx] {
                        Value::Set(s) => s.clone(),
                        _ => return Err(VMError::BadArgument),
                    },
                    _ => return Err(VMError::BadArgument),
                };
                
                let (lambda_ip , stk) = match self.stack[self.sp + 1] {
                    Value::Fn(ip, stk) => (ip as usize, stk as usize),
                    _ => return Err(VMError::BadArgument),
                };

                let mut result_set = Vec::new();
                
                for item in set.borrow().iter() {
                    self.run_lambda(code, lambda_ip, vec![item.clone()], stk)?;
                    
                    let cond = self.pop();
                    if cond.is_truthy() {
                        result_set.push(item.clone());
                    }
                }
                
                self.push(Value::Set(Rc::new(RefCell::new(result_set))));
            }
            _ => unreachable!()
        }
        Ok(())
    } 

    #[inline(always)]
    pub fn deref(&'a self, arg: &'a Value) -> &'a Value {
        match arg {
            Value::Ref(i) => unsafe {&self.frame.get_unchecked(*i)},
            _ => arg
        }
    }
   
    pub fn run_lambda(&mut self, code: &[Op<'a>], target_ip: usize, args: Vec<Value>, env_frame: usize) -> Result<(), VMError> {
        let (display, depth) = if env_frame < self.call_stack.len() {
            let parent = &self.call_stack[env_frame];
            let mut d = parent.display;
            let current_depth = parent.depth;
            d[current_depth] = parent.frame_idx;
            (d, current_depth + 1)
        } else {
            ([0; crate::vm::MAX_DEPTH], 0)
        };

        self.call_stack.push(CallFrame {
            return_ip: consts::STOP_FLAG, 
            old_frame: self.now_frame,
            display,
            depth,
            frame_idx: self.frame.len()
        });

        self.now_frame = self.frame.len();
        self.frame.extend(args);

        self.run(code, target_ip)?;
        Ok(())
    }
}
