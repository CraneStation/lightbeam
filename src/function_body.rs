use backend::*;
use disassemble::disassemble;
use error::Error;
use wasmparser::{FunctionBody, Operator};
use dynasmrt::{ExecutableBuffer, AssemblyOffset};

pub struct TranslatedFunc {
    buf: ExecutableBuffer,
}

impl TranslatedFunc {
    // Assume signature is (i32, i32) -> i32 for now.
    // TODO: Handle generic signatures.
    pub fn execute(&self, a: usize, b: usize) -> usize {
        use std::mem;

        let start_buf = self.buf.ptr(AssemblyOffset(0));

        unsafe {
            let func = mem::transmute::<_, extern "sysv64" fn(usize, usize) -> usize>(start_buf);
            func(a, b)
        }
    }
}

pub fn translate(body: &FunctionBody) -> Result<TranslatedFunc, Error> {
    let locals = body.get_locals_reader()?;

    // Assume signature is (i32, i32) -> i32 for now.
    // TODO: Use a real signature
    const ARG_COUNT: u32 = 2;

    let mut framesize = ARG_COUNT;
    for local in locals {
        let (count, _ty) = local?;
        framesize += count;
    }

    let mut ops = dynasmrt::x64::Assembler::new().unwrap();
    let mut ctx = Context::new();
    let operators = body.get_operators_reader()?;

    prologue(&mut ctx, &mut ops, framesize);

    for arg_pos in 0..ARG_COUNT {
        copy_incoming_arg(&mut ctx, &mut ops, arg_pos);
    }

    for op in operators {
        match op? {
            Operator::I32Add => {
                add_i32(&mut ctx, &mut ops);
            }
            Operator::GetLocal { local_index } => {
                get_local_i32(&mut ctx, &mut ops, local_index);
            }
            Operator::End => {
                // TODO: This is super naive and makes a lot of unfounded assumptions 
                // but will for the start.
                prepare_return_value(&mut ctx, &mut ops);
            }
            _ => {
                unsupported_opcode(&mut ops);
            }
        }
    }
    epilogue(&mut ctx, &mut ops);

    let output = ops
        .finalize()
        .map_err(|_asm| Error::Assembler("assembler error".to_owned()))?;

    // TODO: Do something with the output.
    disassemble(&output)?;

    Ok(TranslatedFunc {
        buf: output,
    })
}
