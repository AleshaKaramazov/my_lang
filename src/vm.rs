use std::cell::RefCell;
use std::rc::Rc;
use crate::op::Op;
use crate::consts;
use crate::errors::VMError;
use crate::value::{UnpackedValue, Value};

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
            stack: vec![Value::void(); STACK_MAX],
            tos: Value::void(),
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
            return Value::void(); 
        }
        self.sp -= 1;
        let popped = std::mem::replace(&mut self.tos, Value::void());
        if self.sp > 0 {
            unsafe {
                self.tos = std::mem::replace(self.stack.get_unchecked_mut(self.sp - 1), Value::void());
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
                Op::PushFLoat(f) => self.push(Value::from_float(*f)),
                Op::PushStr(s) => self.push(Value::from_str(Rc::new(RefCell::new(s.to_string())))),
                Op::PushChar(c) => self.push(Value::from_char(*c)),
                Op::PushNumber(n) => self.push(Value::from_number(*n)),
                Op::PushBool(b) => self.push(Value::from_bool(*b)),
                Op::PushRefGlobal(idx) => self.push(Value::from_ref(*idx)),
                Op::PushFn(id) => {
                    let env_idx = self.call_stack.len().saturating_sub(1);
                    self.push(Value::from_fn(*id as u32, env_idx as u32));
                }
                Op::PushVoid => self.push(Value::void()),
                Op::Pop => {
                    self.pop();
                }
                Op::Dup => {
                    let val = self.tos.clone();
                    self.push(val);
                }
                Op::Try => {
                    let val = self.pop();
                    match val.unpack() {
                        UnpackedValue::Result(inner) => match &**inner {
                            Ok(inner_val) => self.push(inner_val.clone()),
                            Err(err_val) => {
                                let return_val = Value::from_result(Box::new(Err(err_val.clone())));
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
                        UnpackedValue::Cat(inner) => match &*inner {
                            Some(inner_val) => {
                                self.push(*inner_val.clone());
                            }
                            None => {
                                let return_val = Value::from_cat(None);
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
                    let right = std::mem::replace(&mut self.tos, Value::void());
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
                    
                    self.tos = std::mem::replace(left, Value::void());
                }
                Op::Equal | Op::NotEqual | Op::Greater | Op::Less | Op::GreaterEq | Op::LessEq => {
                    let right = std::mem::replace(&mut self.tos, Value::void());
                    self.sp -= 1;
                    let left = std::mem::replace(unsafe { self.stack.get_unchecked_mut(self.sp - 1) }, Value::void());
                    
                    let result = match *op {
                        Op::Equal => left == right,
                        Op::Greater => left > right,
                        Op::NotEqual => left != right,
                        Op::Less => left < right,
                        Op::GreaterEq => left >= right,
                        Op::LessEq => left <= right,
                        _ => unreachable!(),
                    };
                    self.tos = Value::from_bool(result);
                }
                Op::MakeTuple(count) => {
                    let count = *count;
                    if count == 0 {
                        self.push(Value::from_tuple(Rc::new(RefCell::new(Vec::new()))));
                    } else {
                        let start = self.sp - count;
                        let mut vals = Vec::with_capacity(count);
                        
                        unsafe {
                            let src = self.stack.as_mut_ptr().add(start);
                            let dst = vals.as_mut_ptr();
                            
                            std::ptr::copy_nonoverlapping(src, dst, count - 1);
                            std::ptr::write(dst.add(count - 1), std::mem::replace(&mut self.tos, Value::void()));
                            vals.set_len(count);
                            
                            for i in 0..(count - 1) {
                                std::ptr::write(src.add(i), Value::void());
                            }
                        }
                        
                        self.tos = Value::from_tuple(Rc::new(RefCell::new(vals)));
                        self.sp = start + 1;
                    }
                }
                Op::UnpackTuple(count) => {
                    let val = self.pop();
                    if let UnpackedValue::Tuple(vals) = val.unpack() {
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
                    let val = std::mem::replace(&mut self.tos, Value::void());
                    self.tos = Value::from_result(Box::new(Ok(val)));
                }
                Op::MakeErr => {
                    let val = std::mem::replace(&mut self.tos, Value::void());
                    self.tos = Value::from_result(Box::new(Err(val)));
                }
                Op::MakeSome => {
                    let val = std::mem::replace(&mut self.tos, Value::void());
                    self.tos = Value::from_cat(Some(Box::new(val)));
                }
                Op::None => {
                    self.push(Value::from_cat(None));
                }
                Op::MakeRange(incl) => {
                    let end = self.pop();
                    let start = self.pop();
                    self.push(Value::make_range(start, end, *incl)?);
                }
                Op::MakeIter => {
                    let vale = self.pop();
                    let val = vale.unpack();
                    
                    self.push(
                        if matches!(val, UnpackedValue::Iter(_)) {vale} else {Value::from_iter(Box::new(vale.make_iter()?))}
                    );
                }
                Op::Not => {
                    self.tos = Value::from_bool(!self.tos.is_truthy());
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
                    let val = std::mem::replace(&mut self.tos, Value::void());
                    match val.unpack() {
                        UnpackedValue::Result(inner) => if let Err(inner) = &**inner {
                            self.tos = inner.clone()
                        } 
                        else {
                            self.sp -= 1;
                            if self.sp > 0 {
                                self.tos = std::mem::replace(unsafe { self.stack.get_unchecked_mut(self.sp - 1) }, Value::void());
                            }
                            ip_ptr = unsafe { base_ptr.add(*target) };
                            continue;
                        }
                        _ => {
                            self.sp -= 1;
                            if self.sp > 0 {
                                self.tos = std::mem::replace(unsafe { self.stack.get_unchecked_mut(self.sp - 1) }, Value::void());
                            }
                            ip_ptr = unsafe { base_ptr.add(*target) };
                            continue;
                        }
                    }
                }
                Op::SafeUnwR(target) => {
                    let val = std::mem::replace(&mut self.tos, Value::void());
                    match val.unpack() {
                        UnpackedValue::Result(inner) => if let Ok(inner_val) = &**inner {
                            self.tos = inner_val.clone(); 
                        } else {
                            self.sp -= 1;
                            if self.sp > 0 {
                                self.tos = std::mem::replace(unsafe { self.stack.get_unchecked_mut(self.sp - 1) }, Value::void());
                            }
                            ip_ptr = unsafe { base_ptr.add(*target) };
                            continue;
                        }
                        UnpackedValue::Cat(inner) => {
                            if let Some(inner_val) = &*inner {
                                self.tos = *inner_val.clone()
                            } else {
                                self.sp -= 1;
                                if self.sp > 0 {
                                    self.tos = std::mem::replace(unsafe { self.stack.get_unchecked_mut(self.sp - 1) }, Value::void());
                                }
                                ip_ptr = unsafe { base_ptr.add(*target) };
                                continue;
                            }
                        }
                        _ => {
                            self.sp -= 1;
                            if self.sp > 0 {
                                self.tos = std::mem::replace(unsafe { self.stack.get_unchecked_mut(self.sp - 1) }, Value::void());
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
                        self.push(Value::from_set(Rc::new(RefCell::new(Vec::new()))));
                    } else {
                        let start = self.sp - count;
                        let mut vals = Vec::with_capacity(count);
                        
                        unsafe {
                            let src = self.stack.as_mut_ptr().add(start);
                            let dst = vals.as_mut_ptr();
                            
                            std::ptr::copy_nonoverlapping(src, dst, count - 1);
                            std::ptr::write(dst.add(count - 1), std::mem::replace(&mut self.tos, Value::void()));
                            vals.set_len(count);
                            
                            for i in 0..(count - 1) {
                                std::ptr::write(src.add(i), Value::void());
                            }
                        }
                        
                        self.tos = Value::from_set(Rc::new(RefCell::new(vals)));
                        self.sp = start + 1;
                    }
                }
                Op::DupTarget(deep) => {
                    let count = *deep + 1;
                    let start = self.sp - count;
                    
                    for i in 0..(count - 1) {
                        let val = unsafe { self.stack.get_unchecked(start + i) }.clone();
                        self.push(val);
                    }
                    let tos_clone = self.tos.clone();
                    self.push(tos_clone);
                }
               Op::StoreIndex(count) => {
                    let count = *count;
                    let to_set = self.pop();
                    
                    if count > 1 {
                        let mut indexes = Vec::with_capacity(count);
                        let start = self.sp - count;
                        
                        unsafe {
                            let src = self.stack.as_mut_ptr().add(start);
                            let dst = indexes.as_mut_ptr();
                            
                            std::ptr::copy_nonoverlapping(src, dst, count - 1);
                            std::ptr::write(dst.add(count - 1), std::mem::replace(&mut self.tos, Value::void()));
                            indexes.set_len(count);
                            
                            for i in 0..(count - 1) {
                                std::ptr::write(src.add(i), Value::void());
                            }
                        }
                        
                        self.sp = start;
                        if self.sp > 0 {
                            self.tos = std::mem::replace(&mut self.stack[self.sp - 1], Value::void());
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
                        
                        unsafe {
                            let src = self.stack.as_mut_ptr().add(start);
                            let dst = indexes.as_mut_ptr();
                            
                            std::ptr::copy_nonoverlapping(src, dst, count - 1);
                            std::ptr::write(dst.add(count - 1), std::mem::replace(&mut self.tos, Value::void()));
                            indexes.set_len(count);
                            
                            for i in 0..(count - 1) {
                                std::ptr::write(src.add(i), Value::void());
                            }
                        }
                        
                        self.sp = start;
                        if self.sp > 0 {
                            self.tos = std::mem::replace(&mut self.stack[self.sp - 1], Value::void());
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
                        self.frame.resize(idx + 1, Value::void());
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
                        self.frame.resize(index + 1, Value::void());
                    }
                    unsafe { *self.frame.get_unchecked_mut(index) = value; }
                }

                Op::PushRefLocal(idx, depth_delta) => {
                    let base = self.get_frame_base(*depth_delta);
                    self.push(Value::from_ref(base + idx));
                }
                Op::CallFunc(n) => {
                    let func_val = self.pop();
                    
                    match func_val.unpack() {
                        UnpackedValue::Number(func) => {
                            if *n > 0 {
                                self.stack[self.sp - 1] = std::mem::replace(&mut self.tos, Value::void());
                                
                                self.sp -= n;
                                
                                if self.sp > 0 {
                                    self.tos = std::mem::replace(&mut self.stack[self.sp - 1], Value::void());
                                }
                            }
                            
                            self.run_func(func, *n, code)?;
                        }
                        UnpackedValue::Fn(target_ip, env_frame) => { 
                            let next_frame_idx = self.frame.len(); 
                            if *n > 0 {
                                self.stack[self.sp - 1] = std::mem::replace(&mut self.tos, Value::void());
                                self.sp -= n;
                                
                                self.frame.reserve(*n);
                                unsafe {
                                    let src = self.stack.as_mut_ptr().add(self.sp);
                                    let dst = self.frame.as_mut_ptr().add(self.frame.len());
                                    
                                    std::ptr::copy_nonoverlapping(src, dst, *n);
                                    self.frame.set_len(self.frame.len() + *n);
                                    
                                    for i in 0..*n {
                                        std::ptr::write(src.add(i), Value::void());
                                    }
                                }
                                
                                if self.sp > 0 {
                                    self.tos = std::mem::replace(&mut self.stack[self.sp - 1], Value::void());
                                }
                            }

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
