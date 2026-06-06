

#[derive(Debug, Clone, Copy)]
pub enum Op<'a> {
    PushStr(&'a str),
    PushChar(char),
    PushNumber(i64),
    PushRef(usize),
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

    MakeIter,
    IterNext(usize),

    Equal,
    Greater,
    Less,
    GreaterEq,
    LessEq,
}
