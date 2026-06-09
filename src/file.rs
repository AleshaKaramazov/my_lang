use std::{
    cell::RefCell, 
    fs::{File, OpenOptions}, 
    io::{Read, Seek},
    path::{PathBuf}, 
    rc::Rc
};
use crate::{consts, value::Value, errors::VMError};

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
    pub fn new_file(filename: &str) -> Result<Self, VMError> {
        match File::create(filename) {
            Ok(file) => Ok(Self {
                file: Rc::new(RefCell::new(file)),
                path: filename.into()
            }),
            Err(_) => Err(VMError::FileError)
        } 
    }

    pub fn open(filename: &str, opt: i64) -> Result<Self, VMError> {
        let read = consts::READ_FM & opt != 0; 
        let truncate = consts::TRUNCATE_FM & opt != 0;
        let write = consts::WRITE_FM & opt != 0 || truncate;
        let create = consts::CREATE_FM & opt != 0;
        
        match OpenOptions::new()
            .read(read)
            .write(write)
            .create(create)
            .append(!truncate)
            .truncate(truncate)
            .open(filename) {
                Ok(file) => {
                    Ok(Self {
                        file: Rc::new(RefCell::new(file)),
                        path: filename.into()
                    })
                }
                Err(_) => Err(VMError::FileError)
        }
    }
    
    pub fn read<'a>(&mut self) -> Value {
        if let Err(e) = self.file.borrow_mut().seek(std::io::SeekFrom::Start(0)) {
            return Value::Result(Box::new(Err(Value::Str(
                    format!("Error while trying seek the file({}): {}", self.path.display(), e)))))
        }

        let mut buffer = String::new();
        let val = if let Err(e) = self.file.borrow_mut().read_to_string(&mut buffer) {
            Err(format!("Error while trying read the file({}): {}", self.path.display(), e))
        } else {
            Ok(Value::Str(buffer))
        };
        Value::new_control(val)
    }
}

