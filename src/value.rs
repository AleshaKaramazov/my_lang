use std::ops::{Add, Sub, Mul, Div};
use std::cmp::Ordering;
use std::vec::IntoIter;

#[derive(Debug, Clone)]
pub enum Value {
    Void,
    Number(i64),
    Str(String),
    Char(char),
    Bool(bool),
    Ref(usize),
    Iter(Iterator),
}

#[derive(Debug, Clone)]
pub enum Iterator {
    String(IntoIter<char>),
}

impl Iterator {
    fn next(&mut self) -> Option<Value> {
        match self {
            Self::String(s) => s.next().map(|x| Value::Char(x)),
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

    pub fn next(&mut self) -> Option<Value> {
        match self {
            Value::Iter(i) => i.next(),
            _ => None, 
        }
    }

    pub fn make_iter(self) -> Result<Value, String> {
        let val = match self {
            Self::Str(s) => {
                let iter = s.chars().collect::<Vec<char>>().into_iter();
                Value::Iter(Iterator::String(iter))
            }
            _ => return Err(format!("Can't eval Iterator from: {}", self)),
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
            Self::Void => write!(f, "()",),
            Self::Number(n) => write!(f, "{}", n),
            Self::Bool(b) => write!(f, "{}", b),
            Self::Char(c) => write!(f, "{}", c),
            Self::Str(s) => write!(f, "{}", s),
            Self::Ref(i) => write!(f, "REF<ID: {}>", i),
            Self::Iter(i) => write!(f, "Iter<{:?}>", i)
        } 
    }
}

impl<'a> PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Void, Value::Void) => true,
            (Value::Number(a), Value::Number(b)) => a == b,
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
            _ => Err("Runtime Error: Invalid types for subtraction (-)".to_string()),
        }
    }
}

impl<'a> Mul for Value {
    type Output = Result<Value, String>;
    fn mul(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a * b)),
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
            _ => Err("Runtime Error: Invalid types for division (/)".to_string()),
        }
    }
}
