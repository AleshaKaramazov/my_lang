

#[derive(Debug, Clone, Copy)]
pub enum Op<'a> {
    PushStr(&'a str),
    PushNumber(i64),
    PushBool(bool),
    PushVoid,
    Pop,
    Dup,
    Swap,
    StoreLocal(usize),
    LoadLocal(usize),
    CallFunc(usize),
    JumpIfFalse(usize),
    Jump(usize),
    JumpIfTrue(usize),
    ArifmOr,
    ArifmAnd,
    Not,
    Plus,
    Sub,
    Mult,
    Pow,
    Div,
}
