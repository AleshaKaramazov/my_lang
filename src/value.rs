use std::cell::RefCell;
use std::cmp::Ordering;
use std::mem::ManuallyDrop;
use std::rc::Rc;
use std::vec::IntoIter;
use crate::consts;
use crate::errors::VMError;
use crate::file::FileHandler;

#[derive(Debug)]
pub struct Value(pub u64);

pub enum UnpackedValue {
    Void,
    Number(i64),
    Str(ManuallyDrop<Rc<RefCell<String>>>),
    Char(char),
    Bool(bool),
    Ref(usize),
    Iter(ManuallyDrop<Box<Iterator>>),
    Fn(u32, u32),
    Set(ManuallyDrop<Rc<RefCell<Vec<Value>>>>),
    Result(ManuallyDrop<Box<Result<Value, Value>>>),
    Cat(ManuallyDrop<Option<Box<Value>>>),
    File(ManuallyDrop<Box<FileHandler>>),
    Float(f64),
    Tuple(ManuallyDrop<Rc<RefCell<Vec<Value>>>>),
}

impl Drop for Value {
    fn drop(&mut self) {
        let tag = self.0 >> 48;
        let payload = self.0 & 0x0000FFFFFFFFFFFF;
        match tag {
            0xFFF3 => { unsafe { drop(Rc::from_raw(payload as *mut RefCell<String>)) } }
            0xFFF7 => { unsafe { drop(Box::from_raw(payload as *mut Iterator)) } }
            0xFFF8 => { unsafe { drop(Box::from_raw(payload as *mut (u32, u32))) } }
            0xFFF9 => { unsafe { drop(Rc::from_raw(payload as *mut RefCell<Vec<Value>>)) } }
            0xFFFA => { unsafe { drop(Box::from_raw(payload as *mut Result<Value, Value>)) } }
            0xFFFB => { if payload != 0 { unsafe { drop(Box::from_raw(payload as *mut Value)) } } }
            0xFFFC => { unsafe { drop(Box::from_raw(payload as *mut FileHandler)) } }
            0xFFFD => { unsafe { drop(Rc::from_raw(payload as *mut RefCell<Vec<Value>>)) } }
            _ => {}
        }
    }
}

impl Clone for Value {
    fn clone(&self) -> Self {
        let tag = self.0 >> 48;
        if tag < 0xFFF1 || tag > 0xFFFD || matches!(tag, 0xFFF1 | 0xFFF2 | 0xFFF4 | 0xFFF5 | 0xFFF6) {
            return Value(self.0);
        }
        match self.unpack() {
            UnpackedValue::Str(s) => Value::from_str((*s).clone()),
            UnpackedValue::Iter(i) => Value::from_iter((*i).clone()),
            UnpackedValue::Fn(a, b) => Value::from_fn(a, b),
            UnpackedValue::Set(s) => Value::from_set((*s).clone()),
            UnpackedValue::Result(r) => Value::from_result((*r).clone()),
            UnpackedValue::Cat(c) => Value::from_cat((*c).clone()),
            UnpackedValue::File(f) => Value::from_file((*f).clone()),
            UnpackedValue::Tuple(t) => Value::from_tuple((*t).clone()),
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }
}

impl Value {
    pub fn unpack(&self) -> UnpackedValue {
        let tag = self.0 >> 48;
        let payload = self.0 & 0x0000FFFFFFFFFFFF;
        if !(0xFFF1..=0xFFFD).contains(&tag) {
            return UnpackedValue::Float(f64::from_bits(self.0));
        }
        match tag {
            0xFFF1 => UnpackedValue::Void,
            0xFFF2 => UnpackedValue::Number(((payload << 16) as i64) >> 16),
            0xFFF3 => UnpackedValue::Str(ManuallyDrop::new(unsafe { Rc::from_raw(payload as *const RefCell<String>) })),
            0xFFF4 => UnpackedValue::Char(unsafe { char::from_u32_unchecked(payload as u32) }),
            0xFFF5 => UnpackedValue::Bool(payload != 0),
            0xFFF6 => UnpackedValue::Ref(payload as usize),
            0xFFF7 => UnpackedValue::Iter(ManuallyDrop::new(unsafe { Box::from_raw(payload as *mut Iterator) })),
            0xFFF8 => {
                let val = unsafe { *(payload as *const (u32, u32)) };
                UnpackedValue::Fn(val.0, val.1)
            }
            0xFFF9 => UnpackedValue::Set(ManuallyDrop::new(unsafe { Rc::from_raw(payload as *const RefCell<Vec<Value>>) })),
            0xFFFA => UnpackedValue::Result(ManuallyDrop::new(unsafe { Box::from_raw(payload as *mut Result<Value, Value>) })),
            0xFFFB => {
                if payload == 0 {
                    UnpackedValue::Cat(ManuallyDrop::new(None))
                } else {
                    UnpackedValue::Cat(ManuallyDrop::new(Some(unsafe { Box::from_raw(payload as *mut Value) })))
                }
            }
            0xFFFC => UnpackedValue::File(ManuallyDrop::new(unsafe { Box::from_raw(payload as *mut FileHandler) })),
            0xFFFD => UnpackedValue::Tuple(ManuallyDrop::new(unsafe { Rc::from_raw(payload as *const RefCell<Vec<Value>>) })),
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub const fn void() -> Self { Value(0xFFF1000000000000) }

    pub fn from_number(n: i64) -> Self {
        Value(0xFFF2000000000000 | ((n as u64) & 0x0000FFFFFFFFFFFF))
    }
    
    pub fn from_str(s: Rc<RefCell<String>>) -> Self {
        let ptr = Rc::into_raw(s) as u64;
        Value(0xFFF3000000000000 | (ptr & 0x0000FFFFFFFFFFFF))
    }
    
    pub const fn from_char(c: char) -> Self {
        Value(0xFFF4000000000000 | ((c as u64) & 0x0000FFFFFFFFFFFF))
    }
    
    pub const fn from_bool(b: bool) -> Self {
        Value(0xFFF5000000000000 | (if b { 1 } else { 0 }))
    }
    
    pub const fn from_ref(r: usize) -> Self {
        Value(0xFFF6000000000000 | ((r as u64) & 0x0000FFFFFFFFFFFF))
    }
    
    pub fn from_iter(i: Box<Iterator>) -> Self {
        let ptr = Box::into_raw(i) as u64;
        Value(0xFFF7000000000000 | (ptr & 0x0000FFFFFFFFFFFF))
    }
    
    pub fn from_fn(a: u32, b: u32) -> Self {
        let ptr = Box::into_raw(Box::new((a, b))) as u64;
        Value(0xFFF8000000000000 | (ptr & 0x0000FFFFFFFFFFFF))
    }
    
    pub fn from_set(s: Rc<RefCell<Vec<Value>>>) -> Self {
        let ptr = Rc::into_raw(s) as u64;
        Value(0xFFF9000000000000 | (ptr & 0x0000FFFFFFFFFFFF))
    }
    
    pub fn from_result(r: Box<Result<Value, Value>>) -> Self {
        let ptr = Box::into_raw(r) as u64;
        Value(0xFFFA000000000000 | (ptr & 0x0000FFFFFFFFFFFF))
    }
    
    pub fn from_cat(c: Option<Box<Value>>) -> Self {
        let ptr = match c {
            Some(b) => Box::into_raw(b) as u64,
            None => 0,
        };
        Value(0xFFFB000000000000 | (ptr & 0x0000FFFFFFFFFFFF))
    }
    
    pub fn from_file(f: Box<FileHandler>) -> Self {
        let ptr = Box::into_raw(f) as u64;
        Value(0xFFFC000000000000 | (ptr & 0x0000FFFFFFFFFFFF))
    }
    
    pub fn from_tuple(t: Rc<RefCell<Vec<Value>>>) -> Self {
        let ptr = Rc::into_raw(t) as u64;
        Value(0xFFFD000000000000 | (ptr & 0x0000FFFFFFFFFFFF))
    }
    
    pub fn from_float(f: f64) -> Self {
        if f.is_nan() {
            Value(0x7FF8000000000000)
        } else {
            Value(f.to_bits())
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Range {
    pub start: i64,
    pub end: i64,
    pub step: i64,
}

#[derive(Debug, Clone)]
pub struct LinesIter {
    pub source: String,
    pub offset: usize,
}

#[derive(Debug, Clone)]
pub struct SplitIter {
    pub source: String,
    pub delimiter: String,
    pub offset: usize,
}

#[derive(Debug, Clone)]
pub struct SplitWhitespaceIter {
    pub source: String,
    pub offset: usize,
}

#[derive(Debug, Clone)]
pub enum Iterator {
    String(IntoIter<char>),
    Set(IntoIter<Value>),
    Range(Range),
    Enumerate(Box<Iterator>, i64),
    Lines(LinesIter),
    Split(SplitIter),
    SplitWhitespace(SplitWhitespaceIter),
}

impl Iterator {
    fn next(&mut self) -> Option<Value> {
        match self {
            Self::Set(i) => i.next(),
            Self::String(s) => s.next().map(Value::from_char),
            Iterator::Range(range) => {
                let current = range.start;
                
                if (range.step > 0 && current >= range.end) || 
                   (range.step < 0 && current <= range.end) {
                    return None;
                }
                range.start += range.step;
                
                Some(Value::from_number(current))
            },
            Self::Enumerate(inner_iter, index) => {
                if let Some(val) = inner_iter.next() {
                    let current_idx = *index;
                    *index += 1; 
                    
                    Some(Value::from_tuple(Rc::new(RefCell::new(vec![Value::from_number(current_idx), val]))))
                } else {
                    None
                }
            }
            Self::Split(iter) => {
                if iter.offset > iter.source.len() {
                    return None;
                }
                let tail = &iter.source[iter.offset..];
                
                if iter.delimiter.is_empty() {
                    let mut chars = tail.chars();
                    if let Some(c) = chars.next() {
                        iter.offset += c.len_utf8();
                        return Some(Value::from_str(Rc::new(RefCell::new(c.to_string()))))
                    } else {
                        iter.offset = iter.source.len() + 1;
                        return None;
                    }
                }

                if let Some(pos) = tail.find(&iter.delimiter) {
                    let part = &tail[..pos];
                    iter.offset += pos + iter.delimiter.len();
                    Some(Value::from_str(Rc::new(RefCell::new(part.to_string()))))
                } else {
                    iter.offset = iter.source.len() + 1; 
                    Some(Value::from_str(Rc::new(RefCell::new(tail.to_string()))))
                }
            }

            Self::SplitWhitespace(iter) => {
                if iter.offset >= iter.source.len() {
                    return None;
                }
                let tail = &iter.source[iter.offset..];
                
                if let Some(start_pos) = tail.find(|c: char| !c.is_whitespace()) {
                    let word_tail = &tail[start_pos..];
                    
                    if let Some(end_pos) = word_tail.find(|c: char| c.is_whitespace()) {
                        let word = &word_tail[..end_pos];
                        iter.offset += start_pos + end_pos;
                        Some(Value::from_str(Rc::new(RefCell::new(word.to_string()))))
                    } else {
                        iter.offset = iter.source.len();
                        Some(Value::from_str(Rc::new(RefCell::new(word_tail.to_string()))))
                    }
                } else {
                    iter.offset = iter.source.len();
                    None
                }
            }
            Self::Lines(iter) => {
                if iter.offset >= iter.source.len() {
                    return None;
                }

                let tail = &iter.source[iter.offset..];
                
                if let Some(newline_pos) = tail.find('\n') {
                    let mut line = &tail[..newline_pos];
                    
                    if line.ends_with('\r') {
                        line = &line[..line.len() - 1];
                    }
                    
                    iter.offset += newline_pos + 1;
                    Some(Value::from_str(Rc::new(RefCell::new(line.to_string()))))
                } else {
                    let line = tail;
                    iter.offset = iter.source.len();
                    Some(Value::from_str(Rc::new(RefCell::new(line.to_string()))))
                }
            }
        } 
    }
}

impl std::io::Write for Value {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self.unpack() {
            UnpackedValue::File(f) => f.file.borrow_mut().write(buf),
            UnpackedValue::Str(filepath) => {
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true).append(true).open(&*filepath.borrow()) {
                    f.write(buf)
                }  
                else {
                    std::io::stdout().write(buf)
                }
            }
            UnpackedValue::Number(u) => {
                match u {
                    consts::STDERR => std::io::stderr().write(buf),
                    consts::STDOUT  => std::io::stdout().write(buf),
                    unk => Err(std::io::Error::other(format!("output by file descriptor is only for:\n\
                        STDOUT: 1\nSTDERR: 2\nget: {}", unk))),
                }
            }
            _ => std::io::stdout().write(buf)
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let UnpackedValue::File(f) = self.unpack() {
            f.file.borrow_mut().flush()
        } else {
            std::io::stdout().flush()
        }
    }
}

impl Value {
    pub fn is_truthy(&self) -> bool {
        match self.unpack() {
            UnpackedValue::Void => false,
            UnpackedValue::Bool(b) => b,
            UnpackedValue::Number(n) => n != 0,
            UnpackedValue::Char(_) => true,
            UnpackedValue::Str(s) => !s.borrow().is_empty(),
            _ => false,
        }
    }

    #[inline(always)]
    pub fn eval_str(&self) -> String {
        match self.unpack() {
            UnpackedValue::Str(s) => s.borrow().to_string(),
            _ => unsafe { std::hint::unreachable_unchecked() }, 
        }
    }

    pub fn open_file(filename: &str) -> Self {
        Value::from_result(Box::new(
            FileHandler::new_file(filename)
                .map(|x| Value::from_file(Box::new(x)))
                .map_err(|_| Value::from_str(Rc::new(RefCell::new(format!("error with open file: {}", filename)))))
        ))
    }

    pub fn new_file(filename: &str, opt: i64) -> Result<Self, VMError> {
        Ok(Value::from_file(Box::new(FileHandler::open(filename, opt)?)))
    }

    pub fn add_assign(&mut self, rhs: Self) -> Result<(), VMError> {
        let self_tag = self.0 >> 48;
        let rhs_tag = rhs.0 >> 48;

        let self_is_num = self_tag == 0xFFF2;
        let rhs_is_num = rhs_tag == 0xFFF2;
        let self_is_float = !(0xFFF1..=0xFFFD).contains(&self_tag);
        let rhs_is_float = !(0xFFF1..=0xFFFD).contains(&rhs_tag);

        if self_is_num && rhs_is_num {
            let a = ((self.0 << 16) as i64) >> 16;
            let b = ((rhs.0 << 16) as i64) >> 16;
            *self = Value::from_number(a + b);
            return Ok(());
        } else if self_is_float && rhs_is_float {
            *self = Value::from_float(f64::from_bits(self.0) + f64::from_bits(rhs.0));
            return Ok(());
        } else if self_is_num && rhs_is_float {
            let a = ((self.0 << 16) as i64) >> 16;
            *self = Value::from_float(a as f64 + f64::from_bits(rhs.0));
            return Ok(());
        } else if self_is_float && rhs_is_num {
            let b = ((rhs.0 << 16) as i64) >> 16;
            *self = Value::from_float(f64::from_bits(self.0) + b as f64);
            return Ok(());
        }

        let new_val = match (self.unpack(), rhs.unpack()) {
            (UnpackedValue::Str(a), b) => {
                let mut a_mut = a.borrow_mut();
                match b {
                    UnpackedValue::Str(s2) => a_mut.push_str(&s2.borrow()),
                    UnpackedValue::Char(c) => a_mut.push(c),
                    _ => a_mut.push_str(&rhs.to_string()),
                }
                return Ok(());
            }
            (UnpackedValue::Char(a), b) => {
                let mut new_str = a.to_string();
                match b {
                    UnpackedValue::Str(s2) => new_str.push_str(&s2.borrow()),
                    UnpackedValue::Char(c) => new_str.push(c),
                    _ => new_str.push_str(&rhs.to_string()),
                }
                Value::from_str(Rc::new(RefCell::new(new_str)))
            }
            (a, UnpackedValue::Str(b)) => {
                let mut new_str = match a {
                    UnpackedValue::Number(n) => n.to_string(),
                    UnpackedValue::Float(f) => f.to_string(),
                    UnpackedValue::Bool(bl) => bl.to_string(),
                    _ => return Err(VMError::BadOperand),
                };
                new_str.push_str(&b.borrow());
                Value::from_str(Rc::new(RefCell::new(new_str)))
            }
            (a, UnpackedValue::Char(b)) => {
                let mut new_str = match a {
                    UnpackedValue::Number(n) => n.to_string(),
                    UnpackedValue::Float(f) => f.to_string(),
                    UnpackedValue::Bool(bl) => bl.to_string(),
                    _ => return Err(VMError::BadOperand),
                };
                new_str.push(b);
                Value::from_str(Rc::new(RefCell::new(new_str)))
            }
            _ => return Err(VMError::BadOperand),
        };
        *self = new_val;
        Ok(())
    }

    pub fn sub_assign(&mut self, rhs: Self) -> Result<(), VMError> {
        let self_tag = self.0 >> 48;
        let rhs_tag = rhs.0 >> 48;

        let self_is_num = self_tag == 0xFFF2;
        let rhs_is_num = rhs_tag == 0xFFF2;
        let self_is_float = !(0xFFF1..=0xFFFD).contains(&self_tag);
        let rhs_is_float = !(0xFFF1..=0xFFFD).contains(&rhs_tag);

        if self_is_num && rhs_is_num {
            let a = ((self.0 << 16) as i64) >> 16;
            let b = ((rhs.0 << 16) as i64) >> 16;
            *self = Value::from_number(a - b);
            Ok(())
        } else if self_is_float && rhs_is_float {
            *self = Value::from_float(f64::from_bits(self.0) - f64::from_bits(rhs.0));
            Ok(())
        } else if self_is_num && rhs_is_float {
            let a = ((self.0 << 16) as i64) >> 16;
            *self = Value::from_float(a as f64 - f64::from_bits(rhs.0));
            Ok(())
        } else if self_is_float && rhs_is_num {
            let b = ((rhs.0 << 16) as i64) >> 16;
            *self = Value::from_float(f64::from_bits(self.0) - b as f64);
            Ok(())
        } else {
            Err(VMError::BadOperand)
        }
    }    

    pub fn mul_assign(&mut self, rhs: Self) -> Result<(), VMError> {
        let self_tag = self.0 >> 48;
        let rhs_tag = rhs.0 >> 48;

        let self_is_num = self_tag == 0xFFF2;
        let rhs_is_num = rhs_tag == 0xFFF2;
        let self_is_float = !(0xFFF1..=0xFFFD).contains(&self_tag);
        let rhs_is_float = !(0xFFF1..=0xFFFD).contains(&rhs_tag);

        if self_is_num && rhs_is_num {
            let a = ((self.0 << 16) as i64) >> 16;
            let b = ((rhs.0 << 16) as i64) >> 16;
            *self = Value::from_number(a * b);
            Ok(())
        } else if self_is_float && rhs_is_float {
            *self = Value::from_float(f64::from_bits(self.0) * f64::from_bits(rhs.0));
            Ok(())
        } else if self_is_num && rhs_is_float {
            let a = ((self.0 << 16) as i64) >> 16;
            *self = Value::from_float(a as f64 * f64::from_bits(rhs.0));
            Ok(())
        } else if self_is_float && rhs_is_num {
            let b = ((rhs.0 << 16) as i64) >> 16;
            *self = Value::from_float(f64::from_bits(self.0) * b as f64);
            Ok(())
        } else {
            Err(VMError::BadOperand)
        }
    }

    pub fn div_assign(&mut self, rhs: Self) -> Result<(), VMError> {
        let self_tag = self.0 >> 48;
        let rhs_tag = rhs.0 >> 48;

        let self_is_num = self_tag == 0xFFF2;
        let rhs_is_num = rhs_tag == 0xFFF2;
        let self_is_float = !(0xFFF1..=0xFFFD).contains(&self_tag);
        let rhs_is_float = !(0xFFF1..=0xFFFD).contains(&rhs_tag);

        if self_is_num && rhs_is_num {
            let a = ((self.0 << 16) as i64) >> 16;
            let b = ((rhs.0 << 16) as i64) >> 16;
            if b == 0 { return Err(VMError::ZeroDiv); }
            *self = Value::from_number(a / b);
            Ok(())
        } else if self_is_float && rhs_is_float {
            let b = f64::from_bits(rhs.0);
            if b == 0.0 { return Err(VMError::ZeroDiv); }
            *self = Value::from_float(f64::from_bits(self.0) / b);
            Ok(())
        } else if self_is_num && rhs_is_float {
            let a = ((self.0 << 16) as i64) >> 16;
            let b = f64::from_bits(rhs.0);
            if b == 0.0 { return Err(VMError::ZeroDiv); }
            *self = Value::from_float(a as f64 / b);
            Ok(())
        } else if self_is_float && rhs_is_num {
            let b = ((rhs.0 << 16) as i64) >> 16;
            if b == 0 { return Err(VMError::ZeroDiv); }
            *self = Value::from_float(f64::from_bits(self.0) / b as f64);
            Ok(())
        } else {
            Err(VMError::BadOperand)
        }
    }

    pub fn arifm_and_assign(&mut self, rhs: Self) -> Result<(), VMError> {
        if self.0 >> 48 == 0xFFF2 && rhs.0 >> 48 == 0xFFF2 {
            let a = ((self.0 << 16) as i64) >> 16;
            let b = ((rhs.0 << 16) as i64) >> 16;
            *self = Value::from_number(a & b);
            Ok(())
        } else {
            Err(VMError::NotOperation)
        }
    }

    pub fn arifm_or_assign(&mut self, rhs: Self) -> Result<(), VMError> {
        if self.0 >> 48 == 0xFFF2 && rhs.0 >> 48 == 0xFFF2 {
            let a = ((self.0 << 16) as i64) >> 16;
            let b = ((rhs.0 << 16) as i64) >> 16;
            *self = Value::from_number(a | b);
            Ok(())
        } else {
            Err(VMError::NotOperation)
        }
    }

    pub fn arifm_mod_assign(&mut self, rhs: Self) -> Result<(), VMError> {
        if self.0 >> 48 == 0xFFF2 && rhs.0 >> 48 == 0xFFF2 {
            let a = ((self.0 << 16) as i64) >> 16;
            let b = ((rhs.0 << 16) as i64) >> 16;
            if b == 0 { return Err(VMError::ZeroDiv); } 
            *self = Value::from_number(a % b);
            Ok(())
        } else {
            Err(VMError::NotOperation)
        }
    }

    pub fn pow_assign(&mut self, rhs: Self) -> Result<(), VMError> {
        if self.0 >> 48 == 0xFFF2 && rhs.0 >> 48 == 0xFFF2 {
            let a = ((self.0 << 16) as i64) >> 16;
            let b = ((rhs.0 << 16) as i64) >> 16;
            if b < 0 {
                return Err(VMError::BadOperand);
            }
            *self = Value::from_number(a.pow(b as u32));
            Ok(())
        } else {
            Err(VMError::NotOperation)
        }
    }

    pub fn next(&mut self) -> Result<Option<Value>, VMError> {
        match self.unpack() {
            UnpackedValue::Iter(mut i) => Ok(i.next()),
            _ => Err(VMError::CantIter)
        }
    }

    pub fn expect_number(&self) -> Result<i64, VMError> {
        match self.unpack() {
            UnpackedValue::Number(i) => Ok(i),
            _ => Err(VMError::UnExpectedType)
        }
    }

    pub fn make_range(start: Value, end: Value, incl: bool) -> Result<Value, VMError> {
        let start = start.expect_number()?;
        let mut end = end.expect_number()?;
        if incl {
            if start > end {
                end -= 1;
            } else {
                end += 1; 
            }
        }
        Ok(Value::from_iter(Box::new(Iterator::Range(Range { start, end, step: if start > end { -1 } else { 1 } }))))
    }

    pub fn new_control(res: Result<Value, String>) -> Value {
        Value::from_result(Box::new(match res {
            Ok(res) => Ok(res),
            Err(er) => Err(Value::from_str(Rc::new(RefCell::new(er))))
        }))
    }

    pub fn make_iter(self) -> Result<Iterator, VMError> {
        let val = match self.unpack() {
            UnpackedValue::Str(s) => {
                let iter = s.borrow().chars().collect::<Vec<char>>().into_iter();
                Iterator::String(iter)
            }
            UnpackedValue::Iter(_) => {
                let b = unsafe { Box::from_raw((self.0 & 0x0000FFFFFFFFFFFF) as *mut Iterator) };
                std::mem::forget(self);
                return Ok(*b);
            }
            UnpackedValue::Set(i) => Iterator::Set(i.borrow().clone().into_iter()),
            _ => return Err(VMError::UnExpectedType),
        };
        Ok(val)
    }

    pub fn set_index(&mut self, index: Self, to_set: Value) -> Result<(), VMError> {
        let index = index.expect_number()? as usize;
        match self.unpack() {
            UnpackedValue::Set(v) => {
                v.borrow_mut()[index] = to_set;
            }
            _ => return Err(VMError::CantIndex)
        }
        Ok(())
    }

    pub fn load_dyap(&self, start: usize, end: usize) -> Result<Value, VMError> {
        let res = match self.unpack() {
            UnpackedValue::Set(s) => {
                let s = s.borrow();
                let end = if end > s.len() { s.len() } else { end };
                Value::from_set(Rc::new(RefCell::new(s[start..end].to_vec())))
            }
            UnpackedValue::Str(s) => {
                let s = s.borrow();
                let count = s.chars().count();
                let end = if end > count { count } else { end };
                Value::from_str(Rc::new(RefCell::new(s[start..end].to_string())))
            }
            _ => return Err(VMError::UnExpectedType),
        };
        Ok(res)
    }

    pub fn new_str<S: Into<String>>(str: S) -> Self {
        Value::from_str(Rc::new(RefCell::new(str.into())))
    }
    
    pub fn set_index_deep(&mut self, index: Vec<Self>, to_set: Value) -> Result<(), VMError> {
        let mut current_rc = match self.unpack() {
            UnpackedValue::Set(v) => (*v).clone(),
            _ => return Err(VMError::CantIndex),
        };

        for item in index.iter().take(index.len() - 1) {
            let idx = item.expect_number()? as usize;

            let next_rc = match current_rc.borrow()[idx].unpack() {
                UnpackedValue::Set(next_v) => (*next_v).clone(),
                _ => return Err(VMError::CantIndex),
            };
            
            current_rc = next_rc;
        }
        let last_idx = index.last().ok_or(VMError::CantIndex)?.expect_number()? as usize;
        current_rc.borrow_mut()[last_idx] = to_set;

        Ok(())
    }

    pub fn load_index_deep(&self, index: Vec<Self>) -> Result<Value, VMError> {
        let mut current = self.clone();

        for i in index.iter() {
            current = current.load_index(i)?.clone();
        }
        Ok(current.clone())
    }
    
    pub fn load_index(&self, index: &Self) -> Result<Value, VMError> {
        let val = match index.unpack() {
            UnpackedValue::Iter(i) => {
                if let Iterator::Range(r) = &**i {
                    if r.start < 0 || r.end < 0 || r.step < 0 || r.start > r.end {
                        return Err(VMError::CantIndex)
                    } 
                    self.load_dyap(r.start as usize, r.end as usize)?
                } else {
                    return Err(VMError::CantIndex)
                }
            } 
            UnpackedValue::Number(i) => {
                match self.unpack() {
                    UnpackedValue::Set(v) => {
                        let v = v.borrow();
                        v[i.rem_euclid(v.len() as i64) as usize].clone()
                    }
                    UnpackedValue::Str(s) => {
                        let s = s.borrow();
                        let index = i.rem_euclid(s.chars().count() as i64) as usize;
                        Value::from_char(s.chars().nth(index).unwrap())
                    }
                    _ => return Err(VMError::CantIndex)
                }
            }
            _ => return Err(VMError::CantIndex)
        };
        Ok(val)
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.unpack() {
            UnpackedValue::File(file) => write!(f, "{}", *file),
            UnpackedValue::Tuple(t) => {
                write!(f, "(")?;
                let t = t.borrow();
                for (i, val) in t.iter().enumerate() {
                    write!(f, "{}", val)?;
                    if i < t.len() - 1 { write!(f, ", ")?; }
                }
                write!(f, ")")
            }
            UnpackedValue::Float(fl) => write!(f, "{}", fl),
            UnpackedValue::Void => write!(f, "()"),
            UnpackedValue::Number(n) => write!(f, "{}", n),
            UnpackedValue::Bool(b) => write!(f, "{}", b),
            UnpackedValue::Char(c) => write!(f, "{}", c),
            UnpackedValue::Str(s) => write!(f, "{}", s.borrow()),
            UnpackedValue::Ref(i) => write!(f, "REF<ID: {}>", i),
            UnpackedValue::Fn(_, _) => write!(f, "FN"),
            UnpackedValue::Iter(i) => write!(f, "Iter<{:?}>", *i),
            UnpackedValue::Cat(c) => if let Some(c) = &*c {
                write!(f, "Cat<{}>", c)
            } else {
                write!(f, "None")
            },
            UnpackedValue::Result(res) => {
                match &**res {
                    Ok(res) => write!(f, "Ok<{}>", res),
                    Err(err) => write!(f, "Err<{}>", err)
                } 
            }
            UnpackedValue::Set(s) => {
                write!(f, "[ ")?;
                for i in s.borrow().iter() {
                    write!(f, "{}, ", i)?;
                }
                write!(f, "]")
            }
        } 
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self.unpack(), other.unpack()) {
            (UnpackedValue::Void, UnpackedValue::Void) => true,
            (UnpackedValue::Number(a), UnpackedValue::Number(b)) => a == b,
            (UnpackedValue::Float(a), UnpackedValue::Float(b)) => a == b,
            
            (UnpackedValue::Float(a), UnpackedValue::Number(b)) => a == (b as f64),
            (UnpackedValue::Number(a), UnpackedValue::Float(b)) => (a as f64) == b,
            
            (UnpackedValue::Char(a), UnpackedValue::Char(b)) => a == b,
            (UnpackedValue::Str(a), UnpackedValue::Str(b)) => *a == *b,
            (UnpackedValue::Bool(a), UnpackedValue::Bool(b)) => a == b,
            (UnpackedValue::Ref(a), UnpackedValue::Ref(b)) => a == b,
            
            (UnpackedValue::Cat(a), UnpackedValue::Cat(b)) => *a == *b,
            (UnpackedValue::Result(a), UnpackedValue::Result(b)) => *a == *b,
            (UnpackedValue::Tuple(a), UnpackedValue::Tuple(b)) => *a == *b,
            (UnpackedValue::Set(a), UnpackedValue::Set(b)) => *a == *b,
            
            _ => false,
        }
    }
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self.unpack(), other.unpack()) {
            (UnpackedValue::Number(a), UnpackedValue::Number(b)) => a.partial_cmp(&b),
            (UnpackedValue::Float(a), UnpackedValue::Float(b)) => a.partial_cmp(&b),
            (UnpackedValue::Float(a), UnpackedValue::Number(b)) => a.partial_cmp(&(b as f64)),
            (UnpackedValue::Number(a), UnpackedValue::Float(b)) => (a as f64).partial_cmp(&b),
            (UnpackedValue::Char(a), UnpackedValue::Char(b)) => a.partial_cmp(&b),
            (UnpackedValue::Str(a), UnpackedValue::Str(b)) => a.borrow().partial_cmp(&b.borrow()),
            (UnpackedValue::Bool(a), UnpackedValue::Bool(b)) => a.partial_cmp(&b),
            
            (UnpackedValue::Tuple(a), UnpackedValue::Tuple(b)) => a.borrow().partial_cmp(&b.borrow()),
            (UnpackedValue::Set(a), UnpackedValue::Set(b)) => a.borrow().partial_cmp(&b.borrow()),
            (UnpackedValue::Cat(a), UnpackedValue::Cat(b)) => a.partial_cmp(&b),
            (UnpackedValue::Result(a), UnpackedValue::Result(b)) => a.partial_cmp(&b),
            
            _ => None, 
        }
    }
}
