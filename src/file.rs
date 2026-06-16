use std::{
    cell::RefCell, 
    fs::{File, OpenOptions}, 
    path::{PathBuf}, 
    rc::Rc
};

use crate::{consts, value::Value};

#[derive(Debug, Clone)]
pub struct FileHandler {
    pub file: Rc<RefCell<File>>,
    pub path: PathBuf,
}

impl std::fmt::Display for FileHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.path.display())
    } 
}

impl FileHandler {
    pub fn new_file(filename: &str) -> Value {
        let res = match File::create(filename) {
            Ok(file) => Ok(Value::File(Box::new(Self {
                file: Rc::new(RefCell::new(file)),
                path: filename.into()
            }))),
            Err(_) => Err(format!("error with open: {}", filename))
        } ;
        Value::new_control(res)
    }

    pub fn open(filename: &str, opt: i64) -> Value {
        let read = consts::READ_FM & opt != 0; 
        let truncate = consts::TRUNCATE_FM & opt != 0;
        let write = consts::WRITE_FM & opt != 0 || truncate;
        let create = consts::CREATE_FM & opt != 0;
        
        let res = match OpenOptions::new()
            .read(read)
            .write(write)
            .create(create)
            .append(!truncate)
            .truncate(truncate)
            .open(filename) {
                Ok(file) => {
                    Ok(Value::File(Box::new(Self {
                        file: Rc::new(RefCell::new(file)),
                        path: filename.into()
                    })))
                }
                Err(_) => Err(format!("error with open: {}", filename))
        };
        Value::new_control(res)
    }
    
}

