use std::ops::{Add, Sub, Mul, Div};

#[derive(Debug, Clone, PartialEq)]
pub enum Value<'a> {
    Void,
    Number(i64),
    Str(&'a str),
    Bool(bool),
    Ref(usize),
}

impl<'a> Add for Value<'a> {
    type Output = Result<Value<'a>, String>;
    fn add(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a + b)),
            _ => Err("Runtime Error: Invalid types for addition (+)".to_string()),
        }
    }
}

impl<'a> Sub for Value<'a> {
    type Output = Result<Value<'a>, String>;
    fn sub(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a - b)),
            _ => Err("Runtime Error: Invalid types for subtraction (-)".to_string()),
        }
    }
}

impl<'a> Mul for Value<'a> {
    type Output = Result<Value<'a>, String>;
    fn mul(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a * b)),
            _ => Err("Runtime Error: Invalid types for multiplication (*)".to_string()),
        }
    }
}

impl<'a> Div for Value<'a> {
    type Output = Result<Value<'a>, String>;
    fn div(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Value::Number(_), Value::Number(0)) => Err("Runtime Error: Division by zero".to_string()),
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a / b)),
            _ => Err("Runtime Error: Invalid types for division (/)".to_string()),
        }
    }
}

impl<'a> Value<'a> {
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Void => false,
            Value::Bool(b) => *b,
            Value::Number(n) => *n != 0,
            Value::Str(s) => !s.is_empty(),
            _ => false,
        }
    }

    pub fn arifm_and(self, rhs: Self) -> Result<Value<'a>, String> {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a & b)),
            _ => Err("Runtime Error: Invalid types for ArifmAnd".to_string()),
        }
    }

    pub fn arifm_or(self, rhs: Self) -> Result<Value<'a>, String> {
        match (self, rhs) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a | b)),
            _ => Err("Runtime Error: Invalid types for ArifmOr".to_string()),
        }
    }

    pub fn pow(self, rhs: Self) -> Result<Value<'a>, String> {
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
