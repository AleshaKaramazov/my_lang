use std::cell::RefCell;
use std::rc::Rc;
use crate::op::Op;
use crate::consts;
use crate::value::Value;

pub struct VM {
    pub stack: Vec<Value>,
    pub sp: usize,
    pub frame: Vec<Value>,
    pub fp: usize,
    pub call_stack: Vec<CallFrame>,
    pub now_frame: usize,
}

pub struct CallFrame {
    pub return_ip: usize, 
    pub old_frame: usize, 
    pub parent_frame: usize, 
    pub frame_idx: usize,   
}

const STACK_MAX: usize = 2048;

impl<'a> VM {
    pub fn new() -> Self {
        Self {
            stack: vec![Value::Void; STACK_MAX],
            sp: 0,
            frame: vec![Value::Void; 1024],
            fp: 0,
            call_stack: Vec::with_capacity(1024),
            now_frame: 0,
        }
    }

    #[inline(always)]
    pub fn push(&mut self, val: Value) {
        unsafe {
            *self.stack.get_unchecked_mut(self.sp) = val;
        }
        self.sp += 1;
    }

    #[inline(always)]
    pub fn pop(&mut self) -> Value {
        self.sp -= 1;
        unsafe {
            let ptr = self.stack.get_unchecked_mut(self.sp);
            std::mem::replace(ptr, Value::Void)
        }
    }

    #[inline(always)]
    fn get_frame_base(&self, depth_delta: usize) -> usize {
        if depth_delta == 0 {
            return self.now_frame;
        }
        let mut current_frame_idx = self.call_stack.last().unwrap().parent_frame;
        for _ in 1..depth_delta {
            if current_frame_idx == consts::STOP_FLAG {
                return 0; 
            }
            current_frame_idx = self.call_stack[current_frame_idx].parent_frame;
        }
        
        if current_frame_idx == consts::STOP_FLAG {
            0
        } else {
            self.call_stack[current_frame_idx].frame_idx
        }
    }

    #[inline(always)]
    pub fn run(&mut self, code: &[Op<'a>], start_ip: usize) {
        let base_ptr = code.as_ptr();
        let end_ptr = unsafe { base_ptr.add(code.len()) }; 
        
        let mut ip_ptr = unsafe { base_ptr.add(start_ip) };
        loop {
            if ip_ptr >= end_ptr {
                break;
            }
            let op = unsafe { &*ip_ptr };

            match op {
                Op::PushFLoat(f) => self.push(Value::Float(*f)),
                Op::PushStr(s) => self.push(Value::Str(Rc::new(RefCell::new(s.to_string())))),
                Op::PushChar(c) => self.push(Value::Char(*c)),
                Op::PushNumber(n) => self.push(Value::Number(*n)),
                Op::PushBool(b) => self.push(Value::Bool(*b)),
                Op::PushRefGlobal(idx) => self.push(Value::Ref(*idx)),
                Op::PushFn(id) => {
                    let env_idx = self.call_stack.len().saturating_sub(1);
                    self.push(Value::Fn(*id as u32, env_idx as u32));
                }
                Op::PushVoid => self.push(Value::Void),
                Op::Pop => {
                    self.pop();
                }
                Op::Dup => {
                    let val = unsafe { self.stack.get_unchecked(self.sp - 1).clone()};
                    self.push(val);
                }
                Op::Try => {
                    let val = self.pop();
                    match val {
                        Value::Result(inner) => match &*inner {
                            Ok(inner_val) => self.push(inner_val.clone()),
                            Err(err_val) => {
                                let return_val = Value::Result(Box::new(Err(err_val.clone())));
                                let frame = self.call_stack.pop().unwrap();
                                
                                self.fp = self.now_frame;
                                self.now_frame = frame.old_frame;
                                
                                if frame.return_ip == consts::STOP_FLAG {
                                    self.push(return_val);
                                    break;
                                }

                                ip_ptr = unsafe { base_ptr.add(frame.return_ip) };
                                self.push(return_val);
                                continue;
                            }
                        },
                        Value::Cat(inner) => match &inner {
                            Some(inner_val) => {
                                self.push(*inner_val.clone());
                            }
                            None => {
                                let return_val = Value::Cat(None);
                                let frame = self.call_stack.pop().unwrap();
                                
                                self.fp = self.now_frame;
                                self.now_frame = frame.old_frame;
                                
                                if frame.return_ip == consts::STOP_FLAG {
                                    self.push(return_val);
                                    break;
                                }

                                ip_ptr = unsafe { base_ptr.add(frame.return_ip) };
                                self.push(return_val);
                                continue;
                            }
                        },
                        _ => unreachable!()
                    }
                }
                Op::Plus | Op::Mod | Op::Sub | Op::Mult | Op::Div | Op::Pow | Op::ArifmAnd | Op::ArifmOr => {
                    let right = self.pop();
                    let left = unsafe { self.stack.get_unchecked_mut(self.sp - 1) };
                    
                    match *op {
                        Op::Plus => left.add_assign(right),
                        Op::Sub => left.sub_assign(right),
                        Op::Mult => left.mul_assign(right),
                        Op::Div => left.div_assign(right),
                        Op::Pow => left.pow_assign(right),
                        Op::ArifmAnd => left.arifm_and_assign(right),
                        Op::ArifmOr => left.arifm_or_assign(right),
                        Op::Mod => left.arifm_mod_assign(right),
                        _ => unreachable!(),
                    }
                }
                Op::Equal | Op::NotEqual | Op::Greater | Op::Less | Op::GreaterEq | Op::LessEq => {
                    let right = self.pop();
                    let left = unsafe { self.stack.get_unchecked_mut(self.sp - 1)};
                    
                    let result = match *op {
                        Op::Equal => *left == right,
                        Op::Greater => *left > right,
                        Op::NotEqual => *left != right,
                        Op::Less => *left < right,
                        Op::GreaterEq => *left >= right,
                        Op::LessEq => *left <= right,
                        _ => unreachable!(),
                    };
                    *left = Value::Bool(result);
                }
                Op::UnpackTuple => {
                    let val = self.pop();
                    if let Value::Tuple(vals) = val {
                        let vals = vals.borrow();
                        for v in vals.iter() {
                            self.push(v.clone());
                        }
                    }  
                }
                Op::LoadGlobal(idx) => {
                    self.push(unsafe {self.frame.get_unchecked(*idx)}.clone());
                }
                Op::MakeOk => {
                    let tos = unsafe { self.stack.get_unchecked_mut(self.sp - 1) };
                    let val = std::mem::replace(tos, Value::Void);
                    *tos = Value::Result(Box::new(Ok(val)));
                }
                Op::MakeErr => {
                    let tos = unsafe { self.stack.get_unchecked_mut(self.sp - 1) };
                    let val = std::mem::replace(tos, Value::Void);
                    *tos = Value::Result(Box::new(Err(val)));
                }
                Op::MakeSome => {
                    let tos = unsafe { self.stack.get_unchecked_mut(self.sp - 1) };
                    let val = std::mem::replace(tos, Value::Void);
                    *tos = Value::Cat(Some(Box::new(val)));
                }
                Op::Not => {
                    let tos = unsafe { self.stack.get_unchecked_mut(self.sp - 1) };
                    *tos = Value::Bool(!tos.is_truthy());
                }
                Op::None => {
                    self.push(Value::Cat(None));
                }
                Op::MakeRange(incl) => {
                    let end = self.pop();
                    let start = self.pop();
                    self.push(Value::make_range(start, end, *incl));
                }
                Op::MakeIter => {
                    let val = self.pop();
                    self.push(
                        if matches!(val, Value::Iter(_)) {val} else {Value::Iter(Box::new(val.make_iter()))}
                    );
                }
                Op::Jump(target) => {
                    ip_ptr = unsafe { base_ptr.add(*target) };
                    continue;
                }
                Op::JumpIfFalse(target) => {
                    let val = self.pop();
                    if !val.is_truthy() {
                        ip_ptr = unsafe { base_ptr.add(*target) };
                        continue;
                    }
                }
                Op::JumpIfTrue(target) => {
                    let val = self.pop();
                    if val.is_truthy() {
                        ip_ptr = unsafe { base_ptr.add(*target) };
                        continue;
                    }
                }
                Op::SafeUnwL(target) => {
                    let val = self.pop();
                    match val {
                        Value::Result(inner) => if let Err(inner_err) = &*inner {
                            self.push(inner_err.clone());
                        } else {
                            ip_ptr = unsafe { base_ptr.add(*target) };
                            continue;
                        }
                        _ => {
                            ip_ptr = unsafe { base_ptr.add(*target) };
                            continue;
                        }
                    }
                }
                Op::SafeUnwR(target) => {
                    let val = self.pop();
                    match val {
                        Value::Result(inner) => if let Ok(inner_val) = &*inner {
                            self.push(inner_val.clone());
                        } else {
                            ip_ptr = unsafe { base_ptr.add(*target) };
                            continue;
                        }
                        Value::Cat(Some(inner_val)) => {
                            self.push(*inner_val.clone());
                        }
                        _ => {
                            ip_ptr = unsafe { base_ptr.add(*target) };
                            continue;
                        }
                    }
                }
                Op::IterNext(target) => {
                    let tos = unsafe { self.stack.get_unchecked_mut(self.sp - 1) };
                    let val = tos.next();
                    match val {
                        Some(val) => self.push(val),
                        None => {
                            ip_ptr = unsafe { base_ptr.add(*target) };
                            continue;
                        }
                    }
                }
               Op::MakeTuple(count) | Op::MakeSet(count) => {
                    let count = *count;
                    let is_tuple = matches!(op, Op::MakeTuple(_));
                    
                    if count == 0 {
                        let val = if is_tuple {
                            Value::Tuple(Rc::new(RefCell::new(Vec::new())))
                        } else {
                            Value::Set(Rc::new(RefCell::new(Vec::new())))
                        };
                        self.push(val);
                    } else {
                        let start = self.sp - count;
                        let mut vals = Vec::with_capacity(count);
                        
                        for i in 0..count {
                            vals.push(std::mem::replace(&mut self.stack[start + i], Value::Void));
                        }
                        
                        let container = if is_tuple {
                            Value::Tuple(Rc::new(RefCell::new(vals)))
                        } else {
                            Value::Set(Rc::new(RefCell::new(vals)))
                        };
                        
                        self.stack[start] = container;
                        self.sp = start + 1;
                    }
                }
                Op::DupTarget(deep) => {
                    let count = *deep + 1;
                    let start = self.sp - count;
                    
                    for i in 0..count { 
                        let val = unsafe { self.stack.get_unchecked(start + i) }.clone();
                        self.push(val);
                    }
                }
                Op::StoreIndex(count) => {
                    let count = *count;
                    let to_set = self.pop();
                      
                    if count > 1 {
                        let start = self.sp - count;
                        
                        let mut target = std::mem::replace(&mut self.stack[start - 1], Value::Void);
                        target.set_index_deep(&self.stack[start..start + count], to_set);
                        
                        for i in 0..count {
                            self.stack[start + i] = Value::Void;
                        }
                        
                        self.stack[start - 1] = target;
                        self.sp = start; 
                    } else {
                        let index = self.pop();
                        let mut target = self.pop();
                        target.set_index(index, to_set);
                        self.push(target);
                    }
                }             

                Op::LoadIndex(count) => {
                    let count = *count;
                    let res = if count > 1 {
                        let start = self.sp - count;
                        
                        let target = std::mem::replace(&mut self.stack[start - 1], Value::Void);
                        let value = target.load_index_deep(&self.stack[start..start + count]);
                        
                        for i in 0..count {
                            self.stack[start + i] = Value::Void;
                        }
                        
                        self.sp = start - 1; 
                        value
                    } else {
                        let index = self.pop();
                        let value = self.pop();
                        value.load_index(&index)
                    };
                    self.push(res); 
                }
                Op::StoreGlobal(idx) => {
                    let value = self.pop();
                    if *idx >= self.fp {
                        self.fp = idx + 1;
                    }
                    unsafe {*self.frame.get_unchecked_mut(*idx) = value}
                }
                Op::LoadLocal(idx, depth_delta) => {
                    let base = self.get_frame_base(*depth_delta);
                    let index = base + idx;
                    self.push(unsafe { self.frame.get_unchecked(index) }.clone());
                }

                Op::StoreLocal(idx, depth_delta) => {
                    let value = self.pop();
                    let base = self.get_frame_base(*depth_delta);
                    let index = base + idx;

                    if index >= self.fp {
                        self.fp = index + 1;
                    }
                    unsafe { *self.frame.get_unchecked_mut(index) = value; }
                }

                Op::PushRefLocal(idx, depth_delta) => {
                    let base = self.get_frame_base(*depth_delta);
                    self.push(Value::Ref(base + idx));
                }
                Op::CallFunc(n) => {
                    let func_val = self.pop();
                    
                    match func_val {
                        Value::Number(func) => {
                            self.sp -= *n; 
                            self.run_func(func, *n, code); 
                            if *n > 1 {
                                for i in 0..(*n - 1) {
                                    unsafe { *self.stack.get_unchecked_mut(self.sp + i) = Value::Void; }
                                }
                            }
                        }
                        Value::Fn(target_ip, env_frame) => { 
                            let next_frame_idx = self.fp;

                            if *n > 0 {
                                let start = self.sp - *n;

                                for i in start..self.sp {
                                    let val = std::mem::replace(unsafe { self.stack.get_unchecked_mut(i) }, Value::Void);
                                    
                                    if self.fp >= self.frame.len() { self.frame.resize(self.fp + 1024, Value::Void); }
                                    unsafe { *self.frame.get_unchecked_mut(self.fp) = val; }
                                    self.fp += 1;
                                }

                                self.sp -= *n;
                            }

                            let current_idx = unsafe { ip_ptr.offset_from(base_ptr) as usize };

                            let parent_frame = if (env_frame as usize) < self.call_stack.len() {
                                env_frame as usize
                            } else {
                                usize::MAX
                            };

                            self.call_stack.push(CallFrame {
                                return_ip: current_idx + 1, 
                                old_frame: self.now_frame,
                                parent_frame, 
                                frame_idx: next_frame_idx,
                            });

                            self.now_frame = next_frame_idx;
                            
                            ip_ptr = unsafe { base_ptr.add(target_ip as usize) };
                            continue;
                        }     
                        _ => {}
                    }
                }
                Op::Return => {
                    let return_val = self.pop();
                    let frame = self.call_stack.pop().unwrap();
                    
                    self.fp = self.now_frame;
                    self.now_frame = frame.old_frame;
                    
                    if frame.return_ip == consts::STOP_FLAG {
                        self.push(return_val);
                        break; 
                    }
                    
                    ip_ptr = unsafe { base_ptr.add(frame.return_ip) };
                    self.push(return_val);
                    continue;
                }
            }
            ip_ptr = unsafe { ip_ptr.add(1) };
        }
    }

}
