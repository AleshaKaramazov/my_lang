
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Number,
    Str,
    Bool,
    Char,
    Void,
    File,
    Infer,
    Float,
    Iter(Box<Type>),
    Set(Box<Type>),
    Result(Box<(Type, Type)>),
    Cat(Box<Type>),
}
