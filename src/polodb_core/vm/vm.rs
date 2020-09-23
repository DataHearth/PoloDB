/*
 * Copyright (c) 2020 Vincent Chan
 *
 * This program is free software; you can redistribute it and/or modify it under
 * the terms of the GNU Lesser General Public License as published by the Free Software
 * Foundation; either version 3, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE.  See the GNU Lesser General Public License for more
 * details.
 *
 * You should have received a copy of the GNU Lesser General Public License along with
 * this program.  If not, see <http://www.gnu.org/licenses/>.
 */
use std::vec::Vec;
use super::vm_code::VmCode;
use super::subprogram::SubProgram;
use crate::bson::Value;

const STACK_SIZE: usize = 256;

#[repr(i8)]
pub enum VmState {
    Reject = -1,
    Init = 0,
    Running = 1,
    Resolve = 2,
}

pub struct VM {
    state: VmState,
    stack: Vec<Value>,
    program: Box<SubProgram>,
}

impl VM {

    pub(crate) fn new(program: Box<SubProgram>) -> VM {
        let mut stack = Vec::new();
        stack.resize(STACK_SIZE, Value::Null);
        VM {
            state: VmState::Init,
            stack,
            program,
        }
    }

    pub(crate) fn execute(&mut self) {
        // let pc: *mut u8 = self.pro
        unsafe {
            let mut pc: *const u8 = self.program.instructions.as_ptr();
            let mut st: *mut Value = self.stack.as_mut_ptr();
            loop {
                let op: VmCode = (pc.cast() as *const VmCode).read();
                pc = pc.add(1);

                match op {
                    VmCode::PushNull => {
                        st.write(Value::Null);
                        st = st.add(1);
                        pc = pc.add(1);
                    }

                    VmCode::PushI32 => {
                        let mut buffer: [u8; 4] = [0; 4];
                        pc.copy_to(buffer.as_mut_ptr(), 4);
                        let num = i32::from_be_bytes(buffer);
                        st.write(Value::Int(num as i64));
                        st = st.add(1);
                        pc = pc.add(4);
                    }

                    VmCode::PushI64 => {
                        let mut buffer: [u8; 8] = [0; 8];
                        pc.copy_to(buffer.as_mut_ptr(), 8);
                        let num = i64::from_be_bytes(buffer);
                        st.write(Value::Int(num));
                        st = st.add(1);
                        pc = pc.add(8);
                    }

                    VmCode::PushTrue => {
                        st.write(Value::Boolean(true));
                        st = st.add(1);
                    }

                    VmCode::PushFalse => {
                        st.write(Value::Boolean(false));
                        st = st.add(1);
                    }

                    VmCode::PushBool => {
                        let value = pc.read() != 0;
                        st.write(Value::Boolean(value));
                        st = st.add(1);
                        pc = pc.add(1);
                    }

                    VmCode::Pop => {
                        st = st.sub(1);
                    }

                    VmCode::CreateCollection => {
                        let n1 = st.sub(1);
                        let option = n1.read();
                        let n2 = st.sub(2);
                        let name = n2.read();
                        st = n2;

                        println!("create collection: {}, {}", name, option)
                    }

                    // VmCode::AddI32 => {
                    //     let to_add = inst.op1 as i32;
                    //     match self.stack[self.st - 1] {
                    //         Value::I32(current) =>
                    //             self.state[self.st - 1] = Value::I32(current + to_add),
                    //
                    //         _ => ()
                    //
                    //     }
                    // }

                    VmCode::Resolve => {
                        self.state = VmState::Resolve;
                        break
                    }

                    VmCode::Reject => {
                        self.state = VmState::Reject;
                        break
                    }
                }
            }
        }
    }

}
