mod lexer;
mod compiler;
mod op;
mod consts;
mod vm;
mod value;

fn main() {
    let code = r#"
        let q = "hello".len();
    "#;

    let q = compiler::Compiler::new(code);
    match q.compile() {
        Ok(code) => {
            for (pos, op) in code.iter().enumerate() {
                println!("{:>4}) {:?}", pos, op);
            }
            let mut vm = vm::VM::new();
            if let Err(e) = vm.run(&code) {
                println!("error: {}", e)
            }
        }
        Err(e) => println!("{}", e)
    }
    
    println!();
}
