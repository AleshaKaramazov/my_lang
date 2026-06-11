use std::rc::Rc;

use crate::{
    op::Op, 
    value::Value,
    errors::VMError,
};

pub struct VM {
    pub stack: Vec<Value>,
    pub sp: usize,
    pub frame: Vec<Value>,
    pub call_stack: Vec<CallFrame>,
    pub now_frame: usize,
}

pub struct CallFrame {
    pub return_ip: usize, 
    pub old_frame: usize, 
    pub static_link: usize, 
    pub frame_idx: usize,   
}

const STACK_MAX: usize = 2048;

impl<'a> VM {
    pub fn new() -> Self {
        Self {
            stack: vec![Value::Void; STACK_MAX],
            sp: 0,
            frame: Vec::with_capacity(32),
            call_stack: Vec::with_capacity(32),
            now_frame: 0,
        }
    }

    #[inline(always)]
    pub fn push(&mut self, val: Value) {
        if self.sp >= self.stack.len() {
            self.stack.resize(self.stack.len() * 2, Value::Void);
        }
        
        unsafe {
            *self.stack.get_unchecked_mut(self.sp) = val;
        }
        self.sp += 1;
    }

    #[inline(always)]
    pub fn pop(&mut self) -> Result<Value, VMError> {
        if self.sp == 0 {
            return Err(VMError::EmptyStack);
        }
        self.sp -= 1;
        unsafe {
            let val = std::mem::replace(self.stack.get_unchecked_mut(self.sp), Value::Void);
            Ok(val)
        }
    }

    fn get_frame_base(&self, depth_delta: usize) -> usize {
        if depth_delta == 0 {
            return self.now_frame;
        }
        let mut current_static_link = self.call_stack.last().unwrap().static_link;
        
        for _ in 1..depth_delta {
            if let Some(f) = self.call_stack.iter().rev().find(|f| f.frame_idx == current_static_link) {
                current_static_link = f.static_link;
            } else {
                break;
            }
        }
        current_static_link
    }

    #[inline(always)]
    pub fn step(&mut self, code: &[Op<'a>], ip: &mut usize) -> Result<(), VMError> {
        let op = unsafe {code.get_unchecked(*ip) } ;
        match op {
            Op::PushFLoat(f) => self.push(Value::Float(*f)),
            Op::PushStr(s) => self.push(Value::Str(Rc::new(s.to_string()))),
            Op::PushChar(c) => self.push(Value::Char(*c)),
            Op::PushNumber(n) => self.push(Value::Number(*n)),
            Op::PushBool(b) => self.push(Value::Bool(*b)),
            Op::PushRefGlobal(idx) => self.push(Value::Ref(*idx)),
            Op::PushFn(id) => self.push(Value::Fn(*id, self.now_frame)),
            Op::PushVoid => self.push(Value::Void),
            Op::Pop => {
                self.pop()?;
            }
            Op::Dup => {
                if self.sp == 0 { return Err(VMError::EmptyStack); }
                let val = unsafe { self.stack.get_unchecked(self.sp - 1).clone() };
                self.push(val);
            }

            Op::ExpectType(tp) => {
                if self.sp == 0 { return Err(VMError::EmptyStack); }
                let val = unsafe { self.stack.get_unchecked(self.sp - 1) };
                if !val.this_type(&tp) {
                    println!("Expected: {:?}, find: {}", tp, val);
                    return Err(VMError::UnExpectedType);
                }
            }
            Op::Plus | Op::Mod | Op::Sub | Op::Mult | Op::Div | Op::Pow | Op::ArifmAnd | Op::ArifmOr => {
                let right = unsafe { self.pop().unwrap_unchecked() };
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
            }
            Op::Equal | Op::NotEqual | Op::Greater | Op::Less | Op::GreaterEq | Op::LessEq => {
                let right = unsafe { self.pop().unwrap_unchecked() };
                let left = unsafe { self.pop().unwrap_unchecked() };
                
                let result = match *op {
                    Op::Equal => left == right,
                    Op::Greater => left > right,
                    Op::NotEqual => left != right,
                    Op::Less => left < right,
                    Op::GreaterEq => left >= right,
                    Op::LessEq => left <= right,
                    _ => unreachable!(),
                };
                self.push(Value::Bool(result));
            }
            Op::MakeTuple(count) => {
                if self.sp < *count { return Err(VMError::EmptyStack); }
                let start = self.sp - count;
                
                let vals: Vec<Value> = self.stack[start..self.sp].to_vec();
                
                for i in start..self.sp {
                    self.stack[i] = Value::Void;
                }
                
                self.sp = start;
                self.push(Value::Tuple(vals));
            }
            Op::UnpackTuple(count) => {
                let val = self.pop()?;
                if let Value::Tuple(vals) = val {
                    if vals.len() != *count {
                        return Err(VMError::EmptyStack);
                    }
                    for v in vals {
                        self.push(v);
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
                let val = self.pop()?;
                self.push(Value::Result(Box::new(Ok(val))));
            }
            Op::MakeErr => {
                let val = self.pop()?;
                self.push(Value::Result(Box::new(Err(val))));
            }
            Op::MakeSome => {
                let val = self.pop()?;
                self.push(Value::Cat(Some(Box::new(val))));
            }
            Op::None => {
                self.push(Value::Cat(None));
            }
            Op::SafeUnwR(target) => {
                let val = self.pop()?;
                match val {
                    Value::Result(inner) => if let Ok(inner) = *inner {
                        self.push(inner);
                    } else {
                        *ip = *target;
                        return Ok(());
                    }
                    Value::Cat(Some(inner)) => {
                        self.push(*inner);
                    }
                    _ => {
                        *ip = *target;
                        return Ok(());
                    }
                }
            }
            Op::SafeUnwL(target) => {
                let val = self.pop()?;
                match val {
                    Value::Result(inner) => if let Err(inner) = *inner {
                        self.push(inner);
                    } 
                    else {
                        *ip = *target;
                        return Ok(());
                    }
                    _ => {
                        *ip = *target;
                        return Ok(());
                    }
                }
            }
            Op::MakeRange(incl) => {
                let end = self.pop()?;
                let start = self.pop()?;
                self.push(Value::make_range(start, end, *incl)?);
            }
            Op::MakeIter => {
                let val = self.pop()?;
                self.push(
                    if matches!(val, Value::Iter(_)) {val} else {Value::Iter(val.make_iter()?)}
                );
            }
            Op::IterNext(i) => {
                if self.sp == 0 { return Err(VMError::EmptyStack); }
                
                let val = unsafe { self.stack.get_unchecked_mut(self.sp - 1) }.next()?;
                
                match val {
                    Some(val) => self.push(val),
                    None => {
                        *ip = *i;
                        return Ok(()); 
                    }
                }
            }
            Op::Not => {
                let val = self.pop()?;
                self.push(Value::Bool(!val.is_truthy()));
            }
            Op::Jump(target) => {
                *ip = *target;
                return Ok(()); 
            }
            Op::JumpIfFalse(target) => {
                let val = self.pop()?;
                if !val.is_truthy() {
                    *ip = *target;
                    return Ok(());
                }
            }
            Op::MakeSet(i) => {
                if self.sp < *i { return Err(VMError::EmptyStack); }
                let start_idx = self.sp - i;
                let vals: Vec<Value> = self.stack[start_idx..self.sp].to_vec();
                for idx in start_idx..self.sp {
                    self.stack[idx] = Value::Void;
                }
                
                self.sp = start_idx;
                
                self.push(Value::Set(Rc::new(vals))); 
            }
            Op::JumpIfTrue(target) => {
                let val = self.pop()?;
                if val.is_truthy() {
                    *ip = *target;
                    return Ok(());
                }
            }
            Op::DupTarget(deep) => {
                if self.sp < 1 + deep { return Err(VMError::EmptyStack); }
                let start = self.sp - (1 + deep); 
                
                let mut to_dup = Vec::with_capacity(self.sp - start);
                for i in start..self.sp {
                    to_dup.push(unsafe { self.stack.get_unchecked(i) }.clone());
                }
                
                for val in to_dup {
                    self.push(val);
                }
            }
            Op::StoreIndex(count) => {
                let to_set = self.pop()?;
                if *count > 1 {
                    if self.sp < *count { return Err(VMError::EmptyStack); }
                    let index_start = self.sp - count;
                    
                    let indexes: Vec<Value> = self.stack[index_start..self.sp].to_vec();
                    
                    for idx in index_start..self.sp {
                        self.stack[idx] = Value::Void;
                    }
                    self.sp = index_start;

                    let mut target = self.pop()?;
                    
                    target.set_index_deep(indexes, to_set)?;
                    self.push(target); 
                } else {
                    let index = self.pop()?;
                    let mut target = self.pop()?;
                    
                    target.set_index(index, to_set)?;
                    self.push(target); 
                }
            }
            Op::LoadIndex(count) => {
                let res = if *count > 1 {
                    if self.sp < *count { return Err(VMError::EmptyStack); }
                    let index_start = self.sp - count;
                    
                    let indexes: Vec<Value> = self.stack[index_start..self.sp].to_vec();
                    
                    for idx in index_start..self.sp {
                        self.stack[idx] = Value::Void;
                    }
                    self.sp = index_start;

                    let value = self.pop()?;
                    value.load_index_deep(indexes)?
                } else {
                    let index = self.pop()?;
                    let value = self.pop()?;
                    value.load_index(&index)?
                };
                self.push(res); 
            }
            Op::StoreGlobal(idx) => {
                let value = self.pop()?;
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
                let value = self.pop()?;
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
                let func_val = self.pop()?;
                
                match func_val {
                    Value::Str(func_name) => {
                        let mut args = Vec::with_capacity(*n);
                        for _ in 0..*n {
                            args.push(unsafe {self.pop().unwrap_unchecked() });
                        }
                        args.reverse();
                        self.run_func(&func_name, args, code)?;
                    }
                    Value::Fn(target_ip, env_frame) => { 
                        let mut args = Vec::with_capacity(*n);
                        for _ in 0..*n {
                            args.push(unsafe {self.pop().unwrap_unchecked() });
                        }
                        args.reverse();

                        let next_frame_idx = self.frame.len();
                        self.call_stack.push(CallFrame {
                            return_ip: *ip + 1, 
                            old_frame: self.now_frame,
                            static_link: env_frame, 
                            frame_idx: next_frame_idx,
                        });

                        self.now_frame = next_frame_idx;
                        self.frame.extend(args);
                        *ip = target_ip;
                        return Ok(()); 
                    }
                    _ => return Err(VMError::FuncErr),
                }
            }
            Op::Return => {
                let return_val = self.pop()?;
                let frame = self.call_stack.pop().ok_or_else(|| VMError::EmptyStack)?;
                
                self.frame.truncate(self.now_frame);
                
                self.now_frame = frame.old_frame;
                *ip = frame.return_ip;
                
                self.push(return_val);
                return Ok(()); 
            }
        }
        *ip += 1;
        Ok(())
    }

    pub fn run(&mut self, code: &[Op<'a>]) -> Result<(), VMError> {
        let mut ip = 0;
        while ip < code.len() {
            self.step(&code, &mut ip)?;
        }
        Ok(())
    }
}
