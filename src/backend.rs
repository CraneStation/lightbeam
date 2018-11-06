#![allow(dead_code)] // for now

use dynasmrt::x64::Assembler;
use dynasmrt::DynasmApi;

/// Size of a pointer on the target in bytes.
const WORD_SIZE: u32 = 8;

type GPR = u8;

struct GPRs {
    bits: u16,
}

impl GPRs {
    fn new() -> Self {
        Self { bits: 0 }
    }
}

const RAX: u8 = 0;
const RCX: u8 = 1;
const RDX: u8 = 2;
const RBX: u8 = 3;
const RSP: u8 = 4;
const RBP: u8 = 5;
const RSI: u8 = 6;
const RDI: u8 = 7;
const R8: u8 = 8;
const R9: u8 = 9;
const R10: u8 = 10;
const R11: u8 = 11;
const R12: u8 = 12;
const R13: u8 = 13;
const R14: u8 = 14;
const R15: u8 = 15;

impl GPRs {
    fn take(&mut self) -> GPR {
        let lz = self.bits.trailing_zeros();
        assert!(lz < 32, "ran out of free GPRs");
        self.bits &= !(1 << lz);
        lz as GPR
    }

    fn release(&mut self, gpr: GPR) {
        assert_eq!(
            self.bits & (1 << gpr),
            0,
            "released register was already free"
        );
        self.bits |= 1 << gpr;
    }
}

pub struct Registers {
    scratch_gprs: GPRs,
}

impl Registers {
    pub fn new() -> Self {
        let mut result = Self {
            scratch_gprs: GPRs::new(),
        };
        // Give ourselves a few scratch registers to work with, for now.
        result.release_scratch_gpr(RAX);
        result.release_scratch_gpr(RCX);
        result.release_scratch_gpr(RDX);
        result
    }

    pub fn take_scratch_gpr(&mut self) -> GPR {
        self.scratch_gprs.take()
    }

    pub fn release_scratch_gpr(&mut self, gpr: GPR) {
        self.scratch_gprs.release(gpr);
    }
}

/// Describes location of a argument.
enum ArgLocation {
    /// Argument is passed via some register.
    Reg(GPR),
    /// Value is passed thru the stack.
    Stack(i32),
}

/// Get a location for an argument at the given position.
fn abi_loc_for_arg(pos: u32) -> ArgLocation {
    // TODO: This assumes only system-v calling convention.
    // In system-v calling convention the first 6 arguments are passed via registers. 
    // All rest arguments are passed on the stack.
    const ARGS_IN_GPRS: &'static [GPR] = &[
        RDI,
        RSI,
        RDX,
        RCX,
        R8,
        R9,
    ];

    if let Some(&reg) = ARGS_IN_GPRS.get(pos as usize) {
        ArgLocation::Reg(reg)
    } else {
        let stack_pos = pos - ARGS_IN_GPRS.len() as u32;
        ArgLocation::Stack((stack_pos * WORD_SIZE) as i32)
    }
}

pub struct Context {
    regs: Registers,
    /// Offset from starting value of SP counted in words. Each push and pop 
    /// on the value stack increments or decrements this value by 1 respectively.
    sp_depth: usize,
}

impl Context {
    pub fn new() -> Self {
        Context {
            regs: Registers::new(),
            sp_depth: 0,
        }
    }
}

fn push_i32(ctx: &mut Context, ops: &mut Assembler, gpr: GPR) {
    // For now, do an actual push (and pop below). In the future, we could
    // do on-the-fly register allocation here.
    ctx.sp_depth += 1;
    dynasm!(ops
        ; push Rq(gpr)
    );
    ctx.regs.release_scratch_gpr(gpr);
}

fn pop_i32(ctx: &mut Context, ops: &mut Assembler) -> GPR {
    ctx.sp_depth -= 1;
    let gpr = ctx.regs.take_scratch_gpr();
    dynasm!(ops
        ; pop Rq(gpr)
    );
    gpr
}

pub fn add_i32(ctx: &mut Context, ops: &mut Assembler) {
    let op0 = pop_i32(ctx, ops);
    let op1 = pop_i32(ctx, ops);
    dynasm!(ops
        ; add Rq(op0), Rq(op1)
    );
    push_i32(ctx, ops, op0);
    ctx.regs.release_scratch_gpr(op1);
}

fn sp_relative_offset(ctx: &mut Context, slot_idx: u32) -> i32 {
    ((ctx.sp_depth as i32) + slot_idx as i32) * WORD_SIZE as i32
}

pub fn get_local_i32(ctx: &mut Context, ops: &mut Assembler, local_idx: u32) {
    let gpr = ctx.regs.take_scratch_gpr();
    let offset = sp_relative_offset(ctx, local_idx);
    dynasm!(ops
        ; mov Rq(gpr), [rsp + offset]
    );
    push_i32(ctx, ops, gpr);
}

pub fn store_i32(ctx: &mut Context, ops: &mut Assembler, local_idx: u32) {
    let gpr = pop_i32(ctx, ops);
    let offset = sp_relative_offset(ctx, local_idx);
    dynasm!(ops
        ; mov [rsp + offset], Rq(gpr)
    );
    ctx.regs.release_scratch_gpr(gpr);
}

pub fn prepare_return_value(ctx: &mut Context, ops: &mut Assembler) {
    let ret_gpr = pop_i32(ctx, ops);
    if ret_gpr != RAX {
        dynasm!(ops
            ; mov Rq(RAX), Rq(ret_gpr)
        );
        ctx.regs.release_scratch_gpr(ret_gpr);
    }
}

pub fn copy_incoming_arg(ctx: &mut Context, ops: &mut Assembler, arg_pos: u32) {
    let loc = abi_loc_for_arg(arg_pos);

    // First, ensure the argument is in a register.
    let reg = match loc {
        ArgLocation::Reg(reg) => reg,
        ArgLocation::Stack(offset) => {
            // RAX is always scratch?
            dynasm!(ops
                ; mov Rq(RAX), [rsp + offset]
            );
            RAX
        }
    };

    // And then move a value from a register into local variable area on the stack.
    let offset = sp_relative_offset(ctx, arg_pos);
    dynasm!(ops
        ; mov [rsp + offset], Rq(reg) 
    );
}

pub fn prologue(_ctx: &mut Context, ops: &mut Assembler, stack_slots: u32) {
    let framesize: i32 = stack_slots as i32 * WORD_SIZE as i32;
    dynasm!(ops
        ; push rbp
        ; mov rbp, rsp
        ; sub rsp, framesize
    );
}

pub fn epilogue(_ctx: &mut Context, ops: &mut Assembler) {
    dynasm!(ops
        ; mov rsp, rbp
        ; pop rbp
        ; ret
    );
}

pub fn unsupported_opcode(ops: &mut Assembler) {
    dynasm!(ops
        ; ud2
    );
}
