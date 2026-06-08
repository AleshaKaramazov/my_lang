use std::ops::{Add, Sub, Mul, Div};
use std::cmp::Ordering;
use std::vec::IntoIter;

use crate::consts;
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
    Fn(usize),
    Set(Vec<Value>),
    Result(Box<Result<Value, Value>>),
    Cat(Option<Box<Value>>),
    File(FileHandler),
    Float(f64),
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
    Range(Range)
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

    pub fn eval_str(&self) -> Result<&str, String> {
        match self {
            Self::Str(s) => Ok(s),
            unk => Err(format!("can't eval Str, from: {}", unk))
        }
    }

    pub fn open_file(filename: &str) -> Self {
        Value::Result(Box::new(
            FileHandler::new_file(filename)
                .map(|x| Value::File(x)).map_err(|x| Value::Str(x))
        ))
    }

    pub fn new_file(filename: &str, opt: i64) -> Result<Self, String> {
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

    pub fn next(&mut self) -> Result<Option<Value>, String> {
        match self {
            Value::Iter(i) => Ok(i.next()),
            oth => Err(format!("try to iter: {}", oth))
        }
    }

    pub fn expect_number(&self) -> Result<i64, String> {
        match self {
            Self::Number(i) => Ok(*i),
            _ => Err(format!("Can't eval number from: {}", self))
        }
    }

    pub fn make_range(start: Value, end: Value, incl: bool) -> Result<Value, String> {
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

    pub fn make_iter(self) -> Result<Value, String> {
        let val = match self {
            Self::Str(s) => {
                let iter = s.chars().collect::<Vec<char>>().into_iter();
                Value::Iter(Iterator::String(iter))
            }
            Self::Set(i) => Value::Iter(Iterator::Set(i.into_iter())),
            Self::Range(range) => Value::Iter(Iterator::Range(range)),
            _ => return Err(format!("Can't eval Iterator from: {}", self)),
        };

        Ok(val)
    }

    pub fn set_index(&mut self, index: Self, to_set: Value) -> Result<(), String> {
        let index = index.expect_number()? as usize;
        match self {
            Value::Set(v) => v[index] = to_set,
            _ => return Err("can't eval index".to_string())
        }
        Ok(())
    }

    pub fn load_dyap(&self, start: usize, end: usize) -> Result<Value, String> {
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
            _ => return Err(format!("can't get dyapazone from: {}", self)),
        };
        Ok(res)
    }

    pub fn set_index_deep(&mut self, index: Vec<Self>, to_set: Value) -> Result<(), String> {
        let mut current = self;

        for i in 0..index.len() - 1 {
            let idx = index[i].expect_number()? as usize;
            match current {
                Value::Set(v) => {
                    current = &mut v[idx];
                }
                _ => return Err("Cannot index into a non-set value".to_string()),
            }
        }

        let last_idx = index.last().unwrap().expect_number()? as usize;
        match current {
            Value::Set(v) => {
                v[last_idx] = to_set;
            }
            _ => return Err("Cannot assign: target is not a set".to_string()),
        }

        Ok(())
    }

    pub fn load_index_deep(&self, index: Vec<Self>) -> Result<Value, String> {
        let mut current = self.clone();

        for i in index.iter() {
            current = current.load_index(&i)?.clone();
        }
        Ok(current.clone())
    }

    
    pub fn load_index(&self, index: &Self) -> Result<Value, String> {
        let val = match index {
            Value::Range(r) => {
                if r.start < 0 || r.end < 0 || r.step < 0 || r.start > r.end {
                    return Err("now we can't handle that type of dyapazone index".to_string())
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
                    _ => return Err("can't eval index".to_string())
                }
            }
            _ => return Err("can't eval index".to_string())
        };
        Ok(val)
    }

    pub fn arifm_and(self, rhs: Self) -> Result<Value, String> {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a & b)),
            _ => Err("Runtime Error: Invalid types for ArifmAnd".to_string()),
        }
    }

    pub fn arifm_or(self, rhs: Self) -> Result<Value, String> {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a | b)),
            _ => Err("Runtime Error: Invalid types for ArifmOr".to_string()),
        }
    }

    pub fn arifm_mod(self, rhs: Self) -> Result<Value, String> {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a % b)),
            _ => Err("Runtime Error: Invalid types for Mod".to_string()),
        }
    }

    pub fn pow(self, rhs: Self) -> Result<Value, String> {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => {
                if b < 0 {
                    Err("Runtime Error: Negative exponent is not supported for integers".to_string())
                } else {
                    Ok(Value::Number(a.pow(b as u32)))
                }
            }
            _ => Err("Runtime Error: Invalid types for exponentiation (pow)".to_string()),
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::File(file) => write!(f, "{}", file),
            Self::Float(fl) => write!(f, "{}", fl),
            Self::Void => write!(f, "()",),
            Self::Number(n) => write!(f, "{}", n),
            Self::Bool(b) => write!(f, "{}", b),
            Self::Char(c) => write!(f, "{}", c),
            Self::Str(s) => write!(f, "{}", s),
            Self::Range(s) => write!(f, "{}..{}", s.start, s.end),
            Self::Ref(i) => write!(f, "REF<ID: {}>", i),
            Self::Fn(i) => write!(f, "FN<ID: {}>", i),
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

impl<'a> PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Void, Value::Void) => true,
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Char(a), Value::Char(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Ref(a), Value::Ref(b)) => a == b,
            _ => false,
        }
    }
}

impl<'a> PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Value::Number(a), Value::Number(b)) => a.partial_cmp(b),
            (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
            (Value::Float(a), Value::Number(b)) => a.partial_cmp(&(*b as f64)),
            (Value::Number(a), Value::Float(b)) => (*a as f64).partial_cmp(b),
            (Value::Char(a), Value::Char(b)) => a.partial_cmp(b),
            (Value::Str(a), Value::Str(b)) => a.partial_cmp(b),
            (Value::Bool(a), Value::Bool(b)) => a.partial_cmp(b),
            _ => None, 
        }
    }
}

impl<'a> Add for Value {
    type Output = Result<Value, String>;
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
            _ => Err("Runtime Error: Invalid types for addition/concatenation (+)".to_string()),
        }
    }
}

impl<'a> Sub for Value {
    type Output = Result<Value, String>;
    fn sub(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a - b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
            (Value::Float(a), Value::Number(b)) => Ok(Value::Float(a - b as f64)),
            (Value::Number(a), Value::Float(b)) => Ok(Value::Float(a as f64 - b)),
            _ => Err("Runtime Error: Invalid types for subtraction (-)".to_string()),
        }
    }
}

impl<'a> Mul for Value {
    type Output = Result<Value, String>;
    fn mul(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a * b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
            (Value::Float(a), Value::Number(b)) => Ok(Value::Float(a * b as f64)),
            (Value::Number(a), Value::Float(b)) => Ok(Value::Float(a as f64 * b)),
            _ => Err("Runtime Error: Invalid types for multiplication (*)".to_string()),
        }
    }
}

impl<'a> Div for Value {
    type Output = Result<Value, String>;
    fn div(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Value::Number(_), Value::Number(0)) => Err("Runtime Error: Division by zero".to_string()),
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a / b)),
            (Value::Float(_), Value::Float(0.0)) => Err("Runtime Error: Division by zero".to_string()),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
            (Value::Float(a), Value::Number(b)) => Ok(Value::Float(a / b as f64)),
            (Value::Number(a), Value::Float(b)) => Ok(Value::Float(a as f64 / b)),
            _ => Err("Runtime Error: Invalid types for division (/)".to_string()),
        }
    }
}
