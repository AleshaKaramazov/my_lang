
pub const ASSIGN_ADD: usize = 0;
pub const ASSIGN_SUB: usize = 1;
pub const ASSIGN_MUL: usize = 2;
pub const ASSIGN_DIV: usize = 3;
pub const ASSIGN_POW: usize = 4;

pub const STOP_FLAG: usize = usize::MAX;

pub const STDOUT:       i64    = 1;
pub const STDERR:       i64    = 2;
pub const READ_FM:      i64    = 0b00000001;
pub const WRITE_FM:     i64    = 0b00000010;
pub const CREATE_FM:    i64    = 0b00000100;
pub const TRUNCATE_FM:  i64    = 0b00001000;
pub const ALL_FLAGS:    i64    = READ_FM | WRITE_FM | CREATE_FM;
pub const READ_AT_ONCE: usize  = const { 1024 * 64 };


pub const FILE_FT:      i64    = 0b00010000;
pub const DIR_FT:       i64    = 0b00100000;
pub const SYMLINK_FT:   i64    = 0b01000000;
pub const EXEC_FT:      i64    = 0b10000000;
pub const UNKNOWN_FT:   i64    = 0;

pub static mut STATE:   i64    = 88172645463325252;
