use crate::{op::Op, value::Value};

pub struct VM {
    stack: Vec<Value>,
    frame: Vec<Value>,
    call_stack: Vec<CallFrame>,
    now_frame: usize,
}

struct CallFrame {
    return_ip: usize, 
    old_frame: usize, 
}

impl<'a> VM {
    pub fn new() -> Self {
        Self {
            stack: Vec::with_capacity(32),
            frame: Vec::with_capacity(32),
            call_stack: Vec::with_capacity(32),
            now_frame: 0,
        }
    }

    #[inline(always)]
    pub fn step(&mut self, op: &Op<'a>, ip: &mut usize) -> Result<(), String> {
        match *op {
            Op::PushStr(s) => self.stack.push(Value::Str(s.to_string())),
            Op::PushChar(c) => self.stack.push(Value::Char(c)),
            Op::PushNumber(n) => self.stack.push(Value::Number(n)),
            Op::PushBool(b) => self.stack.push(Value::Bool(b)),
            Op::PushRef(r) => self.stack.push(Value::Ref(r)),
            Op::PushFn(id) => self.stack.push(Value::Fn(id)),
            Op::PushVoid => self.stack.push(Value::Void),
            Op::Pop => {
                self.stack.pop().ok_or_else(|| "VM Error: Pop from empty stack".to_string())?;
            }
            Op::Plus | Op::Sub | Op::Mult | Op::Div | Op::Pow | Op::ArifmAnd | Op::ArifmOr => {
                let right = self.stack.pop().ok_or("VM Error: Stack underflow")?;
                let left = self.stack.pop().ok_or("VM Error: Stack underflow")?;
                let result = match *op {
                    Op::Plus => (left + right)?,
                    Op::Sub => (left - right)?,
                    Op::Mult => (left * right)?,
                    Op::Div => (left / right)?,
                    Op::Pow => left.pow(right)?,
                    Op::ArifmAnd => left.arifm_and(right)?,
                    Op::ArifmOr => left.arifm_or(right)?,
                    _ => unreachable!(),
                };
                self.stack.push(result);
            }
            Op::Equal | Op::Greater | Op::Less | Op::GreaterEq | Op::LessEq => {
                let right = self.stack.pop().ok_or("VM Error: Stack underflow")?;
                let left = self.stack.pop().ok_or("VM Error: Stack underflow")?;
                
                let result = match *op {
                    Op::Equal => left == right,
                    Op::Greater => left > right,
                    Op::Less => left < right,
                    Op::GreaterEq => left >= right,
                    Op::LessEq => left <= right,
                    _ => unreachable!(),
                };
                self.stack.push(Value::Bool(result));
            }
            Op::MakeRange(incl) => {
                let end = self.stack.pop().ok_or("VM Error: Stack underflow on MakeRange")?;
                let start = self.stack.pop().ok_or("VM Error: Stack underflow on MakeRange")?;
                self.stack.push(Value::make_range(start, end, incl)?);
            }
            Op::Swap => {
                let len = self.stack.len();
                if len < 2 {
                    return Err("VM Error: Stack underflow (Swap)".to_string());
                }
                self.stack.swap(len - 1, len - 2);
            }
            Op::Dup => {
                let val = self.stack.last().ok_or("VM Error: Stack underflow on Dup")?.clone();
                self.stack.push(val);
            }
            Op::MakeIter => {
                let val = self.stack.pop().ok_or("VM Error: Stack underflow on MakeIter")?;
                self.stack.push(val.make_iter()?);
            }
            Op::IterNext(i) => {
                let val = self.stack.last_mut().unwrap().next();
                match val {
                    Some(val) => self.stack.push(val),
                    None => {
                        *ip = i;
                        return Ok(()); 
                    }
                }
            }
            Op::Not => {
                let val = self.stack.pop().ok_or("VM Error: Stack underflow")?;
                self.stack.push(Value::Bool(!val.is_truthy()));
            }
            Op::Jump(target) => {
                *ip = target;
                return Ok(()); 
            }
            Op::JumpIfFalse(target) => {
                let val = self.stack.pop().ok_or("VM Error: Stack underflow")?;
                if !val.is_truthy() {
                    *ip = target;
                    return Ok(());
                }
            }
            Op::MakeSet(i) => {
                let start_idx = self.stack.len() - i;
                let vals: Vec<Value> = self.stack.drain(start_idx..).collect();
                self.stack.push(Value::Set(vals)); 
            }
            Op::JumpIfTrue(target) => {
                let val = self.stack.pop().ok_or("VM Error: Stack underflow")?;
                if val.is_truthy() {
                    *ip = target;
                    return Ok(());
                }
            }
            Op::DupTarget(deep) => {
                let len = self.stack.len();
                let start = len - (1 + deep); 
                let mut to_dup = vec![];
                for i in start..len {
                    to_dup.push(self.stack[i].clone());
                }
                self.stack.extend(to_dup);
            }
            Op::StoreIndex(count) => {
                let to_set = self.stack.pop().ok_or_else(|| "VM Error: No value for StoreIndex".to_string())?;
                
                if count > 1 {
                    let index_start = self.stack.len() - count;
                    let indexes: Vec<Value> = self.stack.drain(index_start..).collect();
                    let mut target = self.stack.pop().ok_or_else(|| "VM Error: No target for StoreIndex".to_string())?;
                    
                    target.set_index_deep(indexes, to_set)?;
                    self.stack.push(target); 
                } else {
                    let index = self.stack.pop().ok_or_else(|| "VM Error: No index for StoreIndex".to_string())?;
                    let mut target = self.stack.pop().ok_or_else(|| "VM Error: No target for StoreIndex".to_string())?;
                    
                    target.set_index(index, to_set)?;
                    self.stack.push(target); 
                }
            }
            Op::LoadIndex(count) => {
                let res = if count > 1 {
                    let index = self.stack.len() - count;
                    let indexes: Vec<Value> = self.stack.drain(index..).collect();
                    let value = self.stack.pop().ok_or_else(|| "VM Error: No value for LoadIndex".to_string())?;
                    value.load_index_deep(indexes)?
                } else {
                    let index = self.stack.pop().ok_or_else(|| "VM Error: No value for LoadIndex".to_string())?;
                    let value = self.stack.pop().ok_or_else(|| "VM Error: No value for LoadIndex".to_string())?;
                    value.load_index(index)?
                };
                self.stack.push(res); 
            }
            Op::StoreLocal(idx) => {
                let value = self.stack.pop().ok_or_else(|| "VM Error: No value for StoreLocal".to_string())?;
                let index = self.now_frame + idx;

                if index >= self.frame.len() {
                    self.frame.resize(index + 1, Value::Void);
                }
                self.frame[index] = value;
            }
            Op::StoreGlobal(idx) => {
                let value = self.stack.pop().ok_or_else(|| "VM Error: No value for StoreLocal".to_string())?;
                if idx >= self.frame.len() {
                    self.frame.resize(idx + 1, Value::Void);
                }
                self.frame[idx] = value;
            }
            Op::LoadLocal(idx) => {
                let index = self.now_frame + idx;
                if index >= self.frame.len() {
                    return Err(format!("VM Error: Uninitialized frame slot {}", idx));
                }
                self.stack.push(self.frame[index].clone());
            }
            Op::LoadGlobal(idx) => {
                if idx >= self.frame.len() {
                    return Err(format!("VM Error: Uninitialized global slot {}", idx));
                }
                self.stack.push(self.frame[idx].clone());
            }
            Op::CallFunc(n) => {
                let func_val = self.stack.pop().ok_or_else(|| "VM Error: Missing function identifier".to_string())?;
                
                match func_val {
                    Value::Str(func_name) => {
                        let mut args = Vec::with_capacity(n);
                        for _ in 0..n {
                            args.push(self.stack.pop().ok_or_else(|| "VM Error: Missing argument for CallFunc".to_string())?);
                        }
                        args.reverse();

                        match func_name.as_str() {
                            "len" => {
                                let res = match &args[0] {
                                    Value::Str(s) => Value::Number(s.chars().count() as i64),
                                    Value::Ref(idx) => match &self.frame[*idx] {
                                        Value::Str(s) => Value::Number(s.chars().count() as i64),
                                        _ => return Err("can't get len".to_string()),
                                    }
                                    _ => return Err("can't get len".to_string()),
                                };
                                self.stack.push(res);
                            }
                            "step" => {
                                let mut arg = if let Value::Range(i) = &args[0] {
                                    i.clone()
                                } else {
                                    return Err("Step only for ranges".to_string());
                                };
                                arg.step = args[1].expect_number()?;
                                self.stack.push(Value::Range(arg));
                            }
                            "writeln" => {
                                print!("WRITEFUNC: ");
                                for (i, arg) in args.iter().enumerate() {
                                    print!("{}", arg);
                                    if i < args.len() - 1 {
                                        print!(" ");
                                    }
                                }
                                println!();
                                self.stack.push(Value::Void);
                            }
                            _ => return Err(format!("VM Error: Unknown function '{}'", func_name)),
                        }
                    }
                    Value::Fn(target_ip) => {
                        let mut args = Vec::with_capacity(n);
                        for _ in 0..n {
                            args.push(self.stack.pop().ok_or_else(|| "VM Error: Missing argument for user function".to_string())?);
                        }
                        args.reverse();

                        self.call_stack.push(CallFrame {
                            return_ip: *ip + 1, 
                            old_frame: self.now_frame,
                        });

                        self.now_frame = self.frame.len();

                        self.frame.extend(args);

                        *ip = target_ip;
                        return Ok(()); 
                    }
                    _ => return Err("VM Error: Attempted to call a non-callable value".to_string()),
                }
            }
            Op::Return => {
                let return_val = self.stack.pop().ok_or_else(|| "VM Error: No return value on stack".to_string())?;
                
                let frame = self.call_stack.pop().ok_or_else(|| "VM Error: Call stack underflow on Return".to_string())?;
                
                self.frame.truncate(self.now_frame);
                
                self.now_frame = frame.old_frame;
                *ip = frame.return_ip;
                
                self.stack.push(return_val);
                return Ok(()); 
            }
        }
        *ip += 1;
        Ok(())
    }

    pub fn run(&mut self, code: &[Op<'a>]) -> Result<(), String> {
        let mut ip = 0;
        while ip < code.len() {
            self.step(&code[ip], &mut ip)?;
        }
        Ok(())
    }
}
