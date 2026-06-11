use std::cmp::Ordering;
use std::rc::Rc;
use std::vec::IntoIter;

use crate::consts;
use crate::errors::VMError;
use crate::file::FileHandler;
use crate::types::Type;

#[derive(Debug, Clone)]
pub enum Value {
    Void,
    Number(i64),
    Str(Rc<String>),
    Char(char),
    Bool(bool),
    Ref(usize),
    Iter(Box<Iterator>),
    Fn(u32, u32),
    Set(Rc<Vec<Value>>),
    Result(Box<Result<Value, Value>>),
    Cat(Option<Box<Value>>),
    File(Box<FileHandler>),
    Float(f64),
    Tuple(Rc<Vec<Value>>),
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
            Self::String(s) => s.next().map(Value::Char),
            Iterator::Range(range) => {
                let current = range.start;
                
                if (range.step > 0 && current >= range.end) || 
                   (range.step < 0 && current <= range.end) {
                    return None;
                }
                range.start += range.step;
                
                Some(Value::Number(current))
            },
            Self::Enumerate(inner_iter, index) => {
                if let Some(val) = inner_iter.next() {
                    let current_idx = *index;
                    *index += 1; 
                    
                    Some(Value::Tuple(Rc::new(vec![Value::Number(current_idx), val])))
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
                        return Some(Value::Str(Rc::new(c.to_string())))
                    } else {
                        iter.offset = iter.source.len() + 1;
                        return None;
                    }
                }

                if let Some(pos) = tail.find(&iter.delimiter) {
                    let part = &tail[..pos];
                    iter.offset += pos + iter.delimiter.len();
                    Some(Value::Str(Rc::new(part.to_string())))
                } else {
                    iter.offset = iter.source.len() + 1; 
                    Some(Value::Str(Rc::new(tail.to_string())))
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
                        Some(Value::Str(Rc::new(word.to_string())))
                    } else {
                        iter.offset = iter.source.len();
                        Some(Value::Str(Rc::new(word_tail.to_string())))
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
                    Some(Value::Str(Rc::new(line.to_string())))
                } else {
                    let line = tail;
                    iter.offset = iter.source.len();
                    Some(Value::Str(Rc::new(line.to_string())))
                }
            }
        } 
    }
}

impl std::io::Write for Value {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Value::File(f) = self {
            f.file.borrow_mut().write(buf) 
        } else if let Value::Str(filepath) = self {
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true).append(true).open(&**filepath) {
                f.write(buf)
            }  
            else {
                std::io::stdout().write(buf)
            }
        } else if let Ok(u) = self.expect_number() {
            match u {
                consts::STDERR => std::io::stderr().write(buf),
                consts::STDOUT  => std::io::stdout().write(buf),
                unk => Err(std::io::Error::other(format!("output by file descriptor is only for:\n\
                    STDOUT: 1\nSTDERR: 2\nget: {}", unk))),
            }
        } 
        else {
            std::io::stdout().write(buf)
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Value::File(f) = self {
            f.file.borrow_mut().flush()
        } else {
            std::io::stdout().flush()
        }
    }
}

impl Value {
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Void => false,
            Value::Bool(b) => *b,
            Value::Number(n) => *n != 0,
            Value::Char(_) => true,
            Value::Str(s) => !s.is_empty(),
            _ => false,
        }
    }

    pub fn eval_str(&self) -> Result<&str, VMError> {
        match self {
            Self::Str(s) => Ok(s),
            _ => Err(VMError::UnExpectedType)
        }
    }

    pub fn open_file(filename: &str) -> Self {
        Value::Result(Box::new(
            FileHandler::new_file(filename)
                .map(|x| Value::File(Box::new(x))).map_err(|_| Value::Str(Rc::new(format!("error with open file: {}", filename))))
        ))
    }

    pub fn new_file(filename: &str, opt: i64) -> Result<Self, VMError> {
        Ok(Value::File(Box::new(FileHandler::open(filename, opt)?)))
    }
    pub fn add_assign(&mut self, rhs: Self) -> Result<(), VMError> {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => { *a += b; Ok(()) }
            (Value::Float(a), Value::Float(b)) => { *a += b; Ok(()) }
            (Value::Float(a), Value::Number(b)) => { *a += b as f64; Ok(()) }
            (s @ Value::Number(_), Value::Float(b)) => {
                let a = s.expect_number()?;
                *s = Value::Float(a as f64 + b);
                Ok(())
            }
            (Value::Str(a), b) => {
                let a = Rc::make_mut(a);
                match b {
                    Value::Str(s2) => a.push_str(&s2),
                    Value::Char(c) => a.push(c),
                    _ => a.push_str(&b.to_string()),
                }
                Ok(())
            }
            (s @ Value::Char(_), b) => {
                let mut new_str = s.to_string();
                match b {
                    Value::Str(s2) => new_str.push_str(&s2),
                    Value::Char(c) => new_str.push(c),
                    _ => new_str.push_str(&b.to_string()),
                }
                *s = Value::Str(Rc::new(new_str)); Ok(())
            }
            (s, Value::Str(b)) => {
                let mut new_str = s.to_string();
                new_str.push_str(&b);
                *s = Value::Str(Rc::new(new_str));
                Ok(())
            }
            (s, Value::Char(b)) => {
                let mut new_str = s.to_string();
                new_str.push(b);
                *s = Value::Str(Rc::new(new_str));
                Ok(())
            }
            _ => Err(VMError::BadOperand),
        }
    }

    pub fn sub_assign(&mut self, rhs: Self) -> Result<(), VMError> {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => { *a -= b; Ok(()) }
            (Value::Float(a), Value::Float(b)) => { *a -= b; Ok(()) }
            (Value::Float(a), Value::Number(b)) => { *a -= b as f64; Ok(()) }
            (s @ Value::Number(_), Value::Float(b)) => {
                let a = s.expect_number()?;
                *s = Value::Float(a as f64 - b);
                Ok(())
            }
            _ => Err(VMError::BadOperand),
        }
    }

    pub fn mul_assign(&mut self, rhs: Self) -> Result<(), VMError> {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => { *a *= b; Ok(()) }
            (Value::Float(a), Value::Float(b)) => { *a *= b; Ok(()) }
            (Value::Float(a), Value::Number(b)) => { *a *= b as f64; Ok(()) }
            (s @ Value::Number(_), Value::Float(b)) => {
                let a = s.expect_number()?;
                *s = Value::Float(a as f64 * b);
                Ok(())
            }
            _ => Err(VMError::BadOperand),
        }
    }

    pub fn div_assign(&mut self, rhs: Self) -> Result<(), VMError> {
        match (self, rhs) {
            (_, Value::Number(0)) => Err(VMError::ZeroDiv),
            (Value::Number(a), Value::Number(b)) => { *a /= b; Ok(()) }
            (_, Value::Float(0.0)) => Err(VMError::ZeroDiv),
            (Value::Float(a), Value::Float(b)) => { *a /= b; Ok(()) }
            (Value::Float(a), Value::Number(b)) => { *a /= b as f64; Ok(()) }
            (s @ Value::Number(_), Value::Float(b)) => {
                let a = s.expect_number()?;
                *s = Value::Float(a as f64 / b);
                Ok(())
            }
            _ => Err(VMError::BadOperand),
        }
    }

    pub fn arifm_and_assign(&mut self, rhs: Self) -> Result<(), VMError> {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => { *a &= b; Ok(()) }
            _ => Err(VMError::NotOperation),
        }
    }

    pub fn arifm_or_assign(&mut self, rhs: Self) -> Result<(), VMError> {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => { *a |= b; Ok(()) }
            _ => Err(VMError::NotOperation),
        }
    }

    pub fn arifm_mod_assign(&mut self, rhs: Self) -> Result<(), VMError> {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => { *a %= b; Ok(()) }
            _ => Err(VMError::NotOperation),
        }
    }

    pub fn pow_assign(&mut self, rhs: Self) -> Result<(), VMError> {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => {
                if b < 0 {
                    Err(VMError::BadOperand)
                } else {
                    *a = a.pow(b as u32);
                    Ok(())
                }
            }
            _ => Err(VMError::NotOperation),
        }
    }

    pub fn this_type(&self, expected: &Type) -> bool {
        match (self, expected) {
            (Value::Number(_), Type::Number) => true,
            (Value::File(_), Type::File) => true,
            (Value::Float(_), Type::Float) => true,
            (Value::Void, Type::Void) => true,
            (Value::Str(_), Type::Str) => true,
            (Value::Bool(_), Type::Bool) => true,
            (Value::Void, Type::Result(inner)) => matches!(inner.0, Type::Void),
            (Value::Set(arr), Type::Set(inner_type)) => {
                arr.iter().all(|val| val.this_type(inner_type))
            }
            (Value::Result(res), Type::Result(inner)) => {
                match &**res {
                    Ok(val) => val.this_type(&inner.0),
                    Err(val) => val.this_type(&inner.1),
                }
            }
            (Value::Cat(cat), Type::Cat(inner_ty)) => {
                match cat {
                    Some(val) => val.this_type(inner_ty),
                    None => true, 
                }
            }
            _ => false,
        }
    }    

    pub fn next(&mut self) -> Result<Option<Value>, VMError> {
        match self {
            Value::Iter(i) => Ok(i.next()),
            _ => Err(VMError::CantIter)
        }
    }

    pub fn expect_number(&self) -> Result<i64, VMError> {
        match self {
            Self::Number(i) => Ok(*i),
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
        Ok(Value::Iter(Box::new(Iterator::Range(Range { start, end, step: if start > end {-1} else {1} }))))
    }

    pub fn new_control(res: Result<Value, String>) -> Value {
        Value::Result(Box::new(match res {
            Ok(res) => Ok(res),
            Err(er) => Err(Value::Str(Rc::new(er)))
        }))
    }

    pub fn make_iter(self) -> Result<Iterator, VMError> {
        let val = match self {
            Self::Str(s) => {
                let iter = s.chars().collect::<Vec<char>>().into_iter();
                Iterator::String(iter)
            }
            Self::Iter(i) => *i,
            Self::Set(i) => Iterator::Set((*i).clone().into_iter()),
            _ => return Err(VMError::UnExpectedType),
        };

        Ok(val)
    }

    pub fn set_index(&mut self, index: Self, to_set: Value) -> Result<(), VMError> {
        let index = index.expect_number()? as usize;
        match self {
            Value::Set(v) => {
                let v = Rc::make_mut(v);
                v[index] = to_set;
            }
            _ => return Err(VMError::CantIndex)
        }
        Ok(())
    }

    pub fn load_dyap(&self, start: usize, end: usize) -> Result<Value, VMError> {
        let res = match self {
            Value::Set(s) => {
                let end = if end > s.len() {s.len()} else {end};
                Value::Set(Rc::new(s[start..end].to_vec()))
            }
            Value::Str(s) => {
                let count = s.chars().count();
                let end = if end > count {count} else {end};
                Value::Str(Rc::new(s[start..end].to_string()))
            }
            _ => return Err(VMError::UnExpectedType),
        };
        Ok(res)
    }

    pub fn set_index_deep(&mut self, index: Vec<Self>, to_set: Value) -> Result<(), VMError> {
        let mut current = self;

        for idx in index.iter().take(index.len() - 1).map(|x| x.expect_number()) {
            match current {
                Value::Set(v) => {
                    let v = Rc::make_mut(v);
                    current = &mut v[idx? as usize];
                }
                _ => return Err(VMError::CantIndex),
            }
        }

        let last_idx = index.last().unwrap().expect_number()? as usize;
        match current {
            Value::Set(v) => {
                let v = Rc::make_mut(v);
                v[last_idx] = to_set;
            }
            _ => return Err(VMError::CantIndex),
        }

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
        let val = match index {
            Value::Iter(i) => {
                if let Iterator::Range(r) = &**i {
                    if r.start < 0 || r.end < 0 || r.step < 0 || r.start > r.end {
                        return Err(VMError::CantIndex)
                    } 
                    self.load_dyap(r.start as usize, r.end as usize)?
                } else {
                    return Err(VMError::CantIndex)
                }
            } 
            Value::Number(i) => {
                match self {
                    Value::Set(v) =>
                        v[i.rem_euclid(v.len() as i64) as usize].clone(),
                    Value::Str(s) => {
                        let index = i.rem_euclid(s.chars().count() as i64) as usize;
                        s.chars().nth(index).map(Value::Char).unwrap()
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
        match self {
            Self::File(file) => write!(f, "{}", file),
            Self::Tuple(t) => {
                write!(f, "(")?;
                for (i, val) in t.iter().enumerate() {
                    write!(f, "{}", val)?;
                    if i < t.len() - 1 { write!(f, ", ")?; }
                }
                write!(f, ")")
            }
            Self::Float(fl) => write!(f, "{}", fl),
            Self::Void => write!(f, "()",),
            Self::Number(n) => write!(f, "{}", n),
            Self::Bool(b) => write!(f, "{}", b),
            Self::Char(c) => write!(f, "{}", c),
            Self::Str(s) => write!(f, "{}", s),
            Self::Ref(i) => write!(f, "REF<ID: {}>", i),
            Self::Fn(_, _) => write!(f, "FN"),
            Self::Iter(i) => write!(f, "Iter<{:?}>", i),
            Value::Cat(c) => if let Some(c) = c {
                write!(f, "Cat<{}>", c)
            } else {
                write!(f, "None")
            }
            Value::Result(res) => {
                match &**res {
                    Ok(res) => write!(f, "Ok<{}>", res),
                    Err(err) => write!(f, "Err<{}>", err)
                } 
            }
            Self::Set(s) => {
                write!(f, "[ ")?;
                for i in s.iter() {
                    write!(f, "{}, ", i)?;
                }
                write!(f, "]")
            }
        } 
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Void, Value::Void) => true,
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            
            (Value::Float(a), Value::Number(b)) => *a == (*b as f64),
            (Value::Number(a), Value::Float(b)) => (*a as f64) == *b,
            
            (Value::Char(a), Value::Char(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Ref(a), Value::Ref(b)) => a == b,
            
            (Value::Cat(a), Value::Cat(b)) => a == b,
            (Value::Result(a), Value::Result(b)) => a == b,
            (Value::Tuple(a), Value::Tuple(b)) => a == b,
            (Value::Set(a), Value::Set(b)) => a == b,
            
            _ => false,
        }
    }
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Value::Number(a), Value::Number(b)) => a.partial_cmp(b),
            (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
            (Value::Float(a), Value::Number(b)) => a.partial_cmp(&(*b as f64)),
            (Value::Number(a), Value::Float(b)) => (*a as f64).partial_cmp(b),
            (Value::Char(a), Value::Char(b)) => a.partial_cmp(b),
            (Value::Str(a), Value::Str(b)) => a.partial_cmp(b),
            (Value::Bool(a), Value::Bool(b)) => a.partial_cmp(b),
            
            (Value::Tuple(a), Value::Tuple(b)) => a.partial_cmp(b),
            (Value::Set(a), Value::Set(b)) => a.partial_cmp(b),
            (Value::Cat(a), Value::Cat(b)) => a.partial_cmp(b),
            (Value::Result(a), Value::Result(b)) => a.partial_cmp(b),
            
            _ => None, 
        }
    }
}
