use std::cell::RefCell;
use std::rc::Rc;
use crate::op::Op;
use crate::consts;
use crate::errors::VMError;
use crate::value::Value;

pub struct VM {
    pub stack: Vec<Value>,
    pub tos: Value,
    pub sp: usize,
    pub frame: Vec<Value>,
    pub call_stack: Vec<CallFrame>,
    pub now_frame: usize,
}

pub const MAX_DEPTH: usize = 16;

pub struct CallFrame {
    pub return_ip: usize, 
    pub old_frame: usize, 
    pub display: [usize; MAX_DEPTH], 
    pub depth: usize,
    pub frame_idx: usize,   
}

const STACK_MAX: usize = 2048;

impl<'a> VM {
    pub fn new() -> Self {
        Self {
            stack: vec![Value::Void; STACK_MAX],
            tos: Value::Void,
            sp: 0,
            frame: Vec::with_capacity(32),
            call_stack: Vec::with_capacity(32),
            now_frame: 0,
        }
    }

    #[inline(always)]
    pub fn push(&mut self, val: Value) {
        if self.sp > 0 {
            unsafe {
                *self.stack.get_unchecked_mut(self.sp - 1) = std::mem::replace(&mut self.tos, val);
            }
        } else {
            self.tos = val;
        }
        self.sp += 1;
    }

    #[inline(always)]
    pub fn pop(&mut self) -> Value {
        if self.sp == 0 {
            return Value::Void; 
        }
        self.sp -= 1;
        let popped = std::mem::replace(&mut self.tos, Value::Void);
        if self.sp > 0 {
            unsafe {
                self.tos = std::mem::replace(self.stack.get_unchecked_mut(self.sp - 1), Value::Void);
            }
        }
        popped
    }

    #[inline(always)]
    fn get_frame_base(&self, depth_delta: usize) -> usize {
        if depth_delta == 0 {
            return self.now_frame;
        }
        let call_frame = self.call_stack.last().unwrap();
        call_frame.display[call_frame.depth - depth_delta] 
    }

    #[inline(always)]
    pub fn run(&mut self, code: &[Op<'a>], start_ip: usize) -> Result<(), VMError> {
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
                    let val = self.tos.clone();
                    self.push(val);
                }

                Op::ExpectType(tp) => {
                    if !self.tos.this_type(tp) {
                        println!("Expected: {:?}, find: {}", tp, self.tos);
                        return Err(VMError::UnExpectedType);
                    }
                }
                Op::Try => {
                    let val = self.pop();
                    match val {
                        Value::Result(inner) => match *inner {
                            Ok(inner_val) => self.push(inner_val),
                            Err(err_val) => {
                                let return_val = Value::Result(Box::new(Err(err_val)));
                                let frame = self.call_stack.pop().ok_or(VMError::EmptyStack)?;
                                
                                self.frame.truncate(self.now_frame);
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
                        Value::Cat(inner) => match inner {
                            Some(inner_val) => {
                                self.push(*inner_val);
                            }
                            None => {
                                let return_val = Value::Cat(None);
                                let frame = self.call_stack.pop().ok_or(VMError::EmptyStack)?;
                                
                                self.frame.truncate(self.now_frame);
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
                        _ => return Err(VMError::UnExpectedType),
                    }
                }
                Op::Plus | Op::Mod | Op::Sub | Op::Mult | Op::Div | Op::Pow | Op::ArifmAnd | Op::ArifmOr => {
                    let right = std::mem::replace(&mut self.tos, Value::Void);
                    self.sp -= 1; 
                    
                    let left = unsafe { self.stack.get_unchecked_mut(self.sp - 1) };
                    
                    match *op {
                        Op::Plus => left.add_assign(right)?,
                        Op::Sub => left.sub_assign(right)?,
                        Op::Mult => left.mul_assign(right)?,
                        Op::Div => left.div_assign(right)?,
                        Op::Pow => left.pow_assign(right)?,
                        Op::ArifmAnd => left.arifm_and_assign(right)?,
                        Op::ArifmOr => left.arifm_or_assign(right)?,
                        Op::Mod => left.arifm_mod_assign(right)?,
                        _ => unreachable!(),
                    }
                    
                    self.tos = std::mem::replace(left, Value::Void);
                }
                Op::Equal | Op::NotEqual | Op::Greater | Op::Less | Op::GreaterEq | Op::LessEq => {
                    let right = std::mem::replace(&mut self.tos, Value::Void);
                    self.sp -= 1;
                    let left = std::mem::replace(unsafe { self.stack.get_unchecked_mut(self.sp - 1) }, Value::Void);
                    
                    let result = match *op {
                        Op::Equal => left == right,
                        Op::Greater => left > right,
                        Op::NotEqual => left != right,
                        Op::Less => left < right,
                        Op::GreaterEq => left >= right,
                        Op::LessEq => left <= right,
                        _ => unreachable!(),
                    };
                    self.tos = Value::Bool(result);
                }
                Op::MakeTuple(count) => {
                    let count = *count;
                    if count == 0 {
                        self.push(Value::Tuple(Rc::new(RefCell::new(Vec::new()))));
                    } else {
                        let start = self.sp - count;
                        let mut vals = Vec::with_capacity(count);
                        
                        for i in start..(self.sp - 1) {
                            vals.push(std::mem::replace(&mut self.stack[i], Value::Void));
                        }
                        vals.push(std::mem::replace(&mut self.tos, Value::Void));
                        
                        self.tos = Value::Tuple(Rc::new(RefCell::new(vals)));
                        self.sp = start + 1;
                    }
                }
                Op::UnpackTuple(count) => {
                    let val = self.pop();
                    if let Value::Tuple(vals) = val {
                        let vals = vals.borrow();
                        if vals.len() != *count {
                            return Err(VMError::EmptyStack);
                        }
                        for v in vals.iter() {
                            self.push(v.clone());
                        }
                    } else {
                        return Err(VMError::EmptyStack);
                    }
                }
                Op::LoadGlobal(idx) => {
                    if *idx >= self.frame.len() {
                        return Err(VMError::EmptyStack);
                    }
                    self.push(unsafe {self.frame.get_unchecked(*idx)}.clone());
                }
                Op::MakeOk => {
                    let val = std::mem::replace(&mut self.tos, Value::Void);
                    self.tos = Value::Result(Box::new(Ok(val)));
                }
                Op::MakeErr => {
                    let val = std::mem::replace(&mut self.tos, Value::Void);
                    self.tos = Value::Result(Box::new(Err(val)));
                }
                Op::MakeSome => {
                    let val = std::mem::replace(&mut self.tos, Value::Void);
                    self.tos = Value::Cat(Some(Box::new(val)));
                }
                Op::None => {
                    self.push(Value::Cat(None));
                }
                Op::MakeRange(incl) => {
                    let end = self.pop();
                    let start = self.pop();
                    self.push(Value::make_range(start, end, *incl)?);
                }
                Op::MakeIter => {
                    let val = self.pop();
                    self.push(
                        if matches!(val, Value::Iter(_)) {val} else {Value::Iter(Box::new(val.make_iter()?))}
                    );
                }
                Op::Not => {
                    self.tos = Value::Bool(!self.tos.is_truthy());
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
                    let val = std::mem::replace(&mut self.tos, Value::Void);
                    match val {
                        Value::Result(inner) => if let Err(inner) = *inner {
                            self.tos = inner
                        } 
                        else {
                            self.sp -= 1;
                            if self.sp > 0 {
                                self.tos = std::mem::replace(unsafe { self.stack.get_unchecked_mut(self.sp - 1) }, Value::Void);
                            }
                            ip_ptr = unsafe { base_ptr.add(*target) };
                            continue;
                        }
                        _ => {
                            self.sp -= 1;
                            if self.sp > 0 {
                                self.tos = std::mem::replace(unsafe { self.stack.get_unchecked_mut(self.sp - 1) }, Value::Void);
                            }
                            ip_ptr = unsafe { base_ptr.add(*target) };
                            continue;
                        }
                    }
                }
                Op::SafeUnwR(target) => {
                    let val = std::mem::replace(&mut self.tos, Value::Void);
                    match val {
                        Value::Result(inner) => if let Ok(inner_val) = *inner {
                            self.tos = inner_val; 
                        } else {
                            self.sp -= 1;
                            if self.sp > 0 {
                                self.tos = std::mem::replace(unsafe { self.stack.get_unchecked_mut(self.sp - 1) }, Value::Void);
                            }
                            ip_ptr = unsafe { base_ptr.add(*target) };
                            continue;
                        }
                        Value::Cat(Some(inner_val)) => {
                            self.tos = *inner_val; 
                        }
                        _ => {
                            self.sp -= 1;
                            if self.sp > 0 {
                                self.tos = std::mem::replace(unsafe { self.stack.get_unchecked_mut(self.sp - 1) }, Value::Void);
                            }
                            ip_ptr = unsafe { base_ptr.add(*target) };
                            continue;
                        }
                    }
                }
                Op::IterNext(target) => {
                    let val = self.tos.next()?;
                    match val {
                        Some(val) => self.push(val),
                        None => {
                            ip_ptr = unsafe { base_ptr.add(*target) };
                            continue;
                        }
                    }
                }
                Op::MakeSet(count) => {
                    let count = *count;
                    if count == 0 {
                        self.push(Value::Set(Rc::new(RefCell::new(Vec::new()))));
                    } else {
                        let start = self.sp - count;
                        let mut vals = Vec::with_capacity(count);
                        
                        for i in start..(self.sp - 1) {
                            vals.push(std::mem::replace(&mut self.stack[i], Value::Void));
                        }
                        vals.push(std::mem::replace(&mut self.tos, Value::Void));
                        
                        self.tos = Value::Set(Rc::new(RefCell::new(vals)));
                        self.sp = start + 1;
                    }
                }
                Op::DupTarget(deep) => {
                    let deep = *deep;
                    let count = deep + 1;
                    let start = self.sp - count;
                    let mut to_dup = Vec::with_capacity(count);
                    
                    for i in start..(self.sp - 1) {
                        to_dup.push(self.stack[i].clone());
                    }
                    to_dup.push(self.tos.clone());
                    
                    for val in to_dup {
                        self.push(val);
                    }
                }
                Op::StoreIndex(count) => {
                    let count = *count;
                    let to_set = self.pop();
                    
                    if count > 1 {
                        let mut indexes = Vec::with_capacity(count);
                        let start = self.sp - count;
                        
                        for i in start..(self.sp - 1) {
                            indexes.push(std::mem::replace(&mut self.stack[i], Value::Void));
                        }
                        indexes.push(std::mem::replace(&mut self.tos, Value::Void));
                        
                        self.sp = start;
                        if self.sp > 0 {
                            self.tos = std::mem::replace(&mut self.stack[self.sp - 1], Value::Void);
                        }
                        
                        let mut target = self.pop();
                        target.set_index_deep(indexes, to_set)?;
                        self.push(target);
                    } else {
                        let index = self.pop();
                        let mut target = self.pop();
                        target.set_index(index, to_set)?;
                        self.push(target);
                    }
                }

                Op::LoadIndex(count) => {
                    let count = *count;
                    let res = if count > 1 {
                        let mut indexes = Vec::with_capacity(count);
                        let start = self.sp - count;
                        
                        for i in start..(self.sp - 1) {
                            indexes.push(std::mem::replace(&mut self.stack[i], Value::Void));
                        }
                        indexes.push(std::mem::replace(&mut self.tos, Value::Void));
                        
                        self.sp = start;
                        if self.sp > 0 {
                            self.tos = std::mem::replace(&mut self.stack[self.sp - 1], Value::Void);
                        }

                        let value = self.pop();
                        value.load_index_deep(indexes)?
                    } else {
                        let index = self.pop();
                        let value = self.pop();
                        value.load_index(&index)?
                    };
                    self.push(res); 
                }
                Op::StoreGlobal(idx) => {
                    let value = self.pop();
                    if *idx >= self.frame.len() {
                        self.frame.resize(idx + 1, Value::Void);
                    }
                    unsafe {*self.frame.get_unchecked_mut(*idx) = value}
                }
                Op::LoadLocal(idx, depth_delta) => {
                    let base = self.get_frame_base(*depth_delta);
                    let index = base + idx;
                    if index >= self.frame.len() {
                        return Err(VMError::EmptyStack);
                    }
                    self.push(unsafe { self.frame.get_unchecked(index) }.clone());
                }

                Op::StoreLocal(idx, depth_delta) => {
                    let value = self.pop();
                    let base = self.get_frame_base(*depth_delta);
                    let index = base + idx;

                    if index >= self.frame.len() {
                        self.frame.resize(index + 1, Value::Void);
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
                            let mut args = Vec::with_capacity(*n);
                            for _ in 0..*n {
                                args.push(self.pop());
                            }
                            args.reverse();
                            self.run_func(func, args, code)?;
                        }
                        Value::Fn(target_ip, env_frame) => { 
                            let mut args = Vec::with_capacity(*n);
                            for _ in 0..*n {
                                args.push(self.pop());
                            }
                            args.reverse();

                            let next_frame_idx = self.frame.len();
                            let current_idx = unsafe { ip_ptr.offset_from(base_ptr) as usize };

                            let (display, depth) = if (env_frame as usize) < self.call_stack.len() {
                                let parent = &self.call_stack[env_frame as usize];
                                let mut d = parent.display; 
                                let current_depth = parent.depth;
                                
                                if current_depth >= MAX_DEPTH {
                                    return Err(VMError::FuncErr); 
                                }
                                
                                d[current_depth] = parent.frame_idx;
                                (d, current_depth + 1)
                            } else {
                                ([0; MAX_DEPTH], 0)
                            };

                            self.call_stack.push(CallFrame {
                                return_ip: current_idx + 1, 
                                old_frame: self.now_frame,
                                display,
                                depth, 
                                frame_idx: next_frame_idx,
                            });

                            self.now_frame = next_frame_idx;
                            self.frame.extend(args);
                            
                            ip_ptr = unsafe { base_ptr.add(target_ip as usize) };
                            continue;
                        }                   
                        _ => return Err(VMError::FuncErr),
                    }
                }
                Op::Return => {
                    let return_val = self.pop();
                    let frame = self.call_stack.pop().ok_or(VMError::EmptyStack)?;
                    
                    self.frame.truncate(self.now_frame);
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
        Ok(())
    }

}
