mod lexer;
mod compiler;
mod op;
mod consts;
mod vm;
mod value;
mod vm_run_func;
mod types;
mod file;
mod errors;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = if args.is_empty() {
        println!("no input file");
        return;
    } 
    else if let Ok(text) = std::fs::read_to_string(&args[0]) {
        text
    } else {
        println!("error reading : {}", args[0]);
        return
    };
    let q = compiler::Compiler::new(&code);
    match q.compile() {
        Ok(code) => {
            let mut vm = vm::VM::new();
            if let Err(e) = vm.run(&code) {
                println!("error: {:?}", e)
            }
        }
        Err(e) => println!("{:?}", e)
    }
}
