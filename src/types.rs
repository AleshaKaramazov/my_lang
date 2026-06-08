
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Number,
    Str,
    Bool,
    Char,
    Void,
    File,
    Float,
    Set(Box<Type>),
    Result(Box<(Type, Type)>),
    Cat(Box<Type>),
}
