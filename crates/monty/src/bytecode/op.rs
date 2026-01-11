//! Opcode definitions for the bytecode VM.
//!
//! Bytecode is stored as raw `Vec<u8>` for cache efficiency. Opcodes are defined as
//! constants with no data - operands are fetched separately from the byte stream.
//!
//! # Operand Encoding
//!
//! - No suffix, 0 bytes: `BINARY_ADD`, `POP`, `LOAD_NONE`
//! - No suffix, 1 byte (u8/i8): `LOAD_LOCAL`, `STORE_LOCAL`, `LOAD_SMALL_INT`
//! - `W` suffix, 2 bytes (u16/i16): `LOAD_LOCAL_W`, `JUMP`, `LOAD_CONST`
//! - Compound (multiple operands): `CALL_FUNCTION_KW` (u8 + u8), `MAKE_CLOSURE` (u16 + u8)

/// Simple wrapper for a u8 used to make types clear when using opcodes.
///
/// Should be completely transparent and removed at compile time.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Opcode(u8);

impl From<u8> for Opcode {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl From<Opcode> for u8 {
    fn from(value: Opcode) -> Self {
        value.0
    }
}

// === Stack Operations (no operand) ===
/// Discard top of stack.
pub const POP: Opcode = Opcode(0);
/// Duplicate top of stack.
pub const DUP: Opcode = Opcode(1);
/// Swap top two: [a, b] -> [b, a].
pub const ROT2: Opcode = Opcode(2);
/// Rotate top three: [a, b, c] -> [c, a, b].
pub const ROT3: Opcode = Opcode(3);

// === Constants & Literals ===
/// Push constant from pool. Operand: u16 const_id.
pub const LOAD_CONST: Opcode = Opcode(4);
/// Push None.
pub const LOAD_NONE: Opcode = Opcode(5);
/// Push True.
pub const LOAD_TRUE: Opcode = Opcode(6);
/// Push False.
pub const LOAD_FALSE: Opcode = Opcode(7);
/// Push small integer (-128 to 127). Operand: i8.
pub const LOAD_SMALL_INT: Opcode = Opcode(8);

// === Variables ===
// Specialized no-operand versions for common slots (hot path)
/// Push local slot 0 (often 'self').
pub const LOAD_LOCAL0: Opcode = Opcode(9);
/// Push local slot 1.
pub const LOAD_LOCAL1: Opcode = Opcode(10);
/// Push local slot 2.
pub const LOAD_LOCAL2: Opcode = Opcode(11);
/// Push local slot 3.
pub const LOAD_LOCAL3: Opcode = Opcode(12);
// General versions with operand
/// Push local variable. Operand: u8 slot.
pub const LOAD_LOCAL: Opcode = Opcode(13);
/// Push local (wide, slot > 255). Operand: u16 slot.
pub const LOAD_LOCAL_W: Opcode = Opcode(14);
/// Pop and store to local. Operand: u8 slot.
pub const STORE_LOCAL: Opcode = Opcode(15);
/// Store local (wide). Operand: u16 slot.
pub const STORE_LOCAL_W: Opcode = Opcode(16);
/// Push from global namespace. Operand: u16 slot.
pub const LOAD_GLOBAL: Opcode = Opcode(17);
/// Store to global. Operand: u16 slot.
pub const STORE_GLOBAL: Opcode = Opcode(18);
/// Load from closure cell. Operand: u16 slot.
pub const LOAD_CELL: Opcode = Opcode(19);
/// Store to closure cell. Operand: u16 slot.
pub const STORE_CELL: Opcode = Opcode(20);
/// Delete local variable. Operand: u8 slot.
pub const DELETE_LOCAL: Opcode = Opcode(21);

// === Binary Operations (no operand) ===
/// Add: a + b.
pub const BINARY_ADD: Opcode = Opcode(22);
/// Subtract: a - b.
pub const BINARY_SUB: Opcode = Opcode(23);
/// Multiply: a * b.
pub const BINARY_MUL: Opcode = Opcode(24);
/// Divide: a / b.
pub const BINARY_DIV: Opcode = Opcode(25);
/// Floor divide: a // b.
pub const BINARY_FLOOR_DIV: Opcode = Opcode(26);
/// Modulo: a % b.
pub const BINARY_MOD: Opcode = Opcode(27);
/// Power: a ** b.
pub const BINARY_POW: Opcode = Opcode(28);
/// Bitwise AND: a & b.
pub const BINARY_AND: Opcode = Opcode(29);
/// Bitwise OR: a | b.
pub const BINARY_OR: Opcode = Opcode(30);
/// Bitwise XOR: a ^ b.
pub const BINARY_XOR: Opcode = Opcode(31);
/// Left shift: a << b.
pub const BINARY_LSHIFT: Opcode = Opcode(32);
/// Right shift: a >> b.
pub const BINARY_RSHIFT: Opcode = Opcode(33);
/// Matrix multiply: a @ b.
pub const BINARY_MAT_MUL: Opcode = Opcode(34);

// === Comparison Operations (no operand) ===
/// Equal: a == b.
pub const COMPARE_EQ: Opcode = Opcode(35);
/// Not equal: a != b.
pub const COMPARE_NE: Opcode = Opcode(36);
/// Less than: a < b.
pub const COMPARE_LT: Opcode = Opcode(37);
/// Less than or equal: a <= b.
pub const COMPARE_LE: Opcode = Opcode(38);
/// Greater than: a > b.
pub const COMPARE_GT: Opcode = Opcode(39);
/// Greater than or equal: a >= b.
pub const COMPARE_GE: Opcode = Opcode(40);
/// Identity: a is b.
pub const COMPARE_IS: Opcode = Opcode(41);
/// Not identity: a is not b.
pub const COMPARE_IS_NOT: Opcode = Opcode(42);
/// Membership: a in b.
pub const COMPARE_IN: Opcode = Opcode(43);
/// Not membership: a not in b.
pub const COMPARE_NOT_IN: Opcode = Opcode(44);
/// Modulo equality: a % b == k (operand: u16 constant index for k).
///
/// This is an optimization for patterns like `x % 3 == 0` which are common
/// in Python code. Pops b then a, computes `a % b`, then compares with k.
pub const COMPARE_MOD_EQ: Opcode = Opcode(45);

// === Unary Operations (no operand) ===
/// Logical not: not a.
pub const UNARY_NOT: Opcode = Opcode(46);
/// Negation: -a.
pub const UNARY_NEG: Opcode = Opcode(47);
/// Positive: +a.
pub const UNARY_POS: Opcode = Opcode(48);
/// Bitwise invert: ~a.
pub const UNARY_INVERT: Opcode = Opcode(49);

// === In-place Operations (no operand) ===
/// In-place add: a += b.
pub const INPLACE_ADD: Opcode = Opcode(50);
/// In-place subtract: a -= b.
pub const INPLACE_SUB: Opcode = Opcode(51);
/// In-place multiply: a *= b.
pub const INPLACE_MUL: Opcode = Opcode(52);
/// In-place divide: a /= b.
pub const INPLACE_DIV: Opcode = Opcode(53);
/// In-place floor divide: a //= b.
pub const INPLACE_FLOOR_DIV: Opcode = Opcode(54);
/// In-place modulo: a %= b.
pub const INPLACE_MOD: Opcode = Opcode(55);
/// In-place power: a **= b.
pub const INPLACE_POW: Opcode = Opcode(56);
/// In-place bitwise AND: a &= b.
pub const INPLACE_AND: Opcode = Opcode(57);
/// In-place bitwise OR: a |= b.
pub const INPLACE_OR: Opcode = Opcode(58);
/// In-place bitwise XOR: a ^= b.
pub const INPLACE_XOR: Opcode = Opcode(59);
/// In-place left shift: a <<= b.
pub const INPLACE_LSHIFT: Opcode = Opcode(60);
/// In-place right shift: a >>= b.
pub const INPLACE_RSHIFT: Opcode = Opcode(61);

// === Collection Building ===
/// Pop n items, build list. Operand: u16 count.
pub const BUILD_LIST: Opcode = Opcode(62);
/// Pop n items, build tuple. Operand: u16 count.
pub const BUILD_TUPLE: Opcode = Opcode(63);
/// Pop 2n items (k/v pairs), build dict. Operand: u16 count.
pub const BUILD_DICT: Opcode = Opcode(64);
/// Pop n items, build set. Operand: u16 count.
pub const BUILD_SET: Opcode = Opcode(65);
/// Format a value for f-string interpolation. Operand: u8 flags.
///
/// Flags encoding:
/// - bits 0-1: conversion (0=none, 1=str, 2=repr, 3=ascii)
/// - bit 2: has format spec on stack (pop fmt_spec first, then value)
/// - bit 3: has static format spec (operand includes u16 const_id after flags)
///
/// Pops the value (and optionally format spec), pushes the formatted string.
pub const FORMAT_VALUE: Opcode = Opcode(66);
/// Pop n parts, concatenate for f-string. Operand: u16 count.
pub const BUILD_FSTRING: Opcode = Opcode(67);
/// Pop iterable, pop list, extend list with iterable items.
///
/// Used for `*args` unpacking: builds a list of positional args,
/// then extends it with unpacked iterables.
pub const LIST_EXTEND: Opcode = Opcode(68);
/// Pop TOS (list), push tuple containing the same elements.
///
/// Used after building the args list to create the final args tuple
/// for `CALL_FUNCTION_EX`.
pub const LIST_TO_TUPLE: Opcode = Opcode(69);
/// Pop mapping, pop dict, update dict with mapping. Operand: u16 func_name_id.
///
/// Used for `**kwargs` unpacking. The func_name_id is used for error messages
/// when the mapping contains non-string keys.
pub const DICT_MERGE: Opcode = Opcode(70);

// === Subscript & Attribute ===
/// a[b]: pop index, pop obj, push result.
pub const BINARY_SUBSCR: Opcode = Opcode(71);
/// a[b] = c: pop value, pop index, pop obj.
pub const STORE_SUBSCR: Opcode = Opcode(72);
/// del a[b]: pop index, pop obj.
pub const DELETE_SUBSCR: Opcode = Opcode(73);
/// Pop obj, push obj.attr. Operand: u16 name_id.
pub const LOAD_ATTR: Opcode = Opcode(74);
/// Pop value, pop obj, set obj.attr. Operand: u16 name_id.
pub const STORE_ATTR: Opcode = Opcode(75);
/// Pop obj, delete obj.attr. Operand: u16 name_id.
pub const DELETE_ATTR: Opcode = Opcode(76);

// === Function Calls ===
/// Call TOS with n positional args. Operand: u8 arg_count.
pub const CALL_FUNCTION: Opcode = Opcode(77);
/// Call with positional and keyword args.
///
/// Operands: u8 pos_count, u8 kw_count, then kw_count u16 name indices.
///
/// Stack: [callable, pos_args..., kw_values...]
/// After the two count bytes, there are kw_count little-endian u16 values,
/// each being a StringId index for the corresponding keyword argument name.
pub const CALL_FUNCTION_KW: Opcode = Opcode(78);
/// Call method. Operands: u16 name_id, u8 arg_count.
pub const CALL_METHOD: Opcode = Opcode(79);
/// External call (pauses VM). Operands: u16 func_id, u8 arg_count.
pub const CALL_EXTERNAL: Opcode = Opcode(80);
/// Call with *args tuple and **kwargs dict. Operand: u8 flags.
///
/// Flags:
/// - bit 0: has kwargs dict on stack
///
/// Stack layout (bottom to top):
/// - callable
/// - args tuple
/// - kwargs dict (if flag bit 0 set)
///
/// Used for calls with `*args` and/or `**kwargs` unpacking.
pub const CALL_FUNCTION_EX: Opcode = Opcode(81);

// === Control Flow ===
/// Unconditional relative jump. Operand: i16 offset.
pub const JUMP: Opcode = Opcode(82);
/// Jump if TOS truthy, always pop. Operand: i16 offset.
pub const JUMP_IF_TRUE: Opcode = Opcode(83);
/// Jump if TOS falsy, always pop. Operand: i16 offset.
pub const JUMP_IF_FALSE: Opcode = Opcode(84);
/// Jump if TOS truthy (keep), else pop. Operand: i16 offset.
pub const JUMP_IF_TRUE_OR_POP: Opcode = Opcode(85);
/// Jump if TOS falsy (keep), else pop. Operand: i16 offset.
pub const JUMP_IF_FALSE_OR_POP: Opcode = Opcode(86);

// === Iteration ===
/// Convert TOS to iterator.
pub const GET_ITER: Opcode = Opcode(87);
/// Advance iterator or jump to end. Operand: i16 offset.
pub const FOR_ITER: Opcode = Opcode(88);

// === Function Definition ===
/// Create function object. Operand: u16 func_id.
pub const MAKE_FUNCTION: Opcode = Opcode(89);
/// Create closure. Operands: u16 func_id, u8 cell_count.
pub const MAKE_CLOSURE: Opcode = Opcode(90);

// === Exception Handling ===
// Note: No SetupTry/PopExceptHandler - we use static exception_table
/// Raise TOS as exception.
pub const RAISE: Opcode = Opcode(91);
/// Raise TOS from TOS-1.
pub const RAISE_FROM: Opcode = Opcode(92);
/// Re-raise current exception (bare `raise`).
pub const RERAISE: Opcode = Opcode(93);
/// Clear current_exception when exiting except block.
pub const CLEAR_EXCEPTION: Opcode = Opcode(94);
/// Check if exception matches type for except clause.
///
/// Stack: [..., exception, exc_type] -> [..., exception, bool]
/// Validates that exc_type is a valid exception type (ExcType or tuple of ExcTypes).
/// If invalid, raises TypeError. If valid, pushes True if exception matches, else False.
pub const CHECK_EXC_MATCH: Opcode = Opcode(95);

// === Return ===
/// Return TOS from function.
pub const RETURN_VALUE: Opcode = Opcode(96);

// === Unpacking ===
/// Unpack TOS into n values. Operand: u8 count.
pub const UNPACK_SEQUENCE: Opcode = Opcode(97);
/// Unpack with *rest. Operands: u8 before, u8 after.
pub const UNPACK_EX: Opcode = Opcode(98);

// === Special ===
/// No operation (for patching/alignment).
pub const NOP: Opcode = Opcode(99);
