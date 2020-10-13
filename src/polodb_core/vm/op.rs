
#[repr(u8)]
#[derive(Copy, Clone)]
#[allow(dead_code)]
pub enum DbOp {
    _EOF = 0,

    // reset the pc to the position of op0
    //
    // 5 bytes
    // op1. location: 4 bytes
    Goto = 1,

    // if r0 is true, jump to location
    //
    // 5 bytes
    // op1. location: 4 bytes
    TrueJump,

    // if r0 is false, jump to location
    //
    // 5 bytes
    // op1. location: 4 bytes
    FalseJump,

    // reset the cursor to the first element
    Rewind,

    // next element of the cursor
    // if no next element, pass
    // otherwise, jump to location
    //
    // push current value to the stack
    //
    // 5 bytes
    // op1. location: 4bytes
    Next,

    // push value to the stack
    //
    // 5 bytes
    // op1. value_index: 4bytes
    PushValue,

    // get the field of top of the stack
    // push the value to the stack
    //
    // if failed, goto op2
    //
    // 9 bytes
    // op1. value_index: 4bytes
    // op2. location: 4bytes
    GetField,

    // increment the field
    // if not exists, set the value
    //
    // throw error if field is null
    //
    // top-1 is the value to push
    // top-2 is the doc to change
    //
    // 5 bytes
    // op1. field_name_index: 4bytes
    IncField,

    // set the value of the field
    //
    // top-1 is the value to push
    // top-2 is the doc to change
    //
    // 5 bytes
    // op1. field_name_index: 4bytes
    SetField,

    Pop,

    // check if top 2 values on the stack are qual
    // the result is stored in r0
    //
    // -1 for not comparable
    // 0 false not equal
    // 1 for equal
    Equal,

    // compare top 2 values on the stack
    //
    // REJECT when not comparable
    // -1 for less
    // 0 for equal
    // 1 for great
    Cmp,

    // open a cursor with op0 as root_pid
    //
    // 5 byes
    // op1. root_id: 4 bytes
    OpenRead,

    // open a cursor with op0 as root_pid
    //
    // 5 byes
    // op1. root_id: 4 bytes
    OpenWrite,

    // Pause the db
    // The top value of the stack
    // is the result
    ResultRow,

    // Close cursor
    Close,

    // Exit
    // Close cursor automatically
    Halt,

}
