use num_enum::TryFromPrimitive;

#[derive(Debug, Eq, PartialEq, TryFromPrimitive, Clone, Copy)]
#[repr(i32)]
pub enum MsgTy {
    Undefined = 0,
    Find = 1,
    FindOne = 2,
    Count = 3,
    Insert = 8,
    Update = 16,
    Delete = 32,
    Version = 128,
    SafelyQuit = 255,
}
