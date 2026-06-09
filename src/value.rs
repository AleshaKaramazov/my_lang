use std::ops::{Add, Sub, Mul, Div};
use std::cmp::Ordering;
use std::vec::IntoIter;

use crate::consts;
use crate::errors::VMError;
use crate::file::FileHandler;
use crate::types::Type;

#[derive(Debug, Clone)]
pub enum Value {
    Void,
    Number(i64),
    Str(String),
    Char(char),
    Bool(bool),
    Ref(usize),
    Range(Range),
    Iter(Iterator),
    Fn(usize, usize),
    Set(Vec<Value>),
    Result(Box<Result<Value, Value>>),
    Cat(Option<Box<Value>>),
    File(FileHandler),
    Float(f64),
    Tuple(Vec<Value>),
}

#[derive(Debug, Clone)]
pub struct Range {
    pub start: i64,
    pub end: i64,
    pub step: i64,
}

#[derive(Debug, Clone)]
pub enum Iterator {
    String(IntoIter<char>),
    Set(IntoIter<Value>),
    Range(Range),
    Enumerate(Box<Iterator>, i64),
}

impl Iterator {
    fn next(&mut self) -> Option<Value> {
        match self {
            Self::Set(i) => i.next(),
            Self::String(s) => s.next().map(|x| Value::Char(x)),
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
                    
                    Some(Value::Tuple(vec![Value::Number(current_idx), val]))
                } else {
                    None
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
                .create(true).append(true).open(filepath) {
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

impl<'a> Value {
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
                .map(|x| Value::File(x)).map_err(|_| Value::Str(format!("error with open file")))
        ))
    }

    pub fn new_file(filename: &str, opt: i64) -> Result<Self, VMError> {
        Ok(Value::File(FileHandler::open(filename, opt)?))
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
                    Ok(val) => val.this_type(&(*inner).0),
                    Err(val) => val.this_type(&(*inner).1),
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
        Ok(Value::Range(Range { start, end, step: if start > end {-1} else {1} }))
    }

    pub fn new_control(res: Result<Value, String>) -> Value {
        Value::Result(Box::new(match res {
            Ok(res) => Ok(res),
            Err(er) => Err(Value::Str(er))
        }))
    }

    pub fn make_iter(self) -> Result<Iterator, VMError> {
        let val = match self {
            Self::Str(s) => {
                let iter = s.chars().collect::<Vec<char>>().into_iter();
                Iterator::String(iter)
            }
            Self::Set(i) => Iterator::Set(i.into_iter()),
            Self::Range(range) => Iterator::Range(range),
            _ => return Err(VMError::UnExpectedType),
        };

        Ok(val)
    }

    pub fn set_index(&mut self, index: Self, to_set: Value) -> Result<(), VMError> {
        let index = index.expect_number()? as usize;
        match self {
            Value::Set(v) => v[index] = to_set,
            _ => return Err(VMError::CantIndex)
        }
        Ok(())
    }

    pub fn load_dyap(&self, start: usize, end: usize) -> Result<Value, VMError> {
        let res = match self {
            Value::Set(s) => {
                let end = if end > s.len() {s.len()} else {end};
                Value::Set(s[start..end].to_vec())
            }
            Value::Str(s) => {
                let count = s.chars().count();
                let end = if end > count {count} else {end};
                Value::Str(s[start..end].to_string())
            }
            _ => return Err(VMError::UnExpectedType),
        };
        Ok(res)
    }

    pub fn set_index_deep(&mut self, index: Vec<Self>, to_set: Value) -> Result<(), VMError> {
        let mut current = self;

        for i in 0..index.len() - 1 {
            let idx = index[i].expect_number()? as usize;
            match current {
                Value::Set(v) => {
                    current = &mut v[idx];
                }
                _ => return Err(VMError::CantIndex),
            }
        }

        let last_idx = index.last().unwrap().expect_number()? as usize;
        match current {
            Value::Set(v) => {
                v[last_idx] = to_set;
            }
            _ => return Err(VMError::CantIndex),
        }

        Ok(())
    }

    pub fn load_index_deep(&self, index: Vec<Self>) -> Result<Value, VMError> {
        let mut current = self.clone();

        for i in index.iter() {
            current = current.load_index(&i)?.clone();
        }
        Ok(current.clone())
    }

    
    pub fn load_index(&self, index: &Self) -> Result<Value, VMError> {
        let val = match index {
            Value::Range(r) => {
                if r.start < 0 || r.end < 0 || r.step < 0 || r.start > r.end {
                    return Err(VMError::CantIndex)
                } 
                self.load_dyap(r.start as usize, r.end as usize)?
            } 
            Value::Number(i) => {
                match self {
                    Value::Set(v) =>
                        v[i.rem_euclid(v.len() as i64) as usize].clone(),
                    Value::Str(s) => {
                        let index = i.rem_euclid(s.chars().count() as i64) as usize;
                        s.chars().nth(index).map(|x| Value::Char(x)).unwrap()
                    }
                    _ => return Err(VMError::CantIndex)
                }
            }
            _ => return Err(VMError::CantIndex)
        };
        Ok(val)
    }

    pub fn arifm_and(self, rhs: Self) -> Result<Value, VMError> {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a & b)),
            _ => Err(VMError::NotOperation),
        }
    }

    pub fn arifm_or(self, rhs: Self) -> Result<Value, VMError> {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a | b)),
            _ => Err(VMError::NotOperation),
        }
    }

    pub fn arifm_mod(self, rhs: Self) -> Result<Value, VMError> {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a % b)),
            _ => Err(VMError::NotOperation),
        }
    }

    pub fn pow(self, rhs: Self) -> Result<Value, VMError> {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => {
                if b < 0 {
                    Err(VMError::BadOperand)
                } else {
                    Ok(Value::Number(a.pow(b as u32)))
                }
            }
            _ => Err(VMError::NotOperation),
        }
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
            Self::Range(s) => write!(f, "{}..{}", s.start, s.end),
            Self::Ref(i) => write!(f, "REF<ID: {}>", i),
            Self::Fn(i, _) => write!(f, "FN<ID: {}>", i),
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
                for i in s {
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

impl<'a> Add for Value {
    type Output = Result<Value, VMError>;
    fn add(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a + b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
            (Value::Float(a), Value::Number(b)) => Ok(Value::Float(a + b as f64)),
            (Value::Number(a), Value::Float(b)) => Ok(Value::Float(a as f64 + b)),
            (Value::Str(a), b) => {
                Ok(Value::Str(format!("{}{}", a, b)))
            },
            (b, Value::Str(a)) => {
                Ok(Value::Str(format!("{}{}", b, a)))
            },
            (Value::Char(a), b) => {
                Ok(Value::Str(format!("{}{}", a, b)))
            },
            (b, Value::Char(a)) => {
                Ok(Value::Str(format!("{}{}", b, a)))
            }
            _ => Err(VMError::BadOperand),
        }
    }
}

impl<'a> Sub for Value {
    type Output = Result<Value, VMError>;
    fn sub(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a - b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
            (Value::Float(a), Value::Number(b)) => Ok(Value::Float(a - b as f64)),
            (Value::Number(a), Value::Float(b)) => Ok(Value::Float(a as f64 - b)),
            _ => Err(VMError::BadOperand),
        }
    }
}

impl<'a> Mul for Value {
    type Output = Result<Value, VMError>;
    fn mul(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a * b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
            (Value::Float(a), Value::Number(b)) => Ok(Value::Float(a * b as f64)),
            (Value::Number(a), Value::Float(b)) => Ok(Value::Float(a as f64 * b)),
            _ => Err(VMError::BadOperand),
        }
    }
}

impl<'a> Div for Value {
    type Output = Result<Value, VMError>;
    fn div(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Value::Number(_), Value::Number(0)) => Err(VMError::ZeroDiv),
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a / b)),
            (Value::Float(_), Value::Float(0.0)) => Err(VMError::ZeroDiv),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
            (Value::Float(a), Value::Number(b)) => Ok(Value::Float(a / b as f64)),
            (Value::Number(a), Value::Float(b)) => Ok(Value::Float(a as f64 / b)),
            _ => Err(VMError::BadOperand),
        }
    }
}
