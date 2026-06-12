
#[derive(Debug)]
pub enum CompilerError {
    UnexpectedArg,
    UnfindedVar,
    ExpectedToken,
    UnknownFunc,
    UnExpectedType,
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
}
