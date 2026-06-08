mod lexer;
mod compiler;
mod op;
mod consts;
mod vm;
mod value;
mod vm_run_func;
mod types;
mod file;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = if let Ok(text) = std::fs::read_to_string(&args[0]) {
        text
    } else {
        println!("error reading : {}", args[1]);
        return
    };
    let q = compiler::Compiler::new(&code);
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
