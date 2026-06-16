
#[derive(Debug)]
pub enum CompilerError {
    UnexpectedArg,
    UnfindedVar,
    ExpectedToken,
    UnknownFunc,
}

