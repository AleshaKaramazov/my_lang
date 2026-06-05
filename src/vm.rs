use crate::{op::Op, value::Value};

pub struct VM<'a> {
    stack: Vec<Value<'a>>,
    frame: Vec<Value<'a>>,
}

impl<'a> VM<'a> {
    pub fn new() -> Self {
        Self {
            stack: Vec::with_capacity(32),
            frame: Vec::with_capacity(32),
        }
    }

    #[inline(always)]
    pub fn step(&mut self, op: &Op<'a>, ip: &mut usize) -> Result<(), String> {
        match *op {
            Op::PushStr(s) => self.stack.push(Value::Str(s)),
            Op::PushNumber(n) => self.stack.push(Value::Number(n)),
            Op::PushBool(b) => self.stack.push(Value::Bool(b)),
            Op::PushRef(r) => self.stack.push(Value::Ref(r)),
            Op::PushVoid => self.stack.push(Value::Void),
            Op::Pop => {
                self.stack.pop().ok_or_else(|| "VM Error: Pop from empty stack".to_string())?;
            }
            Op::Plus | Op::Sub | Op::Mult | Op::Div | Op::Pow | Op::ArifmAnd | Op::ArifmOr => {
                let right = self.stack.pop().ok_or("VM Error: Stack underflow")?;
                let left = self.stack.pop().ok_or("VM Error: Stack underflow")?;
                let result = match *op {
                    Op::Plus => (left + right)?,
                    Op::Sub => (left - right)?,
                    Op::Mult => (left * right)?,
                    Op::Div => (left / right)?,
                    Op::Pow => left.pow(right)?,
                    Op::ArifmAnd => left.arifm_and(right)?,
                    Op::ArifmOr => left.arifm_or(right)?,
                    _ => unreachable!(),
                };
                self.stack.push(result);
            }
            Op::Swap => {
                let len = self.stack.len();
                if len < 2 {
                    return Err("VM Error: Stack underflow (Swap)".to_string());
                }
                self.stack.swap(len - 1, len - 2);
            }
            Op::Dup => {
                let val = self.stack.last().ok_or("VM Error: Stack underflow on Dup")?.clone();
                self.stack.push(val);
            }
            Op::Not => {
                let val = self.stack.pop().ok_or("VM Error: Stack underflow")?;
                self.stack.push(Value::Bool(!val.is_truthy()));
            }
            Op::Jump(target) => {
                *ip = target;
                return Ok(()); 
            }
            Op::JumpIfFalse(target) => {
                let val = self.stack.pop().ok_or("VM Error: Stack underflow")?;
                if !val.is_truthy() {
                    *ip = target;
                    return Ok(());
                }
            }
            Op::JumpIfTrue(target) => {
                let val = self.stack.pop().ok_or("VM Error: Stack underflow")?;
                if val.is_truthy() {
                    *ip = target;
                    return Ok(());
                }
            }
            Op::StoreLocal(idx) => {
                let value = self.stack.pop().ok_or_else(|| "VM Error: No value for StoreLocal".to_string())?;
                if idx >= self.frame.len() {
                    self.frame.resize(idx + 1, Value::Void);
                }
                self.frame[idx] = value;
            }
            Op::LoadLocal(idx) => {
                if idx >= self.frame.len() {
                    return Err(format!("VM Error: Uninitialized frame slot {}", idx));
                }
                self.stack.push(self.frame[idx].clone());
            }
            Op::CallFunc(n) => {
                self.call_function(n)?;
            }
        }
        *ip += 1;
        Ok(())
    }

    fn call_function(&mut self, n: usize) -> Result<(), String> {
        let func_val = self.stack.pop().ok_or_else(|| "VM Error: Missing function name".to_string())?;
        let func_name = match func_val {
            Value::Str(s) => s,
            _ => return Err("VM Error: Function name must be a string".to_string()),
        };

        let mut args = Vec::with_capacity(n);
        for _ in 0..n {
            args.push(self.stack.pop().ok_or_else(|| "VM Error: Missing argument for CallFunc".to_string())?);
        }
        args.reverse();

        match func_name {
            "len" => {
                let res = match args[0] {
                    Value::Str(s) => Value::Number(s.chars().count() as i64),
                    Value::Ref(idx) => match self.frame[idx] {
                        Value::Str(s) => Value::Number(s.chars().count() as i64),
                        _ => return Err("can't get len".to_string()),
                    }
                    _ => return Err("can't get len".to_string()),
                };
                self.stack.push(res);
            }
            "writeln" => {
                print!("WRITEFUNC: ");
                for (i, arg) in args.iter().enumerate() {
                    match arg {
                        Value::Number(v) => print!("{}", v),
                        Value::Str(v) => print!("{}", v),
                        Value::Bool(v) => print!("{}", v),
                        Value::Void => print!("()"),
                        _ => {},
                    }
                    if i < args.len() - 1 {
                        print!(" ");
                    }
                }
                println!();
                self.stack.push(Value::Void);
            }
            _ => return Err(format!("VM Error: Unknown function '{}'", func_name)),
        }
        Ok(())
    }

    pub fn run(&mut self, code: &[Op<'a>]) -> Result<(), String> {
        let mut ip = 0;
        while ip < code.len() {
            self.step(&code[ip], &mut ip)?;
        }
        Ok(())
    }
}
