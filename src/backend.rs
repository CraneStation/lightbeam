#![allow(dead_code)] // for now

use dynasmrt::x64::Assembler;
use dynasmrt::{AssemblyOffset, DynamicLabel, DynasmApi, DynasmLabelApi, ExecutableBuffer};
use error::Error;
use std::iter;

/// Size of a pointer on the target in bytes.
const WORD_SIZE: u32 = 8;

type GPR = u8;

#[derive(Copy, Clone)]
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
const NUM_GPRS: u8 = 16;

impl GPRs {
    fn take(&mut self) -> GPR {
        let lz = self.bits.trailing_zeros();
        assert!(lz < 16, "ran out of free GPRs");
        let gpr = lz as GPR;
        self.mark_used(gpr);
        gpr
    }

    fn mark_used(&mut self, gpr: GPR) {
        self.bits &= !(1 << gpr as u16);
    }

    fn release(&mut self, gpr: GPR) {
        assert!(!self.is_free(gpr), "released register was already free",);
        self.bits |= 1 << gpr;
    }

    fn free_count(&self) -> u32 {
        self.bits.count_ones()
    }

    fn is_free(&self, gpr: GPR) -> bool {
        (self.bits & (1 << gpr)) != 0
    }
}

#[derive(Copy, Clone)]
pub struct Registers {
    scratch: GPRs,
}

impl Default for Registers {
    fn default() -> Self {
        Self::new()
    }
}

impl Registers {
    pub fn new() -> Self {
        let mut result = Self {
            scratch: GPRs::new(),
        };
        // Give ourselves a few scratch registers to work with, for now.
        for &scratch in SCRATCH_REGS {
            result.release_scratch_gpr(scratch);
        }

        result
    }

    // TODO: Add function that takes a scratch register if possible
    //       but otherwise gives a fresh stack location.
    pub fn take_scratch_gpr(&mut self) -> GPR {
        self.scratch.take()
    }

    pub fn release_scratch_gpr(&mut self, gpr: GPR) {
        self.scratch.release(gpr);
    }

    pub fn is_free(&self, gpr: GPR) -> bool {
        self.scratch.is_free(gpr)
    }

    pub fn free_scratch(&self) -> u32 {
        self.scratch.free_count()
    }
}

/// Describes location of a value.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ValueLocation {
    /// Value exists in a register.
    Reg(GPR),
    /// Value exists on the stack. This is an offset relative to the
    /// first local, and so will have to be adjusted with `adjusted_offset`
    /// before reading (as RSP may have been changed by `push`/`pop`).
    Stack(i32),
}

// TODO: This assumes only system-v calling convention.
// In system-v calling convention the first 6 arguments are passed via registers.
// All rest arguments are passed on the stack.
const ARGS_IN_GPRS: &[GPR] = &[RDI, RSI, RDX, RCX, R8, R9];
// RAX is reserved for return values. In the future we want a system to allow
// use of specific registers by saving/restoring them. This would allow using
// RAX as a scratch register when we're not calling a function, and would also
// allow us to call instructions that require specific registers.
//
// List of scratch registers taken from https://wiki.osdev.org/System_V_ABI
const SCRATCH_REGS: &[GPR] = &[R10, R11];

/// Records data about the function.
struct FuncDef {
    /// Offset to the start of the function. None, until the exact offset is known.
    ///
    /// Used to calculate the address for calling this function.
    /// TODO: This field will not be needed if dynasm gain ability to return `AssemblyOffset` for the
    /// defined labels.
    offset: Option<AssemblyOffset>,
    /// Dynamic label can be used to designate target of calls
    /// before knowning the actual address of the function.
    label: DynamicLabel,
}

impl FuncDef {
    fn new(asm: &mut Assembler) -> FuncDef {
        FuncDef {
            offset: None,
            label: asm.new_dynamic_label(),
        }
    }
}

pub struct CodeGenSession {
    assembler: Assembler,
    func_defs: Vec<FuncDef>,
}

impl CodeGenSession {
    pub fn new(func_count: u32) -> Self {
        let mut assembler = Assembler::new().unwrap();
        let func_defs = iter::repeat_with(|| FuncDef::new(&mut assembler))
            .take(func_count as usize)
            .collect::<Vec<_>>();

        CodeGenSession {
            assembler,
            func_defs,
        }
    }

    pub fn new_context(&mut self, func_idx: u32) -> Context {
        {
            let func_start = &mut self.func_defs[func_idx as usize];

            // At this point we know the exact start address of this function. Save it
            // and define dynamic label at this location.
            func_start.offset = Some(self.assembler.offset());
            self.assembler.dynamic_label(func_start.label);
        }

        Context {
            asm: &mut self.assembler,
            func_starts: &self.func_starts,
            block_state: Default::default(),
            locals: Default::default(),
        }
    }

    pub fn into_translated_code_section(self) -> Result<TranslatedCodeSection, Error> {
        let exec_buf = self
            .assembler
            .finalize()
            .map_err(|_asm| Error::Assembler("assembler error".to_owned()))?;
        let func_defs = self
            .func_defs
            .iter()
            .map(|FuncDef { offset, .. }| offset.unwrap())
            .collect::<Vec<_>>();
        Ok(TranslatedCodeSection {
            exec_buf,
            func_defs,
        })
    }
}

#[derive(Debug)]
pub struct TranslatedCodeSection {
    exec_buf: ExecutableBuffer,
    func_defs: Vec<AssemblyOffset>,
}

impl TranslatedCodeSection {
    pub fn func_start(&self, idx: usize) -> *const u8 {
        let offset = self.func_defs[idx];
        self.exec_buf.ptr(offset)
    }

    pub fn disassemble(&self) {
        ::disassemble::disassemble(&*self.exec_buf).unwrap();
    }
}

// TODO: Immediates? We could implement on-the-fly const folding
#[derive(Copy, Clone)]
enum Value {
    Local(u32),
    Temp(GPR),
}

impl Value {
    fn location(&self, locals: &Locals) -> ValueLocation {
        match *self {
            Value::Local(loc) => local_location(locals, loc),
            Value::Temp(reg) => ValueLocation::Reg(reg),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum StackValue {
    Local(u32),
    Temp(GPR),
    Pop,
}

impl StackValue {
    fn location(&self, locals: &Locals) -> Option<ValueLocation> {
        match *self {
            StackValue::Local(loc) => Some(local_location(locals, loc)),
            StackValue::Temp(reg) => Some(ValueLocation::Reg(reg)),
            StackValue::Pop => None,
        }
    }
}

#[derive(Default)]
struct Locals {
    // TODO: Use `ArrayVec` since we have a hard maximum (the number of registers)
    locs: Vec<ValueLocation>,
}

#[derive(Default, Clone)]
pub struct BlockState {
    stack: Stack,
    depth: StackDepth,
    regs: Registers,
}

fn adjusted_offset(ctx: &mut Context, offset: i32) -> i32 {
    (ctx.block_state.depth.0 * WORD_SIZE) as i32 + offset
}

fn local_location(locals: &Locals, index: u32) -> ValueLocation {
    locals
        .locs
        .get(index as usize)
        .cloned()
        .unwrap_or(ValueLocation::Stack(
            (index.saturating_sub(ARGS_IN_GPRS.len() as u32) * WORD_SIZE) as _,
        ))
}

type Stack = Vec<StackValue>;

pub struct Context<'a> {
    asm: &'a mut Assembler,
    func_starts: &'a Vec<(Option<AssemblyOffset>, DynamicLabel)>,
    /// Each push and pop on the value stack increments or decrements this value by 1 respectively.
    block_state: BlockState,
    locals: Locals,
}

impl<'a> Context<'a> {}

/// Label in code.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Label(DynamicLabel);

/// Create a new undefined label.
pub fn create_label(ctx: &mut Context) -> Label {
    Label(ctx.asm.new_dynamic_label())
}

/// Define the given label at the current position.
///
/// Multiple labels can be defined at the same position. However, a label
/// can be defined only once.
pub fn define_label(ctx: &mut Context, label: Label) {
    ctx.asm.dynamic_label(label.0);
}

/// Offset from starting value of SP counted in words.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub struct StackDepth(u32);

impl StackDepth {
    pub fn reserve(&mut self, slots: u32) {
        self.0 += slots;
    }

    pub fn free(&mut self, slots: u32) {
        self.0 -= slots;
    }
}

pub fn current_block_state(ctx: &Context) -> BlockState {
    ctx.block_state.clone()
}

pub fn restore_block_state(ctx: &mut Context, block_state: BlockState) {
    ctx.block_state = block_state;
}

pub fn push_return_value(ctx: &mut Context) {
    ctx.block_state.stack.push(StackValue::Temp(RAX));
}

fn push_i32(ctx: &mut Context, value: Value) {
    let stack_loc = match value {
        Value::Local(loc) => StackValue::Local(loc),
        Value::Temp(gpr) => {
            if ctx.block_state.regs.free_scratch() >= 1 {
                StackValue::Temp(gpr)
            } else {
                ctx.block_state.depth.reserve(1);
                dynasm!(ctx.asm
                    ; push Rq(gpr)
                );
                ctx.block_state.regs.release_scratch_gpr(gpr);
                StackValue::Pop
            }
        }
    };

    ctx.block_state.stack.push(stack_loc);
}

fn pop_i32(ctx: &mut Context) -> Value {
    match ctx.block_state.stack.pop().expect("Stack is empty") {
        StackValue::Local(loc) => Value::Local(loc),
        StackValue::Temp(reg) => Value::Temp(reg),
        StackValue::Pop => {
            ctx.block_state.depth.free(1);
            let gpr = ctx.block_state.regs.take_scratch_gpr();
            dynasm!(ctx.asm
                ; pop Rq(gpr)
            );
            Value::Temp(gpr)
        }
    }
}

fn pop_i32_into(ctx: &mut Context, dst: ValueLocation) {
    let val = pop_i32(ctx);
    let val_loc = val.location(&ctx.locals);
    copy_value(ctx, val_loc, dst);
    free_val(ctx, val);
}

fn free_val(ctx: &mut Context, val: Value) {
    match val {
        Value::Temp(reg) => ctx.block_state.regs.release_scratch_gpr(reg),
        Value::Local(_) => {}
    }
}

/// Puts this value into a register so that it can be efficiently read
fn into_reg(ctx: &mut Context, val: Value) -> GPR {
    match val.location(&ctx.locals) {
        ValueLocation::Stack(offset) => {
            let offset = adjusted_offset(ctx, offset);
            let scratch = ctx.block_state.regs.take_scratch_gpr();
            dynasm!(ctx.asm
                ; mov Rq(scratch), [rsp + offset]
            );
            scratch
        }
        ValueLocation::Reg(reg) => reg,
    }
}

/// Puts this value into a temporary register so that operations
/// on that register don't write to a local.
fn into_temp_reg(ctx: &mut Context, val: Value) -> GPR {
    match val {
        Value::Local(loc) => {
            let scratch = ctx.block_state.regs.take_scratch_gpr();

            match local_location(&ctx.locals, loc) {
                ValueLocation::Stack(offset) => {
                    let offset = adjusted_offset(ctx, offset);
                    dynasm!(ctx.asm
                        ; mov Rq(scratch), [rsp + offset]
                    );
                }
                ValueLocation::Reg(reg) => {
                    dynasm!(ctx.asm
                        ; mov Rq(scratch), Rq(reg)
                    );
                }
            }

            scratch
        }
        Value::Temp(reg) => reg,
    }
}

// TODO: For the commutative instructions we can do operands in either
//       order, so we can choose the operand order that creates the
//       least unnecessary temps.
pub fn i32_add(ctx: &mut Context) {
    let op0 = pop_i32(ctx);
    let tmp = pop_i32(ctx);
    let op1 = into_temp_reg(ctx, tmp);
    match op0.location(&ctx.locals) {
        ValueLocation::Reg(reg) => {
            dynasm!(ctx.asm
                ; add Rd(op1), Rd(reg)
            );
        }
        ValueLocation::Stack(offset) => {
            let offset = adjusted_offset(ctx, offset);
            dynasm!(ctx.asm
                ; add Rd(op1), [rsp + offset]
            );
        }
    }
    ctx.block_state.stack.push(StackValue::Temp(op1));
    free_val(ctx, op0);
}

pub fn i32_sub(ctx: &mut Context) {
    let op0 = pop_i32(ctx);
    let tmp = pop_i32(ctx);
    let op1 = into_temp_reg(ctx, tmp);
    match op0.location(&ctx.locals) {
        ValueLocation::Reg(reg) => {
            dynasm!(ctx.asm
                ; sub Rd(op1), Rd(reg)
            );
        }
        ValueLocation::Stack(offset) => {
            let offset = adjusted_offset(ctx, offset);
            dynasm!(ctx.asm
                ; sub Rd(op1), [rsp + offset]
            );
        }
    }
    ctx.block_state.stack.push(StackValue::Temp(op1));
    free_val(ctx, op0);
}

pub fn i32_and(ctx: &mut Context) {
    let op0 = pop_i32(ctx);
    let tmp = pop_i32(ctx);
    let op1 = into_temp_reg(ctx, tmp);
    match op0.location(&ctx.locals) {
        ValueLocation::Reg(reg) => {
            dynasm!(ctx.asm
                ; and Rd(op1), Rd(reg)
            );
        }
        ValueLocation::Stack(offset) => {
            let offset = adjusted_offset(ctx, offset);
            dynasm!(ctx.asm
                ; and Rd(op1), [rsp + offset]
            );
        }
    }
    ctx.block_state.stack.push(StackValue::Temp(op1));
    free_val(ctx, op0);
}

pub fn i32_or(ctx: &mut Context) {
    let op0 = pop_i32(ctx);
    let tmp = pop_i32(ctx);
    let op1 = into_temp_reg(ctx, tmp);
    match op0.location(&ctx.locals) {
        ValueLocation::Reg(reg) => {
            dynasm!(ctx.asm
                ; or Rd(op1), Rd(reg)
            );
        }
        ValueLocation::Stack(offset) => {
            let offset = adjusted_offset(ctx, offset);
            dynasm!(ctx.asm
                ; or Rd(op1), [rsp + offset]
            );
        }
    }
    ctx.block_state.stack.push(StackValue::Temp(op1));
    free_val(ctx, op0);
}

pub fn i32_xor(ctx: &mut Context) {
    let op0 = pop_i32(ctx);
    let tmp = pop_i32(ctx);
    let op1 = into_temp_reg(ctx, tmp);
    match op0.location(&ctx.locals) {
        ValueLocation::Reg(reg) => {
            dynasm!(ctx.asm
                ; xor Rd(op1), Rd(reg)
            );
        }
        ValueLocation::Stack(offset) => {
            let offset = adjusted_offset(ctx, offset);
            dynasm!(ctx.asm
                ; xor Rd(op1), [rsp + offset]
            );
        }
    }
    ctx.block_state.stack.push(StackValue::Temp(op1));
    free_val(ctx, op0);
}

pub fn i32_mul(ctx: &mut Context) {
    let op0 = pop_i32(ctx);
    let tmp = pop_i32(ctx);
    let op1 = into_temp_reg(ctx, tmp);
    match op0.location(&ctx.locals) {
        ValueLocation::Reg(reg) => {
            dynasm!(ctx.asm
                ; imul Rd(op1), Rd(reg)
            );
        }
        ValueLocation::Stack(offset) => {
            let offset = adjusted_offset(ctx, offset);
            dynasm!(ctx.asm
                ; imul Rd(op1), [rsp + offset]
            );
        }
    }
    ctx.block_state.stack.push(StackValue::Temp(op1));
    free_val(ctx, op0);
}

pub fn get_local_i32(ctx: &mut Context, local_idx: u32) {
    push_i32(ctx, Value::Local(local_idx));
}

// TODO: We can put locals that were spilled to the stack
//       back into registers here.
pub fn set_local_i32(ctx: &mut Context, local_idx: u32) {
    let val = pop_i32(ctx);
    let val_loc = val.location(&ctx.locals);
    let dst_loc = local_location(&ctx.locals, local_idx);
    copy_value(ctx, val_loc, dst_loc);
    free_val(ctx, val);
}

// TODO: Don't store literals at all, roll them into `Value`
pub fn literal_i32(ctx: &mut Context, imm: i32) {
    let gpr = ctx.block_state.regs.take_scratch_gpr();
    dynasm!(ctx.asm
        ; mov Rd(gpr), imm
    );
    push_i32(ctx, Value::Temp(gpr));
}

pub fn relop_eq_i32(ctx: &mut Context) {
    let right = pop_i32(ctx);
    let left = pop_i32(ctx);
    let result = ctx.block_state.regs.take_scratch_gpr();
    let lreg = into_reg(ctx, left);
    match right.location(&ctx.locals) {
        ValueLocation::Stack(offset) => {
            let offset = adjusted_offset(ctx, offset);
            dynasm!(ctx.asm
                ; xor Rq(result), Rq(result)
                ; cmp Rd(lreg), [rsp + offset]
                ; sete Rb(result)
            );
        }
        ValueLocation::Reg(rreg) => {
            dynasm!(ctx.asm
                ; xor Rq(result), Rq(result)
                ; cmp Rd(lreg), Rd(rreg)
                ; sete Rb(result)
            );
        }
    }
    push_i32(ctx, Value::Temp(result));
    free_val(ctx, left);
    free_val(ctx, right);
}

/// Pops i32 predicate and branches to the specified label
/// if the predicate is equal to zero.
pub fn pop_and_breq(ctx: &mut Context, label: Label) {
    let val = pop_i32(ctx);
    let predicate = into_temp_reg(ctx, val);
    dynasm!(ctx.asm
        ; test Rd(predicate), Rd(predicate)
        ; je =>label.0
    );
    ctx.block_state.regs.release_scratch_gpr(predicate);
}

/// Branch unconditionally to the specified label.
pub fn br(ctx: &mut Context, label: Label) {
    dynasm!(ctx.asm
        ; jmp =>label.0
    );
}

pub fn prepare_return_value(ctx: &mut Context) {
    pop_i32_into(ctx, ValueLocation::Reg(RAX));
}

fn copy_value(ctx: &mut Context, src: ValueLocation, dst: ValueLocation) {
    match (src, dst) {
        (ValueLocation::Stack(in_offset), ValueLocation::Stack(out_offset)) => {
            let in_offset = adjusted_offset(ctx, in_offset);
            let out_offset = adjusted_offset(ctx, out_offset);
            if in_offset != out_offset {
                let gpr = ctx.block_state.regs.take_scratch_gpr();
                dynasm!(ctx.asm
                    ; mov Rq(gpr), [rsp + in_offset]
                    ; mov [rsp + out_offset], Rq(gpr)
                );
                ctx.block_state.regs.release_scratch_gpr(gpr);
            }
        }
        (ValueLocation::Reg(in_reg), ValueLocation::Stack(out_offset)) => {
            let out_offset = adjusted_offset(ctx, out_offset);
            dynasm!(ctx.asm
                ; mov [rsp + out_offset], Rq(in_reg)
            );
        }
        (ValueLocation::Stack(in_offset), ValueLocation::Reg(out_reg)) => {
            let in_offset = adjusted_offset(ctx, in_offset);
            dynasm!(ctx.asm
                ; mov Rq(out_reg), [rsp + in_offset]
            );
        }
        (ValueLocation::Reg(in_reg), ValueLocation::Reg(out_reg)) => {
            if in_reg != out_reg {
                dynasm!(ctx.asm
                    ; mov Rq(out_reg), Rq(in_reg)
                );
            }
        }
    }
}

#[must_use]
pub struct CallCleanup {
    restore_registers: Vec<GPR>,
    stack_depth: i32,
}

/// Make sure that any argument registers that will be used by the call are free
/// by storing them to the stack.
///
/// Unfortunately, we can't elide this store if we're just passing arguments on
/// because these registers are caller-saved and so the callee can use them as
/// scratch space.
fn free_arg_registers(ctx: &mut Context, count: u32) {
    if count == 0 {
        return;
    }

    for i in 0..ctx.locals.locs.len() {
        match ctx.locals.locs[i] {
            ValueLocation::Reg(reg) => {
                if ARGS_IN_GPRS.contains(&reg) {
                    let offset = adjusted_offset(ctx, (i as u32 * WORD_SIZE) as _);
                    dynasm!(ctx.asm
                        ; mov [rsp + offset], Rq(reg)
                    );
                    ctx.locals.locs[i] = ValueLocation::Stack(offset);
                }
            }
            _ => {}
        }
    }
}

fn free_return_register(ctx: &mut Context, count: u32) {
    if count == 0 {
        return;
    }

    for stack_val in &mut ctx.block_state.stack {
        match stack_val.location(&ctx.locals) {
            // For now it's impossible for a local to be in RAX but that might be
            // possible in the future, so we check both cases.
            Some(ValueLocation::Reg(RAX)) => {
                let scratch = ctx.block_state.regs.take_scratch_gpr();
                dynasm!(ctx.asm
                    ; mov Rq(scratch), rax
                );
                *stack_val = StackValue::Temp(scratch);
            }
            _ => {}
        }
    }
}

// TODO: Use `ArrayVec`?
/// Saves volatile (i.e. caller-saved) registers before a function call, if they are used.
fn save_volatile(ctx: &mut Context) -> Vec<GPR> {
    let mut out = vec![];

    // TODO: If there are no `StackValue::Pop`s that need to be popped
    //       before we reach our `Temp` value, we can set the `StackValue`
    //       for the register to be restored to `StackValue::Pop` (and
    //       release the register!) instead of restoring it.
    for &reg in SCRATCH_REGS.iter() {
        if !ctx.block_state.regs.is_free(reg) {
            dynasm!(ctx.asm
                ; push Rq(reg)
            );
            out.push(reg);
        }
    }

    out
}

/// Write the arguments to the callee to the registers and the stack using the SystemV
/// calling convention.
fn pass_outgoing_args(ctx: &mut Context, arity: u32) -> CallCleanup {
    let num_stack_args = (arity as usize).saturating_sub(ARGS_IN_GPRS.len()) as i32;

    let out = CallCleanup {
        stack_depth: num_stack_args,
        restore_registers: save_volatile(ctx),
    };

    // We pop stack arguments first - arguments are RTL
    if num_stack_args > 0 {
        let size = num_stack_args * WORD_SIZE as i32;

        // Reserve space for the outgoing stack arguments (so we don't
        // stomp on any locals or the value stack).
        dynasm!(ctx.asm
            ; sub rsp, size
        );
        ctx.block_state.depth.reserve(num_stack_args as u32);

        for stack_slot in (0..num_stack_args).rev() {
            // Since the stack offset is from the bottom of the locals
            // and we want to start from the actual RSP (so `offset = 0`
            // writes to `[rsp]`), we subtract our current depth.
            //
            // We might want to do this in the future by having a separate
            // `AbsoluteValueLocation` and `RelativeValueLocation`.
            let offset =
                stack_slot * WORD_SIZE as i32 - ctx.block_state.depth.0 as i32 * WORD_SIZE as i32;
            pop_i32_into(ctx, ValueLocation::Stack(offset));
        }
    }

    for reg in ARGS_IN_GPRS[..(arity as usize).min(ARGS_IN_GPRS.len())]
        .iter()
        .rev()
    {
        pop_i32_into(ctx, ValueLocation::Reg(*reg));
    }

    out
}

/// Frees up the stack space used for stack-passed arguments and restores the value
/// of volatile (i.e. caller-saved) registers to the state that they were in before
/// the call.
fn post_call_cleanup(ctx: &mut Context, mut cleanup: CallCleanup) {
    if cleanup.stack_depth > 0 {
        let size = cleanup.stack_depth * WORD_SIZE as i32;
        dynasm!(ctx.asm
            ; add rsp, size
        );
    }

    for reg in cleanup.restore_registers.drain(..).rev() {
        dynasm!(ctx.asm
            ; pop Rq(reg)
        );
    }
}

/// Call a function with the given index
pub fn call_direct(ctx: &mut Context, index: u32, arg_arity: u32, return_arity: u32) {
    assert!(
        return_arity == 0 || return_arity == 1,
        "We don't support multiple return yet"
    );

    free_arg_registers(ctx, arg_arity);
    free_return_register(ctx, return_arity);

    let cleanup = pass_outgoing_args(ctx, arg_arity);

    let label = &ctx.func_starts[index as usize].1;
    dynasm!(ctx.asm
        ; call =>*label
    );

    post_call_cleanup(ctx, cleanup);
}

// TODO: Reserve space to store RBX, RBP, and R12..R15 so we can use them
//       as scratch registers
// TODO: Allow use of unused argument registers as scratch registers.
/// Writes the function prologue and stores the arguments as locals
pub fn start_function(ctx: &mut Context, arguments: u32, locals: u32) {
    let reg_args = &ARGS_IN_GPRS[..(arguments as usize).min(ARGS_IN_GPRS.len())];

    // We need space to store the register arguments if we need to call a function
    // and overwrite these registers so we add `reg_args.len()`
    let locals = locals + reg_args.len() as u32;
    // Align stack slots to the nearest even number. This is required
    // by x86-64 ABI.
    let aligned_stack_slots = (locals + 1) & !1;
    let framesize: i32 = aligned_stack_slots as i32 * WORD_SIZE as i32;

    ctx.locals.locs = reg_args
        .iter()
        .cloned()
        .map(ValueLocation::Reg)
        .chain(
            (0..arguments.saturating_sub(ARGS_IN_GPRS.len() as _))
                // We add 2 here because 1 stack slot is used for the stack pointer and another is
                // used for the return address. It's a magic number but there's not really a way
                // around this.
                .map(|arg_i| ValueLocation::Stack(((arg_i + 2) * WORD_SIZE) as i32 + framesize)),
        )
        .collect();

    dynasm!(ctx.asm
        ; push rbp
        ; mov rbp, rsp
    );

    if framesize > 0 {
        dynasm!(ctx.asm
            ; sub rsp, framesize
        );
    }
}

/// Writes the function epilogue, restoring the stack pointer and returning to the
/// caller.
pub fn epilogue(ctx: &mut Context) {
    // We don't need to clean up the stack - RSP is restored and
    // the calling function has its own register stack and will
    // stomp on the registers from our stack if necessary.
    dynasm!(ctx.asm
        ; mov rsp, rbp
        ; pop rbp
        ; ret
    );
}

pub fn trap(ctx: &mut Context) {
    dynasm!(ctx.asm
        ; ud2
    );
}
