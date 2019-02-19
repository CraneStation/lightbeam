use crate::module::ModuleContext;
use std::{
    fmt,
    iter::{self, FromIterator},
    ops::RangeInclusive,
};
use wasmparser::{
    FunctionBody, Ieee32, Ieee64, MemoryImmediate, Operator as WasmOperator, OperatorsReader,
};

pub fn dis<L>(function_name: impl fmt::Display, microwasm: &[Operator<L>]) -> String
where
    BrTarget<L>: fmt::Display,
    L: Clone,
{
    use std::fmt::Write;

    const DISASSEMBLE_BLOCK_DEFS: bool = true;

    let mut asm = format!(".fn_{}:\n", function_name);
    let mut out = String::new();

    let p = "      ";
    for op in microwasm {
        if op.is_label() {
            writeln!(asm, "{}", op).unwrap();
        } else if op.is_block() {
            writeln!(out, "{}", op).unwrap();
        } else {
            writeln!(asm, "{}{}", p, op).unwrap();
        }
    }

    let out = if DISASSEMBLE_BLOCK_DEFS {
        writeln!(out).unwrap();
        writeln!(out, "{}", asm).unwrap();
        out
    } else {
        asm
    };

    out
}

/// A constant value embedded in the instructions
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Value {
    I32(i32),
    I64(i64),
    F32(Ieee32),
    F64(Ieee64),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::I32(v) => write!(f, "{}i32", v),
            Value::I64(v) => write!(f, "{}i64", v),
            Value::F32(v) => write!(f, "{}f32", f32::from_bits(v.bits())),
            Value::F64(v) => write!(f, "{}f64", f64::from_bits(v.bits())),
        }
    }
}

impl Value {
    fn default_for_type(ty: SignlessType) -> Self {
        match ty {
            Type::Int(Size::_32) => Value::I32(0),
            Type::Int(Size::_64) => Value::I64(0),
            Type::Float(Size::_32) => Value::F32(Ieee32(0)),
            Type::Float(Size::_64) => Value::F64(Ieee64(0)),
        }
    }
}

/// Whether to interpret an integer as signed or unsigned
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Signedness {
    Signed,
    Unsigned,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Size {
    _32,
    _64,
}

type Int = Size;
type Float = Size;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct SignfulInt(Signedness, Size);

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Type<I> {
    Int(I),
    Float(Size),
}

impl fmt::Display for SignfulType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Type::Int(i) => write!(f, "{}", i),
            Type::Float(Size::_32) => write!(f, "f32"),
            Type::Float(Size::_64) => write!(f, "f64"),
        }
    }
}

impl fmt::Display for SignlessType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Type::Int(Size::_32) => write!(f, "i32"),
            Type::Int(Size::_64) => write!(f, "i64"),
            Type::Float(Size::_32) => write!(f, "f32"),
            Type::Float(Size::_64) => write!(f, "f64"),
        }
    }
}

impl fmt::Display for SignfulInt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SignfulInt(Signedness::Signed, Size::_32) => write!(f, "i32"),
            SignfulInt(Signedness::Unsigned, Size::_32) => write!(f, "u32"),
            SignfulInt(Signedness::Signed, Size::_64) => write!(f, "i64"),
            SignfulInt(Signedness::Unsigned, Size::_64) => write!(f, "u64"),
        }
    }
}

pub type SignlessType = Type<Size>;
pub type SignfulType = Type<SignfulInt>;

pub const I32: SignlessType = Type::Int(Size::_32);
pub const I64: SignlessType = Type::Int(Size::_64);
pub const F32: SignlessType = Type::Float(Size::_32);
pub const F64: SignlessType = Type::Float(Size::_64);

pub mod sint {
    use super::{Signedness, SignfulInt, Size};

    pub const I32: SignfulInt = SignfulInt(Signedness::Signed, Size::_32);
    pub const I64: SignfulInt = SignfulInt(Signedness::Signed, Size::_64);
    pub const U32: SignfulInt = SignfulInt(Signedness::Unsigned, Size::_32);
    pub const U64: SignfulInt = SignfulInt(Signedness::Unsigned, Size::_64);
}

pub const SI32: SignfulType = Type::Int(sint::I32);
pub const SI64: SignfulType = Type::Int(sint::I64);
pub const SU32: SignfulType = Type::Int(sint::U32);
pub const SU64: SignfulType = Type::Int(sint::U64);
pub const SF32: SignfulType = Type::Float(Size::_32);
pub const SF64: SignfulType = Type::Float(Size::_64);

impl SignlessType {
    pub fn from_wasm(other: wasmparser::Type) -> Option<Self> {
        use wasmparser::Type;

        match other {
            Type::I32 => Some(I32),
            Type::I64 => Some(I64),
            Type::F32 => Some(F32),
            Type::F64 => Some(F64),
            Type::EmptyBlockType => None,
            _ => unimplemented!(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BrTable<L> {
    targets: Vec<L>,
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum NameTag {
    Header,
    Else,
    End,
}

pub type WasmLabel = (u32, NameTag);

trait Label {
    // TODO
}

// TODO: This is for Wasm blocks - we should have an increasing ID for each block that we hit.
impl Label for (u32, NameTag) {}

type OperatorFromWasm = Operator<WasmLabel>;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum BrTarget<L> {
    Return,
    Label(L),
}

impl<L> BrTarget<L> {
    pub fn label(&self) -> Option<&L> {
        match self {
            BrTarget::Return => None,
            BrTarget::Label(l) => Some(l),
        }
    }
}

impl fmt::Display for BrTarget<WasmLabel> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BrTarget::Return => write!(f, ".return"),
            BrTarget::Label((i, NameTag::Header)) => write!(f, ".L{}", i),
            BrTarget::Label((i, NameTag::Else)) => write!(f, ".L{}_else", i),
            BrTarget::Label((i, NameTag::End)) => write!(f, ".L{}_end", i),
        }
    }
}

impl fmt::Display for BrTarget<&str> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BrTarget::Return => write!(f, ".return"),
            BrTarget::Label(l) => write!(f, ".L{}", l),
        }
    }
}

// TODO: Explicit VmCtx?
#[derive(Debug, Clone)]
pub enum Operator<Label> {
    /// Explicit trap instruction
    Unreachable,
    /// Start a new block. It is an error if the previous block has not been closed by emitting a `Br` or
    /// `BrTable`.
    Block {
        label: Label,
        // TODO: Do we need this?
        params: Vec<SignlessType>,
        // TODO: Ideally we'd have `num_backwards_callers` but we can't know that for WebAssembly
        has_backwards_callers: bool,
        num_callers: Option<u32>,
    },
    Label(Label),
    /// Unconditionally break to a new block. This the parameters off the stack and passes them into
    /// the new block. Any remaining elements on the stack are discarded.
    Br {
        /// Returning from the function is just calling the "return" block
        target: BrTarget<Label>,
    },
    /// Pop a value off the top of the stack, jump to the `else_` label if this value is `true`
    /// and the `then` label otherwise. The `then` and `else_` blocks must have the same parameters.
    BrIf {
        /// Label to jump to if the value at the top of the stack is true
        then: BrTarget<Label>,
        /// Label to jump to if the value at the top of the stack is false
        else_: BrTarget<Label>,
    },
    /// Pop a value off the top of the stack, jump to `table[value.min(table.len() - 1)]`. All elements
    /// in the table must have the same parameters.
    BrTable {
        /// The table of labels to jump to - the index should be clamped to the length of the table
        table: BrTable<Label>,
    },
    /// Call a function
    Call {
        function_index: u32,
    },
    /// Pop an `i32` off the top of the stack, index into the table at `table_index` and call that function
    CallIndirect {
        type_index: u32,
        table_index: u32,
    },
    /// Pop an element off of the stack and discard it.
    Drop(RangeInclusive<u32>),
    /// Pop an `i32` off of the stack and 2 elements off of the stack, call them `A` and `B` where `A` is the
    /// first element popped and `B` is the second. If the `i32` is 0 then discard `B` and push `A` back onto
    /// the stack, otherwise discard `A` and push `B` back onto the stack.
    Select,
    /// Duplicate the element at depth `depth` to the top of the stack. This can be used to implement
    /// `GetLocal`.
    Pick {
        depth: u32,
    },
    /// Swap the top element of the stack with the element at depth `depth`. This can be used to implement
    /// `SetLocal`.
    // TODO: Is it better to have `Swap`, to have `Pull` (which moves the `nth` element instead of swapping)
    //       or to have both?
    Swap {
        depth: u32,
    },
    GetGlobal {
        index: u32,
    },
    SetGlobal {
        index: u32,
    },
    Load {
        ty: SignlessType,
        memarg: MemoryImmediate,
    },
    Load8 {
        ty: SignfulInt,
        memarg: MemoryImmediate,
    },
    Load16 {
        ty: SignfulInt,
        memarg: MemoryImmediate,
    },
    // Only available for {I,U}64
    // TODO: Roll this into `Load` somehow?
    Load32 {
        sign: Signedness,
        memarg: MemoryImmediate,
    },
    Store {
        ty: SignlessType,
        memarg: MemoryImmediate,
    },
    Store8 {
        /// `ty` on integers
        ty: Int,
        memarg: MemoryImmediate,
    },
    Store16 {
        /// `ty` on integers
        ty: Int,
        memarg: MemoryImmediate,
    },
    // Only available for I64
    // TODO: Roll this into `Store` somehow?
    Store32 {
        memarg: MemoryImmediate,
    },
    MemorySize {
        reserved: u32,
    },
    MemoryGrow {
        reserved: u32,
    },
    Const(Value),
    RefNull,
    RefIsNull,
    Eq(SignlessType),
    Ne(SignlessType),
    /// `eqz` on integers
    Eqz(Int),
    Lt(SignfulType),
    Gt(SignfulType),
    Le(SignfulType),
    Ge(SignfulType),
    Add(SignlessType),
    Sub(SignlessType),
    Mul(SignlessType),
    /// `clz` on integers
    Clz(Int),
    /// `ctz` on integers
    Ctz(Int),
    /// `popcnt` on integers
    Popcnt(Int),
    Div(SignfulType),
    Rem(SignfulInt),
    And(Int),
    Or(Int),
    Xor(Int),
    Shl(Int),
    Shr(SignfulInt),
    Rotl(Int),
    Rotr(Int),
    Abs(Float),
    Neg(Float),
    Ceil(Float),
    Floor(Float),
    Trunc(Float),
    Nearest(Float),
    Sqrt(Float),
    Min(Float),
    Max(Float),
    Copysign(Float),
    I32WrapFromI64,
    ITruncFromF {
        input_ty: Float,
        output_ty: SignfulInt,
    },
    FConvertFromI {
        input_ty: SignfulInt,
        output_ty: Float,
    },
    F32DemoteFromF64,
    F64PromoteFromF32,
    I32ReinterpretFromF32,
    I64ReinterpretFromF64,
    F32ReinterpretFromI32,
    F64ReinterpretFromI64,
    // Only available for input I32 and output I64
    Extend {
        sign: Signedness,
    },
    // 0xFC operators
    /// Non-trapping Float-to-int conversion
    ISatTruncFromF {
        input_ty: Float,
        output_ty: SignfulInt,
    },

    // 0xFC operators
    // bulk memory https://github.com/WebAssembly/bulk-memory-operations/blob/master/proposals/bulk-memory-operations/Overview.md
    MemoryInit {
        segment: u32,
    },
    DataDrop {
        segment: u32,
    },
    MemoryCopy,
    MemoryFill,
    TableInit {
        segment: u32,
    },
    ElemDrop {
        segment: u32,
    },
    TableCopy,
}

impl<L> Operator<L> {
    pub fn is_label(&self) -> bool {
        match self {
            Operator::Label(..) => true,
            _ => false,
        }
    }

    pub fn is_block(&self) -> bool {
        match self {
            Operator::Block { .. } => true,
            _ => false,
        }
    }

    pub fn end(params: Vec<SignlessType>, label: L) -> Self {
        Operator::Block {
            params,
            label,
            has_backwards_callers: false,
            // TODO
            num_callers: None,
        }
    }

    pub fn block(params: Vec<SignlessType>, label: L) -> Self {
        Operator::Block {
            params,
            label,
            has_backwards_callers: false,
            num_callers: Some(1),
        }
    }

    pub fn loop_(params: Vec<SignlessType>, label: L) -> Self {
        Operator::Block {
            params,
            label,
            has_backwards_callers: true,
            num_callers: None,
        }
    }
}

impl<L> fmt::Display for Operator<L>
where
    BrTarget<L>: fmt::Display,
    L: Clone,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Operator::Unreachable => write!(f, "unreachable"),
            Operator::Label(label) => write!(f, "{}:", BrTarget::Label(label.clone())),
            Operator::Block {
                label,
                params,
                has_backwards_callers,
                num_callers,
            } => {
                write!(f, "def {} :: [", BrTarget::Label(label.clone()))?;
                let mut iter = params.iter();
                if let Some(p) = iter.next() {
                    write!(f, "{}", p)?;
                    for p in iter {
                        write!(f, ", {}", p)?;
                    }
                }
                write!(f, "]")?;

                if *has_backwards_callers {
                    write!(f, " has_backwards_callers")?;
                }

                if let Some(n) = num_callers {
                    write!(f, " num_callers={}", n)?;
                }

                Ok(())
            }
            Operator::Br { target } => write!(f, "br {}", target),
            Operator::BrIf { then, else_ } => write!(f, "br_if {}, {}", then, else_),
            Operator::Call { function_index } => write!(f, "call {}", function_index),
            Operator::CallIndirect { .. } => write!(f, "call_indirect"),
            Operator::Drop(range) => {
                write!(f, "drop")?;

                match range.clone().into_inner() {
                    (0, 0) => {}
                    (start, end) if start == end => {
                        write!(f, " {}", start)?;
                    }
                    (start, end) => {
                        write!(f, " {}..={}", start, end)?;
                    }
                }

                Ok(())
            }
            Operator::Select => write!(f, "select"),
            Operator::Pick { depth } => write!(f, "pick {}", depth),
            Operator::Swap { depth } => write!(f, "swap {}", depth),
            Operator::Load { ty, memarg } => {
                write!(f, "{}.load {}, {}", ty, memarg.flags, memarg.offset)
            }
            Operator::Load8 { ty, memarg } => {
                write!(f, "{}.load8 {}, {}", ty, memarg.flags, memarg.offset)
            }
            Operator::Load16 { ty, memarg } => {
                write!(f, "{}.load16 {}, {}", ty, memarg.flags, memarg.offset)
            }
            Operator::Load32 { sign, memarg } => write!(
                f,
                "{}.load32 {}, {}",
                SignfulInt(*sign, Size::_64),
                memarg.flags,
                memarg.offset
            ),
            Operator::Store { ty, memarg } => {
                write!(f, "{}.store {}, {}", ty, memarg.flags, memarg.offset)
            }
            Operator::Store8 { ty, memarg } => write!(
                f,
                "{}.store8 {}, {}",
                SignfulInt(Signedness::Unsigned, *ty),
                memarg.flags,
                memarg.offset
            ),
            Operator::Store16 { ty, memarg } => write!(
                f,
                "{}.store16 {}, {}",
                SignfulInt(Signedness::Unsigned, *ty),
                memarg.flags,
                memarg.offset
            ),
            Operator::Store32 { memarg } => {
                write!(f, "u64.store32 {}, {}", memarg.flags, memarg.offset)
            }
            Operator::MemorySize { .. } => write!(f, "memory.size"),
            Operator::MemoryGrow { .. } => write!(f, "memory.grow"),
            Operator::Const(val) => write!(f, "const {}", val),
            Operator::RefNull => write!(f, "refnull"),
            Operator::RefIsNull => write!(f, "refisnull"),
            Operator::Eq(ty) => write!(f, "{}.eq", ty),
            Operator::Ne(ty) => write!(f, "{}.ne", ty),
            Operator::Eqz(ty) => write!(f, "{}.eqz", SignfulInt(Signedness::Unsigned, *ty)),
            Operator::Lt(ty) => write!(f, "{}.lt", ty),
            Operator::Gt(ty) => write!(f, "{}.gt", ty),
            Operator::Le(ty) => write!(f, "{}.le", ty),
            Operator::Ge(ty) => write!(f, "{}.ge", ty),
            Operator::Add(ty) => write!(f, "{}.add", ty),
            Operator::Sub(ty) => write!(f, "{}.sub", ty),
            Operator::Mul(ty) => write!(f, "{}.mul", ty),
            Operator::Clz(ty) => write!(f, "{}.clz", SignfulInt(Signedness::Unsigned, *ty)),
            Operator::Ctz(ty) => write!(f, "{}.ctz", SignfulInt(Signedness::Unsigned, *ty)),
            Operator::Popcnt(ty) => write!(f, "{}.popcnt", SignfulInt(Signedness::Unsigned, *ty)),
            Operator::Div(ty) => write!(f, "{}.div", ty),
            Operator::Rem(ty) => write!(f, "{}.rem", ty),
            Operator::And(ty) => write!(f, "{}.and", SignfulInt(Signedness::Unsigned, *ty)),
            Operator::Or(ty) => write!(f, "{}.or", SignfulInt(Signedness::Unsigned, *ty)),
            Operator::Xor(ty) => write!(f, "{}.xor", SignfulInt(Signedness::Unsigned, *ty)),
            Operator::Shl(ty) => write!(f, "{}.shl", SignfulInt(Signedness::Unsigned, *ty)),
            Operator::Shr(ty) => write!(f, "{}.shr", ty),
            Operator::Rotl(ty) => write!(f, "{}.rotl", SignfulInt(Signedness::Unsigned, *ty)),
            Operator::Rotr(ty) => write!(f, "{}.rotr", SignfulInt(Signedness::Unsigned, *ty)),
            Operator::Abs(ty) => write!(f, "{}.abs", Type::<Size>::Float(*ty)),
            Operator::Neg(ty) => write!(f, "{}.neg", Type::<Size>::Float(*ty)),
            Operator::Ceil(ty) => write!(f, "{}.ceil", Type::<Size>::Float(*ty)),
            Operator::Floor(ty) => write!(f, "{}.floor", Type::<Size>::Float(*ty)),
            Operator::Trunc(ty) => write!(f, "{}.trunc", Type::<Size>::Float(*ty)),
            Operator::Nearest(ty) => write!(f, "{}.nearest", Type::<Size>::Float(*ty)),
            Operator::Sqrt(ty) => write!(f, "{}.sqrt", Type::<Size>::Float(*ty)),
            Operator::Min(ty) => write!(f, "{}.min", Type::<Size>::Float(*ty)),
            Operator::Max(ty) => write!(f, "{}.max", Type::<Size>::Float(*ty)),
            Operator::Copysign(ty) => write!(f, "{}.copysign", Type::<Size>::Float(*ty)),
            Operator::I32WrapFromI64 => write!(f, "i32.wrapfromi64"),
            Operator::F32DemoteFromF64 => write!(f, "f32.demotefromf64"),
            Operator::F64PromoteFromF32 => write!(f, "f64.promotefromf32"),
            Operator::I32ReinterpretFromF32 => write!(f, "i32.reinterpretfromf32"),
            Operator::I64ReinterpretFromF64 => write!(f, "i64.reinterpretfromf64"),
            Operator::F32ReinterpretFromI32 => write!(f, "f32.reinterpretfromi32"),
            Operator::F64ReinterpretFromI64 => write!(f, "f64.reinterpretfromi64"),
            Operator::MemoryCopy => write!(f, "memory.copy"),
            Operator::MemoryFill => write!(f, "memory.fill"),
            Operator::TableCopy => write!(f, "table.copy"),
            _ => unimplemented!(),
        }
    }
}

/// Type of a control frame.
#[derive(Debug, Clone, PartialEq)]
enum ControlFrameKind {
    /// A regular block frame.
    ///
    /// Can be used for an implicit function block.
    Block {
        needs_end_label: bool,
    },
    Function,
    /// Loop frame (branching to the beginning of block).
    Loop,
    /// True-subblock of if expression.
    If {
        params: Vec<SignlessType>,
        has_else: bool,
    },
}

struct ControlFrame {
    id: u32,
    returns: u32,
    kind: ControlFrameKind,
}

impl ControlFrame {
    fn needs_end_label(&self) -> bool {
        match self.kind {
            ControlFrameKind::Block { needs_end_label } => needs_end_label,
            ControlFrameKind::If { .. } => true,
            ControlFrameKind::Loop | ControlFrameKind::Function => false,
        }
    }

    fn mark_branched_to(&mut self) {
        match &mut self.kind {
            ControlFrameKind::Block { needs_end_label } => *needs_end_label = true,
            _ => {}
        }
    }

    fn br_target(&self) -> BrTarget<(u32, NameTag)> {
        match self.kind {
            ControlFrameKind::Loop => BrTarget::Label((self.id, NameTag::Header)),
            ControlFrameKind::Function => BrTarget::Return,
            _ => BrTarget::Label((self.id, NameTag::End)),
        }
    }

    fn params(&self) -> Option<&[SignlessType]> {
        match &self.kind {
            ControlFrameKind::If { params, .. } => Some(params),
            _ => None,
        }
    }
}

pub struct MicrowasmConv<'a, 'b, M> {
    // TODO: Maybe have a `ConvInner` type and have this wrap an `Option` so that
    //       we can dealloc everything when we've finished emitting
    is_done: bool,
    consts_to_emit: Option<Vec<Value>>,
    stack: Vec<SignlessType>,
    internal: OperatorsReader<'a>,
    module: &'b M,
    current_id: u32,
    num_locals: u32,
    control_frames: Vec<ControlFrame>,
    unreachable: bool,
}

#[derive(Debug)]
enum SigT {
    T,
    Concrete(SignlessType),
}

impl From<SignlessType> for SigT {
    fn from(other: SignlessType) -> SigT {
        SigT::Concrete(other)
    }
}

#[derive(Debug)]
pub struct OpSig {
    input: Vec<SigT>,
    output: Vec<SigT>,
}

impl OpSig {
    fn new<I0, I1>(input: I0, output: I1) -> Self
    where
        I0: IntoIterator<Item = SigT>,
        I1: IntoIterator<Item = SigT>,
    {
        OpSig {
            input: Vec::from_iter(input),
            output: Vec::from_iter(output),
        }
    }

    fn none() -> Self {
        Self::new(None, None)
    }
}

impl From<&'_ wasmparser::FuncType> for OpSig {
    fn from(other: &wasmparser::FuncType) -> Self {
        OpSig::new(
            other
                .params
                .iter()
                .map(|t| SigT::Concrete(Type::from_wasm(*t).unwrap())),
            other
                .returns
                .iter()
                .map(|t| SigT::Concrete(Type::from_wasm(*t).unwrap())),
        )
    }
}

impl<'a, 'b, M: ModuleContext> MicrowasmConv<'a, 'b, M>
where
    for<'any> &'any M::Signature: Into<OpSig>,
{
    fn op_sig(&self, op: &WasmOperator) -> OpSig {
        use self::SigT::T;
        use std::iter::{empty as none, once};

        fn one<A>(a: A) -> impl IntoIterator<Item = SigT>
        where
            A: Into<SigT>,
        {
            once(a.into())
        }

        fn two<A, B>(a: A, b: B) -> impl IntoIterator<Item = SigT>
        where
            A: Into<SigT>,
            B: Into<SigT>,
        {
            once(a.into()).chain(once(b.into()))
        }

        fn three<A, B, C>(a: A, b: B, c: C) -> impl IntoIterator<Item = SigT>
        where
            A: Into<SigT>,
            B: Into<SigT>,
            C: Into<SigT>,
        {
            once(a.into()).chain(once(b.into())).chain(once(c.into()))
        }

        macro_rules! sig {
            (@iter $a:expr, $b:expr, $c:expr) => { three($a, $b, $c) };
            (@iter $a:expr, $b:expr) => { two($a, $b) };
            (@iter $a:expr) => { one($a) };
            (@iter) => { none() };
            (($($t:expr),*) -> ($($o:expr),*)) => {
                OpSig::new(sig!(@iter $($t),*), sig!(@iter $($o),*))
            };
        }

        match op {
            WasmOperator::Unreachable => OpSig::none(),
            WasmOperator::Nop => OpSig::none(),

            WasmOperator::Block { .. } => OpSig::none(),
            WasmOperator::Loop { .. } => OpSig::none(),
            WasmOperator::If { .. } => sig!((I32) -> ()),
            WasmOperator::Else => OpSig::none(),
            WasmOperator::End => OpSig::none(),

            WasmOperator::Br { .. } => OpSig::none(),
            WasmOperator::BrIf { .. } => sig!((I32) -> ()),
            WasmOperator::BrTable { .. } => sig!((I32) -> ()),
            WasmOperator::Return => OpSig::none(),

            WasmOperator::Call { function_index } => {
                let func_type = self.module.func_type(*function_index);
                func_type.into()
            }
            WasmOperator::CallIndirect { index, .. } => {
                let func_type = self.module.signature(*index);
                let mut out = func_type.into();
                out.input.push(I32.into());
                out
            }

            WasmOperator::Drop => sig!((T) -> ()),

            // `Select` pops 3 elements and pushes 1
            WasmOperator::Select => sig!((T, T, I32) -> (T)),

            WasmOperator::GetLocal { local_index } => {
                let ty = self.stack[*local_index as usize];

                sig!(() -> (ty))
            }
            WasmOperator::SetLocal { local_index } => {
                let ty = self.stack[*local_index as usize];

                sig!((ty) -> ())
            }
            WasmOperator::TeeLocal { local_index } => {
                let ty = self.stack[*local_index as usize];

                sig!((ty) -> (ty))
            }

            WasmOperator::GetGlobal { global_index: _ } => unimplemented!(),
            WasmOperator::SetGlobal { global_index: _ } => unimplemented!(),

            WasmOperator::F32Load { .. } => sig!((I32) -> (F32)),
            WasmOperator::F64Load { .. } => sig!((I32) -> (F64)),

            WasmOperator::I32Load { .. }
            | WasmOperator::I32Load8S { .. }
            | WasmOperator::I32Load8U { .. }
            | WasmOperator::I32Load16S { .. }
            | WasmOperator::I32Load16U { .. } => sig!((I32) -> (I32)),

            WasmOperator::I64Load { .. }
            | WasmOperator::I64Load8S { .. }
            | WasmOperator::I64Load8U { .. }
            | WasmOperator::I64Load16S { .. }
            | WasmOperator::I64Load16U { .. }
            | WasmOperator::I64Load32S { .. }
            | WasmOperator::I64Load32U { .. } => sig!((I32) -> (I64)),

            WasmOperator::F32Store { .. } => sig!((I32, F32) -> ()),
            WasmOperator::F64Store { .. } => sig!((I32, F64) -> ()),
            WasmOperator::I32Store { .. }
            | WasmOperator::I32Store8 { .. }
            | WasmOperator::I32Store16 { .. } => sig!((I32, I32) -> ()),
            WasmOperator::I64Store { .. }
            | WasmOperator::I64Store8 { .. }
            | WasmOperator::I64Store16 { .. }
            | WasmOperator::I64Store32 { .. } => sig!((I32, I64) -> ()),

            WasmOperator::MemorySize { .. } => sig!(() -> (I32)),
            WasmOperator::MemoryGrow { .. } => sig!((I32) -> (I32)),

            WasmOperator::I32Const { .. } => sig!(() -> (I32)),
            WasmOperator::I64Const { .. } => sig!(() -> (I64)),
            WasmOperator::F32Const { .. } => sig!(() -> (F32)),
            WasmOperator::F64Const { .. } => sig!(() -> (F64)),

            WasmOperator::RefNull => unimplemented!(),
            WasmOperator::RefIsNull => unimplemented!(),

            // All comparison operators remove 2 elements and push 1
            WasmOperator::I32Eqz => sig!((I32) -> (I32)),
            WasmOperator::I32Eq
            | WasmOperator::I32Ne
            | WasmOperator::I32LtS
            | WasmOperator::I32LtU
            | WasmOperator::I32GtS
            | WasmOperator::I32GtU
            | WasmOperator::I32LeS
            | WasmOperator::I32LeU
            | WasmOperator::I32GeS
            | WasmOperator::I32GeU => sig!((I32, I32) -> (I32)),

            WasmOperator::I64Eqz => sig!((I64) -> (I32)),
            WasmOperator::I64Eq
            | WasmOperator::I64Ne
            | WasmOperator::I64LtS
            | WasmOperator::I64LtU
            | WasmOperator::I64GtS
            | WasmOperator::I64GtU
            | WasmOperator::I64LeS
            | WasmOperator::I64LeU
            | WasmOperator::I64GeS
            | WasmOperator::I64GeU => sig!((I64, I64) -> (I32)),

            WasmOperator::F32Eq
            | WasmOperator::F32Ne
            | WasmOperator::F32Lt
            | WasmOperator::F32Gt
            | WasmOperator::F32Le
            | WasmOperator::F32Ge => sig!((F32) -> (I32)),

            WasmOperator::F64Eq
            | WasmOperator::F64Ne
            | WasmOperator::F64Lt
            | WasmOperator::F64Gt
            | WasmOperator::F64Le
            | WasmOperator::F64Ge => sig!((F64) -> (I32)),

            WasmOperator::I32Clz | WasmOperator::I32Ctz | WasmOperator::I32Popcnt => {
                sig!((I32) -> (I32))
            }
            WasmOperator::I64Clz | WasmOperator::I64Ctz | WasmOperator::I64Popcnt => {
                sig!((I64) -> (I64))
            }

            WasmOperator::I32Add
            | WasmOperator::I32Sub
            | WasmOperator::I32Mul
            | WasmOperator::I32DivS
            | WasmOperator::I32DivU
            | WasmOperator::I32RemS
            | WasmOperator::I32RemU
            | WasmOperator::I32And
            | WasmOperator::I32Or
            | WasmOperator::I32Xor
            | WasmOperator::I32Shl
            | WasmOperator::I32ShrS
            | WasmOperator::I32ShrU
            | WasmOperator::I32Rotl
            | WasmOperator::I32Rotr => sig!((I32, I32) -> (I32)),

            WasmOperator::I64Add
            | WasmOperator::I64Sub
            | WasmOperator::I64Mul
            | WasmOperator::I64DivS
            | WasmOperator::I64DivU
            | WasmOperator::I64RemS
            | WasmOperator::I64RemU
            | WasmOperator::I64And
            | WasmOperator::I64Or
            | WasmOperator::I64Xor
            | WasmOperator::I64Shl
            | WasmOperator::I64ShrS
            | WasmOperator::I64ShrU
            | WasmOperator::I64Rotl
            | WasmOperator::I64Rotr => sig!((I64, I64) -> (I64)),

            WasmOperator::F32Abs
            | WasmOperator::F32Neg
            | WasmOperator::F32Ceil
            | WasmOperator::F32Floor
            | WasmOperator::F32Trunc
            | WasmOperator::F32Nearest
            | WasmOperator::F32Sqrt => sig!((F32) -> (F32)),

            WasmOperator::F64Abs
            | WasmOperator::F64Neg
            | WasmOperator::F64Ceil
            | WasmOperator::F64Floor
            | WasmOperator::F64Trunc
            | WasmOperator::F64Nearest
            | WasmOperator::F64Sqrt => sig!((F64) -> (F64)),

            WasmOperator::F32Add
            | WasmOperator::F32Sub
            | WasmOperator::F32Mul
            | WasmOperator::F32Div
            | WasmOperator::F32Min
            | WasmOperator::F32Max
            | WasmOperator::F32Copysign => sig!((F32, F32) -> (F32)),

            WasmOperator::F64Add
            | WasmOperator::F64Sub
            | WasmOperator::F64Mul
            | WasmOperator::F64Div
            | WasmOperator::F64Min
            | WasmOperator::F64Max
            | WasmOperator::F64Copysign => sig!((F64, F64) -> (F64)),

            WasmOperator::I32WrapI64 => sig!((I64) -> (I32)),
            WasmOperator::I32TruncSF32 | WasmOperator::I32TruncUF32 => sig!((F32) -> (I32)),
            WasmOperator::I32TruncSF64 | WasmOperator::I32TruncUF64 => sig!((F64) -> (I32)),
            WasmOperator::I64ExtendSI32 | WasmOperator::I64ExtendUI32 => sig!((I32) -> (I64)),
            WasmOperator::I64TruncSF32 | WasmOperator::I64TruncUF32 => sig!((F32) -> (I64)),
            WasmOperator::I64TruncSF64 | WasmOperator::I64TruncUF64 => sig!((F64) -> (I64)),
            WasmOperator::F32ConvertSI32 | WasmOperator::F32ConvertUI32 => sig!((I32) -> (F32)),
            WasmOperator::F32ConvertSI64 | WasmOperator::F32ConvertUI64 => sig!((I64) -> (F32)),
            WasmOperator::F32DemoteF64 => sig!((F64) -> (F32)),
            WasmOperator::F64ConvertSI32 | WasmOperator::F64ConvertUI32 => sig!((I32) -> (F64)),
            WasmOperator::F64ConvertSI64 | WasmOperator::F64ConvertUI64 => sig!((I64) -> (F64)),
            WasmOperator::F64PromoteF32 => sig!((F32) -> (F64)),
            WasmOperator::I32ReinterpretF32 => sig!((F32) -> (I32)),
            WasmOperator::I64ReinterpretF64 => sig!((F64) -> (I64)),
            WasmOperator::F32ReinterpretI32 => sig!((I32) -> (F32)),
            WasmOperator::F64ReinterpretI64 => sig!((I64) -> (F64)),

            WasmOperator::I32Extend8S => sig!((I32) -> (I32)),
            WasmOperator::I32Extend16S => sig!((I32) -> (I32)),
            WasmOperator::I64Extend8S => sig!((I32) -> (I64)),
            WasmOperator::I64Extend16S => sig!((I32) -> (I64)),
            WasmOperator::I64Extend32S => sig!((I32) -> (I64)),

            _ => unimplemented!(),
        }
    }

    fn next_id(&mut self) -> u32 {
        let id = self.current_id;
        self.current_id += 1;
        id
    }

    fn nth_block(&self, n: usize) -> &ControlFrame {
        self.control_frames.iter().rev().nth(n).unwrap()
    }

    fn nth_block_mut(&mut self, n: usize) -> &mut ControlFrame {
        self.control_frames.iter_mut().rev().nth(n).unwrap()
    }

    fn function_block(&self) -> &ControlFrame {
        self.control_frames.first().unwrap()
    }

    fn local_depth(&self, idx: u32) -> u32 {
        self.stack.len() as u32 - 1 - idx
    }

    fn apply_op(&mut self, sig: OpSig) {
        let mut ty_param = None;

        for p in sig.input.into_iter().rev() {
            let stack_ty = self.stack.pop().expect("Stack is empty");
            let ty = match p {
                SigT::T => {
                    if let Some(t) = ty_param {
                        t
                    } else {
                        ty_param = Some(stack_ty);
                        stack_ty
                    }
                }
                SigT::Concrete(ty) => ty,
            };

            debug_assert_eq!(ty, stack_ty);
        }

        for p in sig.output.into_iter().rev() {
            let ty = match p {
                SigT::T => ty_param.expect("Type parameter was not set"),
                SigT::Concrete(ty) => ty,
            };
            self.stack.push(ty);
        }
    }

    fn block_params(&self) -> Vec<SignlessType> {
        self.stack.clone()
    }

    fn block_params_with_wasm_type(&self, ty: wasmparser::Type) -> Vec<SignlessType> {
        let mut out = self.block_params();
        out.extend(Type::from_wasm(ty));
        out
    }

    fn drop(&mut self, range: RangeInclusive<u32>) {
        let internal_range = self.stack.len() - 1 - *range.end() as usize
            ..=self.stack.len() - 1 - *range.start() as usize;

        for _ in self.stack.drain(internal_range) {}
    }

    // I don't know if we need to know the return type
    pub fn new(
        context: &'b M,
        params: impl IntoIterator<Item = SignlessType>,
        returns: impl IntoIterator<Item = SignlessType>,
        reader: &'a FunctionBody,
    ) -> Self {
        // TODO: Don't panic!
        let locals_reader = reader
            .get_locals_reader()
            .expect("Failed to get locals reader");
        let mut locals = Vec::from_iter(params);
        let mut consts = Vec::new();

        for loc in locals_reader {
            let (count, ty) = loc.expect("Getting local failed");
            let ty = Type::from_wasm(ty).expect("Invalid local type");
            locals.extend(std::iter::repeat(ty).take(count as _));
            consts.extend(
                std::iter::repeat(ty)
                    .map(Value::default_for_type)
                    .take(count as _),
            )
        }

        let num_locals = locals.len() as _;

        let mut out = Self {
            is_done: false,
            stack: locals,
            module: context,
            num_locals,
            consts_to_emit: Some(consts),
            internal: reader
                .get_operators_reader()
                .expect("Failed to get operators reader"),
            current_id: 0,
            control_frames: vec![],
            unreachable: false,
        };

        let id = out.next_id();
        out.control_frames.push(ControlFrame {
            id,
            returns: returns.into_iter().count() as _,
            kind: ControlFrameKind::Function,
        });

        out
    }
}

impl<'a, 'b, M: ModuleContext> Iterator for MicrowasmConv<'a, 'b, M>
where
    for<'any> &'any M::Signature: Into<OpSig>,
{
    type Item = wasmparser::Result<Vec<OperatorFromWasm>>;

    // TODO: We don't need to use vec here, we can maybe use `ArrayVec` or `Option`+`chain`
    fn next(&mut self) -> Option<wasmparser::Result<Vec<OperatorFromWasm>>> {
        macro_rules! to_drop {
            ($block:expr) => {{
                let first_non_local_depth = $block.returns;

                (|| {
                    let last_non_local_depth = (self.stack.len() as u32)
                        .checked_sub(1)?
                        .checked_sub(self.num_locals + 1)?;

                    if first_non_local_depth <= last_non_local_depth {
                        Some(first_non_local_depth..=last_non_local_depth)
                    } else {
                        None
                    }
                })()
            }};
        }

        if self.is_done {
            return None;
        }

        if let Some(consts) = self.consts_to_emit.take() {
            return Some(Ok(consts
                .into_iter()
                .map(|value| Operator::Const(value))
                .collect::<Vec<_>>()));
        }

        if self.unreachable {
            self.unreachable = false;
            let mut depth = 0;

            // `if..then..else`/`br_if` means that there may be branches in which
            // the instruction that caused us to mark this as unreachable to not
            // be executed. Tracking this in the microwasm translation step is
            // very complicated so we just do basic code removal here and leave
            // the removal of uncalled blocks to the backend.
            return Some(Ok(loop {
                let op = match self.internal.read() {
                    Err(e) => return Some(Err(e)),
                    Ok(o) => o,
                };
                match op {
                    WasmOperator::Block { .. }
                    | WasmOperator::Loop { .. }
                    | WasmOperator::If { .. } => {
                        depth += 1;
                    }
                    WasmOperator::Else => {
                        if depth == 0 {
                            let block = self.control_frames.last_mut().expect("Failed");

                            if let ControlFrameKind::If { has_else, .. } = &mut block.kind {
                                *has_else = true;
                            }

                            break vec![Operator::Label((block.id, NameTag::Else))];
                        }
                    }
                    WasmOperator::End => {
                        if depth == 0 {
                            let block = self.control_frames.pop().expect("Failed");

                            if let Some(to_drop) = to_drop!(block) {
                                self.drop(to_drop.clone());
                            }

                            if self.control_frames.is_empty() {
                                self.is_done = true;
                                return None;
                            }

                            let end_label = (block.id, NameTag::End);

                            if let ControlFrameKind::If {
                                has_else: false, ..
                            } = block.kind
                            {
                                self.stack = block.params().unwrap().to_vec();

                                break vec![
                                    Operator::Label((block.id, NameTag::Else)),
                                    Operator::Br {
                                        target: BrTarget::Label(end_label),
                                    },
                                    Operator::Label(end_label),
                                ];
                            } else {
                                break vec![Operator::Label((block.id, NameTag::End))];
                            }
                        } else {
                            depth -= 1;
                        }
                    }
                    _ => {}
                }
            }));
        }

        let op = match self.internal.read() {
            Err(e) => return Some(Err(e)),
            Ok(o) => o,
        };

        let op_sig = self.op_sig(&op);

        self.apply_op(op_sig);

        Some(Ok(match op {
            WasmOperator::Unreachable => {
                self.unreachable = true;
                vec![Operator::Unreachable]
            }
            WasmOperator::Nop => vec![],
            WasmOperator::Block { ty } => {
                let id = self.next_id();
                self.control_frames.push(ControlFrame {
                    id,
                    returns: if ty == wasmparser::Type::EmptyBlockType {
                        0
                    } else {
                        1
                    },
                    kind: ControlFrameKind::Block {
                        needs_end_label: false,
                    },
                });
                vec![Operator::end(
                    self.block_params_with_wasm_type(ty),
                    (id, NameTag::End),
                )]
            }
            WasmOperator::Loop { ty } => {
                let id = self.next_id();
                self.control_frames.push(ControlFrame {
                    id,
                    returns: if ty == wasmparser::Type::EmptyBlockType {
                        0
                    } else {
                        1
                    },
                    kind: ControlFrameKind::Loop,
                });
                let label = (id, NameTag::Header);
                vec![
                    Operator::loop_(self.block_params(), label),
                    Operator::end(self.block_params_with_wasm_type(ty), (id, NameTag::End)),
                    Operator::Br {
                        target: BrTarget::Label(label),
                    },
                    Operator::Label(label),
                ]
            }
            WasmOperator::If { ty } => {
                let id = self.next_id();
                let params = self.block_params();
                self.control_frames.push(ControlFrame {
                    id,
                    returns: if ty == wasmparser::Type::EmptyBlockType {
                        0
                    } else {
                        1
                    },
                    kind: ControlFrameKind::If {
                        params,
                        has_else: false,
                    },
                });
                let (then, else_, end) = (
                    (id, NameTag::Header),
                    (id, NameTag::Else),
                    (id, NameTag::End),
                );
                vec![
                    Operator::block(self.block_params(), then),
                    Operator::block(self.block_params(), else_),
                    Operator::end(self.block_params_with_wasm_type(ty), end),
                    Operator::BrIf {
                        then: BrTarget::Label(then),
                        else_: BrTarget::Label(else_),
                    },
                    Operator::Label(then),
                ]
            }
            WasmOperator::Else => {
                // We don't pop it since we're still in the second block.
                let to_drop = to_drop!(self.control_frames.last().expect("Failed"));
                let block = self.control_frames.last_mut().expect("Failed");

                if let ControlFrameKind::If { has_else, .. } = &mut block.kind {
                    *has_else = true;
                }

                self.stack = block.params().unwrap().to_vec();

                let label = (block.id, NameTag::Else);

                Vec::from_iter(
                    to_drop
                        .into_iter()
                        .map(Operator::Drop)
                        .chain(iter::once(Operator::Br {
                            target: BrTarget::Label((block.id, NameTag::End)),
                        }))
                        .chain(iter::once(Operator::Label(label))),
                )
            }
            WasmOperator::End => {
                let block = self.control_frames.pop().expect("Failed");

                let to_drop = to_drop!(block);

                if let Some(to_drop) = &to_drop {
                    self.drop(to_drop.clone());
                }

                if let ControlFrameKind::If {
                    has_else: false, ..
                } = block.kind
                {
                    let else_ = (block.id, NameTag::Else);
                    let end = (block.id, NameTag::End);

                    self.stack = block.params().unwrap().to_vec();

                    to_drop
                        .map(Operator::Drop)
                        .into_iter()
                        .chain(vec![
                            Operator::Br {
                                target: BrTarget::Label(else_),
                            },
                            Operator::Label(else_),
                            Operator::Br {
                                target: BrTarget::Label(end),
                            },
                            Operator::Label(end),
                        ])
                        .collect()
                } else {
                    Vec::from_iter(if self.control_frames.is_empty() {
                        self.is_done = true;

                        None.into_iter()
                            .chain(Some(Operator::Br {
                                target: BrTarget::Return,
                            }))
                            .chain(None)
                    } else if block.needs_end_label() {
                        let label = (block.id, NameTag::End);

                        to_drop
                            .map(Operator::Drop)
                            .into_iter()
                            .chain(Some(Operator::Br {
                                target: BrTarget::Label(label),
                            }))
                            .chain(Some(Operator::Label(label)))
                    } else {
                        to_drop
                            .map(Operator::Drop)
                            .into_iter()
                            .chain(None)
                            .into_iter()
                            .chain(None)
                    })
                }
            }
            // TODO: If we're breaking out of the function block we want
            //       to drop locals too (see code for `WasmOperator::End`)
            WasmOperator::Br { relative_depth } => {
                self.unreachable = true;
                let to_drop = to_drop!(self.nth_block(relative_depth as _));

                let block = self.nth_block_mut(relative_depth as _);
                block.mark_branched_to();
                Vec::from_iter(to_drop.into_iter().map(Operator::Drop).chain(iter::once(
                    Operator::Br {
                        target: block.br_target(),
                    },
                )))
            }
            WasmOperator::BrIf { relative_depth } => {
                let to_drop = to_drop!(self.nth_block(relative_depth as _));

                let label = (self.next_id(), NameTag::Header);
                let params = self.block_params();
                let block = self.nth_block_mut(relative_depth as _);
                block.mark_branched_to();

                if let Some(_to_drop) = to_drop {
                    // TODO: We want to generate an intermediate block here, but that might cause
                    //       us to generate a spurious `jmp`.
                    unimplemented!()
                } else {
                    vec![
                        Operator::block(params, label),
                        Operator::BrIf {
                            then: block.br_target(),
                            else_: BrTarget::Label(label),
                        },
                        Operator::Label(label),
                    ]
                }
            }
            WasmOperator::BrTable { .. } => unimplemented!(),
            WasmOperator::Return => {
                self.unreachable = true;

                let block = self.function_block();
                let to_drop = to_drop!(block);

                Vec::from_iter(to_drop.into_iter().map(Operator::Drop).chain(iter::once(
                    Operator::Br {
                        target: block.br_target(),
                    },
                )))
            }
            WasmOperator::Call { function_index } => vec![Operator::Call { function_index }],
            WasmOperator::CallIndirect { index, table_index } => vec![Operator::CallIndirect {
                type_index: index,
                table_index,
            }],
            WasmOperator::Drop => vec![Operator::Drop(0..=0)],
            WasmOperator::Select => vec![Operator::Select],

            WasmOperator::GetLocal { local_index } => {
                // TODO: `- 1` because we apply the stack difference _before_ this point
                let depth = self.local_depth(local_index) - 1;
                vec![Operator::Pick { depth }]
            }
            WasmOperator::SetLocal { local_index } => {
                // TODO: `+ 1` because we apply the stack difference _before_ this point
                let depth = self.local_depth(local_index) + 1;
                vec![Operator::Swap { depth }, Operator::Drop(0..=0)]
            }
            WasmOperator::TeeLocal { local_index } => {
                let depth = self.local_depth(local_index);
                vec![
                    Operator::Swap { depth },
                    Operator::Drop(0..=0),
                    Operator::Pick { depth: depth - 1 },
                ]
            }

            WasmOperator::I32Load { memarg } => vec![Operator::Load { ty: I32, memarg }],
            WasmOperator::I64Load { memarg } => vec![Operator::Load { ty: I64, memarg }],
            WasmOperator::F32Load { memarg } => vec![Operator::Load { ty: F32, memarg }],
            WasmOperator::F64Load { memarg } => vec![Operator::Load { ty: F64, memarg }],
            WasmOperator::I32Load8S { memarg } => vec![Operator::Load8 {
                ty: sint::I32,
                memarg,
            }],
            WasmOperator::I32Load8U { memarg } => vec![Operator::Load8 {
                ty: sint::U32,
                memarg,
            }],
            WasmOperator::I32Load16S { memarg } => vec![Operator::Load16 {
                ty: sint::I32,
                memarg,
            }],
            WasmOperator::I32Load16U { memarg } => vec![Operator::Load16 {
                ty: sint::U32,
                memarg,
            }],
            WasmOperator::I64Load8S { memarg } => vec![Operator::Load8 {
                ty: sint::I64,
                memarg,
            }],
            WasmOperator::I64Load8U { memarg } => vec![Operator::Load8 {
                ty: sint::U64,
                memarg,
            }],
            WasmOperator::I64Load16S { memarg } => vec![Operator::Load16 {
                ty: sint::I64,
                memarg,
            }],
            WasmOperator::I64Load16U { memarg } => vec![Operator::Load16 {
                ty: sint::U64,
                memarg,
            }],
            WasmOperator::I64Load32S { memarg } => vec![Operator::Load32 {
                sign: Signedness::Signed,
                memarg,
            }],
            WasmOperator::I64Load32U { memarg } => vec![Operator::Load32 {
                sign: Signedness::Unsigned,
                memarg,
            }],

            WasmOperator::I32Store { memarg } => vec![Operator::Store { ty: I32, memarg }],
            WasmOperator::I64Store { memarg } => vec![Operator::Store { ty: I64, memarg }],
            WasmOperator::F32Store { memarg } => vec![Operator::Store { ty: F32, memarg }],
            WasmOperator::F64Store { memarg } => vec![Operator::Store { ty: F64, memarg }],

            WasmOperator::I32Store8 { memarg } => vec![Operator::Store8 {
                ty: Size::_32,
                memarg,
            }],
            WasmOperator::I32Store16 { memarg } => vec![Operator::Store16 {
                ty: Size::_32,
                memarg,
            }],
            WasmOperator::I64Store8 { memarg } => vec![Operator::Store8 {
                ty: Size::_64,
                memarg,
            }],
            WasmOperator::I64Store16 { memarg } => vec![Operator::Store16 {
                ty: Size::_64,
                memarg,
            }],
            WasmOperator::I64Store32 { memarg } => vec![Operator::Store32 { memarg }],
            WasmOperator::MemorySize { reserved } => vec![Operator::MemorySize { reserved }],
            WasmOperator::MemoryGrow { reserved } => vec![Operator::MemoryGrow { reserved }],
            WasmOperator::I32Const { value } => vec![Operator::Const(Value::I32(value))],
            WasmOperator::I64Const { value } => vec![Operator::Const(Value::I64(value))],
            WasmOperator::F32Const { value } => vec![Operator::Const(Value::F32(value))],
            WasmOperator::F64Const { value } => vec![Operator::Const(Value::F64(value))],
            WasmOperator::RefNull => unimplemented!(),
            WasmOperator::RefIsNull => unimplemented!(),
            WasmOperator::I32Eqz => vec![Operator::Eqz(Size::_32)],
            WasmOperator::I32Eq => vec![Operator::Eq(I32)],
            WasmOperator::I32Ne => vec![Operator::Ne(I32)],
            WasmOperator::I32LtS => vec![Operator::Lt(SI32)],
            WasmOperator::I32LtU => vec![Operator::Lt(SU32)],
            WasmOperator::I32GtS => vec![Operator::Gt(SI32)],
            WasmOperator::I32GtU => vec![Operator::Gt(SU32)],
            WasmOperator::I32LeS => vec![Operator::Le(SI32)],
            WasmOperator::I32LeU => vec![Operator::Le(SU32)],
            WasmOperator::I32GeS => vec![Operator::Ge(SI32)],
            WasmOperator::I32GeU => vec![Operator::Ge(SU32)],
            WasmOperator::I64Eqz => vec![Operator::Eqz(Size::_64)],
            WasmOperator::I64Eq => vec![Operator::Eq(I64)],
            WasmOperator::I64Ne => vec![Operator::Ne(I64)],
            WasmOperator::I64LtS => vec![Operator::Lt(SI64)],
            WasmOperator::I64LtU => vec![Operator::Lt(SU64)],
            WasmOperator::I64GtS => vec![Operator::Gt(SI64)],
            WasmOperator::I64GtU => vec![Operator::Gt(SU64)],
            WasmOperator::I64LeS => vec![Operator::Le(SI64)],
            WasmOperator::I64LeU => vec![Operator::Le(SU64)],
            WasmOperator::I64GeS => vec![Operator::Ge(SI64)],
            WasmOperator::I64GeU => vec![Operator::Ge(SU64)],
            WasmOperator::F32Eq => vec![Operator::Eq(F32)],
            WasmOperator::F32Ne => vec![Operator::Ne(F32)],
            WasmOperator::F32Lt => vec![Operator::Lt(SF32)],
            WasmOperator::F32Gt => vec![Operator::Gt(SF32)],
            WasmOperator::F32Le => vec![Operator::Le(SF32)],
            WasmOperator::F32Ge => vec![Operator::Ge(SF32)],
            WasmOperator::F64Eq => vec![Operator::Eq(F64)],
            WasmOperator::F64Ne => vec![Operator::Ne(F64)],
            WasmOperator::F64Lt => vec![Operator::Lt(SF64)],
            WasmOperator::F64Gt => vec![Operator::Gt(SF64)],
            WasmOperator::F64Le => vec![Operator::Le(SF64)],
            WasmOperator::F64Ge => vec![Operator::Ge(SF64)],
            WasmOperator::I32Clz => vec![Operator::Clz(Size::_32)],
            WasmOperator::I32Ctz => vec![Operator::Ctz(Size::_32)],
            WasmOperator::I32Popcnt => vec![Operator::Popcnt(Size::_32)],
            WasmOperator::I32Add => vec![Operator::Add(I32)],
            WasmOperator::I32Sub => vec![Operator::Sub(I32)],
            WasmOperator::I32Mul => vec![Operator::Mul(I32)],
            WasmOperator::I32DivS => vec![Operator::Div(SI32)],
            WasmOperator::I32DivU => vec![Operator::Div(SU32)],
            WasmOperator::I32RemS => vec![Operator::Rem(sint::I32)],
            WasmOperator::I32RemU => vec![Operator::Rem(sint::U32)],
            WasmOperator::I32And => vec![Operator::And(Size::_32)],
            WasmOperator::I32Or => vec![Operator::Or(Size::_32)],
            WasmOperator::I32Xor => vec![Operator::Xor(Size::_32)],
            WasmOperator::I32Shl => vec![Operator::Shl(Size::_32)],
            WasmOperator::I32ShrS => vec![Operator::Shr(sint::I32)],
            WasmOperator::I32ShrU => vec![Operator::Shr(sint::U32)],
            WasmOperator::I32Rotl => vec![Operator::Rotl(Size::_32)],
            WasmOperator::I32Rotr => vec![Operator::Rotr(Size::_32)],
            WasmOperator::I64Clz => vec![Operator::Clz(Size::_64)],
            WasmOperator::I64Ctz => vec![Operator::Ctz(Size::_64)],
            WasmOperator::I64Popcnt => vec![Operator::Popcnt(Size::_64)],
            WasmOperator::I64Add => vec![Operator::Add(I64)],
            WasmOperator::I64Sub => vec![Operator::Sub(I64)],
            WasmOperator::I64Mul => vec![Operator::Mul(I64)],
            WasmOperator::I64DivS => vec![Operator::Div(SI64)],
            WasmOperator::I64DivU => vec![Operator::Div(SU64)],
            WasmOperator::I64RemS => vec![Operator::Rem(sint::I64)],
            WasmOperator::I64RemU => vec![Operator::Rem(sint::U64)],
            WasmOperator::I64And => vec![Operator::And(Size::_64)],
            WasmOperator::I64Or => vec![Operator::Or(Size::_64)],
            WasmOperator::I64Xor => vec![Operator::Xor(Size::_64)],
            WasmOperator::I64Shl => vec![Operator::Shl(Size::_64)],
            WasmOperator::I64ShrS => vec![Operator::Shr(sint::I64)],
            WasmOperator::I64ShrU => vec![Operator::Shr(sint::U64)],
            WasmOperator::I64Rotl => vec![Operator::Rotl(Size::_64)],
            WasmOperator::I64Rotr => vec![Operator::Rotr(Size::_64)],
            WasmOperator::F32Abs => vec![Operator::Abs(Size::_32)],
            WasmOperator::F32Neg => vec![Operator::Neg(Size::_32)],
            WasmOperator::F32Ceil => vec![Operator::Ceil(Size::_32)],
            WasmOperator::F32Floor => vec![Operator::Floor(Size::_32)],
            WasmOperator::F32Trunc => vec![Operator::Trunc(Size::_32)],
            WasmOperator::F32Nearest => vec![Operator::Nearest(Size::_32)],
            WasmOperator::F32Sqrt => vec![Operator::Sqrt(Size::_32)],
            WasmOperator::F32Add => vec![Operator::Add(F32)],
            WasmOperator::F32Sub => vec![Operator::Sub(F32)],
            WasmOperator::F32Mul => vec![Operator::Mul(F32)],
            WasmOperator::F32Div => vec![Operator::Div(SF32)],
            WasmOperator::F32Min => vec![Operator::Min(Size::_32)],
            WasmOperator::F32Max => vec![Operator::Max(Size::_32)],
            WasmOperator::F32Copysign => vec![Operator::Copysign(Size::_32)],
            WasmOperator::F64Abs => vec![Operator::Abs(Size::_64)],
            WasmOperator::F64Neg => vec![Operator::Neg(Size::_64)],
            WasmOperator::F64Ceil => vec![Operator::Ceil(Size::_64)],
            WasmOperator::F64Floor => vec![Operator::Floor(Size::_64)],
            WasmOperator::F64Trunc => vec![Operator::Trunc(Size::_64)],
            WasmOperator::F64Nearest => vec![Operator::Nearest(Size::_64)],
            WasmOperator::F64Sqrt => vec![Operator::Sqrt(Size::_64)],
            WasmOperator::F64Add => vec![Operator::Add(F64)],
            WasmOperator::F64Sub => vec![Operator::Sub(F64)],
            WasmOperator::F64Mul => vec![Operator::Mul(F64)],
            WasmOperator::F64Div => vec![Operator::Div(SF64)],
            WasmOperator::F64Min => vec![Operator::Min(Size::_64)],
            WasmOperator::F64Max => vec![Operator::Max(Size::_64)],
            WasmOperator::F64Copysign => vec![Operator::Copysign(Size::_64)],
            WasmOperator::I32WrapI64 => unimplemented!(),
            WasmOperator::I32TruncSF32 => unimplemented!(),
            WasmOperator::I32TruncUF32 => unimplemented!(),
            WasmOperator::I32TruncSF64 => unimplemented!(),
            WasmOperator::I32TruncUF64 => unimplemented!(),
            WasmOperator::I64ExtendSI32 => unimplemented!(),
            WasmOperator::I64ExtendUI32 => unimplemented!(),
            WasmOperator::I64TruncSF32 => unimplemented!(),
            WasmOperator::I64TruncUF32 => unimplemented!(),
            WasmOperator::I64TruncSF64 => unimplemented!(),
            WasmOperator::I64TruncUF64 => unimplemented!(),
            WasmOperator::F32ConvertSI32 => unimplemented!(),
            WasmOperator::F32ConvertUI32 => unimplemented!(),
            WasmOperator::F32ConvertSI64 => unimplemented!(),
            WasmOperator::F32ConvertUI64 => unimplemented!(),
            WasmOperator::F32DemoteF64 => unimplemented!(),
            WasmOperator::F64ConvertSI32 => unimplemented!(),
            WasmOperator::F64ConvertUI32 => unimplemented!(),
            WasmOperator::F64ConvertSI64 => unimplemented!(),
            WasmOperator::F64ConvertUI64 => unimplemented!(),
            WasmOperator::F64PromoteF32 => unimplemented!(),
            WasmOperator::I32ReinterpretF32 => unimplemented!(),
            WasmOperator::I64ReinterpretF64 => unimplemented!(),
            WasmOperator::F32ReinterpretI32 => unimplemented!(),
            WasmOperator::F64ReinterpretI64 => unimplemented!(),
            WasmOperator::I32Extend8S => unimplemented!(),
            WasmOperator::I32Extend16S => unimplemented!(),
            WasmOperator::I64Extend8S => unimplemented!(),
            WasmOperator::I64Extend16S => unimplemented!(),
            WasmOperator::I64Extend32S => unimplemented!(),

            // 0xFC operators
            // Non-trapping Float-to-int Conversions
            WasmOperator::I32TruncSSatF32 => unimplemented!(),
            WasmOperator::I32TruncUSatF32 => unimplemented!(),
            WasmOperator::I32TruncSSatF64 => unimplemented!(),
            WasmOperator::I32TruncUSatF64 => unimplemented!(),
            WasmOperator::I64TruncSSatF32 => unimplemented!(),
            WasmOperator::I64TruncUSatF32 => unimplemented!(),
            WasmOperator::I64TruncSSatF64 => unimplemented!(),
            WasmOperator::I64TruncUSatF64 => unimplemented!(),

            _ => unimplemented!(),
        }))
    }
}
