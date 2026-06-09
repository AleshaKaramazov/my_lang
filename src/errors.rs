
#[derive(Debug)]
pub enum CompilerError {
    UnexpectedArg,
    UnfindedVar,
}

#[derive(Debug)]
pub enum VMError {
    EmptyStack,
    UnExpectedType,
    NotOperation,
    CantIter,
    CantIndex,
    BadOperand,
    ZeroDiv,
    FileError,
    FuncErr,
    NeedMoreArgs,
    BadArgument,
    TooManyArgs,
    WriteError,
    UnknownFunc
}
