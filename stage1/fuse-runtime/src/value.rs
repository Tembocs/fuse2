use std::fmt::Write;
use std::ptr;
use std::slice;
use std::collections::VecDeque;

pub type FuseHandle = *mut FuseValue;

pub type FuseDestructor = Option<unsafe extern "C" fn(FuseHandle)>;

pub struct FuseValue {
    released: bool,
    kind: ValueKind,
}

enum ValueKind {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    List(Vec<FuseHandle>),
    Channel(ChannelValue),
    Shared(FuseHandle),
    Data(DataValue),
    Option(Option<FuseHandle>),
    Result { is_ok: bool, value: FuseHandle },
    Unit,
}

struct ChannelValue {
    items: VecDeque<FuseHandle>,
    pending: VecDeque<FuseHandle>,
    capacity: Option<usize>,
}

struct DataValue {
    type_name: String,
    fields: Vec<FuseHandle>,
    destructor: FuseDestructor,
}

impl FuseValue {
    fn new(kind: ValueKind) -> FuseHandle {
        Box::into_raw(Box::new(Self {
            released: false,
            kind,
        }))
    }
}

fn read_utf8(ptr: *const u8, len: usize) -> String {
    if ptr.is_null() || len == 0 {
        return String::new();
    }
    let bytes = unsafe { slice::from_raw_parts(ptr, len) };
    String::from_utf8_lossy(bytes).into_owned()
}

unsafe fn value_ref<'a>(handle: FuseHandle) -> &'a FuseValue {
    unsafe { handle.as_ref() }.expect("runtime received null Fuse handle")
}

unsafe fn value_mut<'a>(handle: FuseHandle) -> &'a mut FuseValue {
    unsafe { handle.as_mut() }.expect("runtime received null Fuse handle")
}

unsafe fn clone_to_string(handle: FuseHandle) -> String {
    match &unsafe { value_ref(handle) }.kind {
        ValueKind::Int(value) => value.to_string(),
        ValueKind::Float(value) => value.to_string(),
        ValueKind::Bool(value) => {
            if *value {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        ValueKind::String(value) => value.clone(),
        ValueKind::List(items) => {
            let mut rendered = String::from("[");
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    rendered.push_str(", ");
                }
                rendered.push_str(&unsafe { clone_to_string(*item) });
            }
            rendered.push(']');
            rendered
        }
        ValueKind::Channel(_) => "Chan(..)".to_string(),
        ValueKind::Shared(value) => unsafe { clone_to_string(*value) },
        ValueKind::Data(data) => {
            let mut rendered = String::new();
            let _ = write!(&mut rendered, "{}(", data.type_name);
            for (index, field) in data.fields.iter().enumerate() {
                if index > 0 {
                    rendered.push_str(", ");
                }
                rendered.push_str(&unsafe { clone_to_string(*field) });
            }
            rendered.push(')');
            rendered
        }
        ValueKind::Option(Some(value)) => format!("Some({})", unsafe { clone_to_string(*value) }),
        ValueKind::Option(None) => "None".to_string(),
        ValueKind::Result { is_ok, value } => {
            let tag = if *is_ok { "Ok" } else { "Err" };
            format!("{tag}({})", unsafe { clone_to_string(*value) })
        }
        ValueKind::Unit => "Unit".to_string(),
    }
}

fn numeric_binary(lhs: FuseHandle, rhs: FuseHandle, op: fn(i64, i64) -> i64) -> FuseHandle {
    unsafe {
        match (&value_ref(lhs).kind, &value_ref(rhs).kind) {
            (ValueKind::Int(left), ValueKind::Int(right)) => fuse_int(op(*left, *right)),
            _ => fuse_unit(),
        }
    }
}

fn numeric_compare(lhs: FuseHandle, rhs: FuseHandle, op: fn(i64, i64) -> bool) -> FuseHandle {
    unsafe {
        match (&value_ref(lhs).kind, &value_ref(rhs).kind) {
            (ValueKind::Int(left), ValueKind::Int(right)) => fuse_bool(op(*left, *right)),
            _ => fuse_bool(false),
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_unit() -> FuseHandle {
    FuseValue::new(ValueKind::Unit)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_int(value: i64) -> FuseHandle {
    FuseValue::new(ValueKind::Int(value))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_float(value: f64) -> FuseHandle {
    FuseValue::new(ValueKind::Float(value))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_bool(value: bool) -> FuseHandle {
    FuseValue::new(ValueKind::Bool(value))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_string_new_utf8(ptr: *const u8, len: usize) -> FuseHandle {
    FuseValue::new(ValueKind::String(read_utf8(ptr, len)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_to_string(handle: FuseHandle) -> FuseHandle {
    FuseValue::new(ValueKind::String(unsafe { clone_to_string(handle) }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_concat(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    let mut value = unsafe { clone_to_string(lhs) };
    value.push_str(&unsafe { clone_to_string(rhs) });
    FuseValue::new(ValueKind::String(value))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_to_upper(handle: FuseHandle) -> FuseHandle {
    let value = unsafe { clone_to_string(handle) }.to_uppercase();
    FuseValue::new(ValueKind::String(value))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_string_is_empty(handle: FuseHandle) -> FuseHandle {
    let empty = unsafe { clone_to_string(handle) }.is_empty();
    unsafe { fuse_bool(empty) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_add(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    unsafe {
        match (&value_ref(lhs).kind, &value_ref(rhs).kind) {
            (ValueKind::Int(left), ValueKind::Int(right)) => fuse_int(left + right),
            _ => fuse_concat(lhs, rhs),
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_sub(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    numeric_binary(lhs, rhs, |left, right| left - right)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_mul(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    numeric_binary(lhs, rhs, |left, right| left * right)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_div(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    unsafe {
        match (&value_ref(lhs).kind, &value_ref(rhs).kind) {
            (ValueKind::Int(left), ValueKind::Int(right)) if *right != 0 => fuse_int(left / right),
            (ValueKind::Float(left), ValueKind::Float(right)) if *right != 0.0 => fuse_float(left / right),
            _ => fuse_unit(),
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_mod(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    numeric_binary(lhs, rhs, |left, right| left % right)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_eq(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    let equals = unsafe { clone_to_string(lhs) == clone_to_string(rhs) };
    unsafe { fuse_bool(equals) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_data_eq(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    unsafe { fuse_eq(lhs, rhs) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_lt(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    numeric_compare(lhs, rhs, |left, right| left < right)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_le(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    numeric_compare(lhs, rhs, |left, right| left <= right)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_gt(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    numeric_compare(lhs, rhs, |left, right| left > right)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_ge(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    numeric_compare(lhs, rhs, |left, right| left >= right)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_is_truthy(handle: FuseHandle) -> bool {
    unsafe {
        match &value_ref(handle).kind {
            ValueKind::Bool(value) => *value,
            ValueKind::Option(value) => value.is_some(),
            ValueKind::Result { is_ok, .. } => *is_ok,
            ValueKind::Int(value) => *value != 0,
            ValueKind::Float(value) => *value != 0.0,
            ValueKind::String(value) => !value.is_empty(),
            ValueKind::List(value) => !value.is_empty(),
            ValueKind::Channel(value) => !value.items.is_empty(),
            ValueKind::Shared(_) => true,
            ValueKind::Data(_) => true,
            ValueKind::Unit => false,
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_println(handle: FuseHandle) {
    println!("{}", unsafe { clone_to_string(handle) });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_none() -> FuseHandle {
    FuseValue::new(ValueKind::Option(None))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_some(value: FuseHandle) -> FuseHandle {
    FuseValue::new(ValueKind::Option(Some(value)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_option_is_some(handle: FuseHandle) -> bool {
    unsafe { matches!(&value_ref(handle).kind, ValueKind::Option(Some(_))) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_option_unwrap(handle: FuseHandle) -> FuseHandle {
    unsafe {
        match &value_ref(handle).kind {
            ValueKind::Option(Some(value)) => *value,
            _ => ptr::null_mut(),
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_ok(value: FuseHandle) -> FuseHandle {
    FuseValue::new(ValueKind::Result { is_ok: true, value })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_err(value: FuseHandle) -> FuseHandle {
    FuseValue::new(ValueKind::Result { is_ok: false, value })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_result_is_ok(handle: FuseHandle) -> bool {
    unsafe {
        match &value_ref(handle).kind {
            ValueKind::Result { is_ok, .. } => *is_ok,
            _ => false,
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_result_unwrap(handle: FuseHandle) -> FuseHandle {
    unsafe {
        match &value_ref(handle).kind {
            ValueKind::Result { value, .. } => *value,
            _ => ptr::null_mut(),
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_list_new() -> FuseHandle {
    FuseValue::new(ValueKind::List(Vec::new()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_list_push(list: FuseHandle, item: FuseHandle) {
    unsafe {
        if let ValueKind::List(items) = &mut value_mut(list).kind {
            items.push(item);
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_list_len(list: FuseHandle) -> usize {
    unsafe {
        match &value_ref(list).kind {
            ValueKind::List(items) => items.len(),
            _ => 0,
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_list_get(list: FuseHandle, index: usize) -> FuseHandle {
    unsafe {
        match &value_ref(list).kind {
            ValueKind::List(items) => items.get(index).copied().unwrap_or(ptr::null_mut()),
            _ => ptr::null_mut(),
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_sum(list: FuseHandle) -> FuseHandle {
    unsafe {
        match &value_ref(list).kind {
            ValueKind::List(items) => {
                let mut int_total = 0_i64;
                let mut float_total = 0.0_f64;
                let mut saw_float = false;
                for item in items {
                    match &value_ref(*item).kind {
                        ValueKind::Int(value) => int_total += *value,
                        ValueKind::Float(value) => {
                            saw_float = true;
                            float_total += *value;
                        }
                        _ => {}
                    }
                }
                if saw_float {
                    fuse_float(float_total + int_total as f64)
                } else {
                    fuse_int(int_total)
                }
            }
            _ => fuse_unit(),
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_new() -> FuseHandle {
    FuseValue::new(ValueKind::Channel(ChannelValue {
        items: VecDeque::new(),
        pending: VecDeque::new(),
        capacity: None,
    }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_bounded(capacity: usize) -> FuseHandle {
    FuseValue::new(ValueKind::Channel(ChannelValue {
        items: VecDeque::new(),
        pending: VecDeque::new(),
        capacity: Some(capacity),
    }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_send(chan: FuseHandle, value: FuseHandle) {
    unsafe {
        if let ValueKind::Channel(channel) = &mut value_mut(chan).kind {
            let is_full = channel
                .capacity
                .is_some_and(|capacity| channel.items.len() >= capacity);
            if !is_full {
                channel.items.push_back(value);
            } else {
                channel.pending.push_back(value);
            }
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_recv(chan: FuseHandle) -> FuseHandle {
    unsafe {
        match &mut value_mut(chan).kind {
            ValueKind::Channel(channel) => {
                let value = channel.items.pop_front().unwrap_or(ptr::null_mut());
                let can_promote = channel
                    .capacity
                    .is_none_or(|capacity| channel.items.len() < capacity);
                if can_promote {
                    if let Some(next) = channel.pending.pop_front() {
                        channel.items.push_back(next);
                    }
                }
                value
            }
            _ => ptr::null_mut(),
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_shared_new(value: FuseHandle) -> FuseHandle {
    FuseValue::new(ValueKind::Shared(value))
}

/// Clone a FuseValue, producing an independent snapshot.
///
/// Primitives (Int, Float, Bool, String, Unit) are deep-copied.
/// Compound types (List, Data, Option, Result) get a new container whose
/// children are the *same* handles — a shallow structural copy.
/// Data clones intentionally carry **no destructor** so that only the
/// original owner fires `__del__`.
/// Reference-like types (Channel, Shared) are not cloned — the original
/// handle is returned as-is.
unsafe fn clone_value(handle: FuseHandle) -> FuseHandle {
    if handle.is_null() {
        return handle;
    }
    let src = unsafe { value_ref(handle) };
    match &src.kind {
        ValueKind::Int(v) => unsafe { fuse_int(*v) },
        ValueKind::Float(v) => unsafe { fuse_float(*v) },
        ValueKind::Bool(v) => unsafe { fuse_bool(*v) },
        ValueKind::String(v) => FuseValue::new(ValueKind::String(v.clone())),
        ValueKind::Unit => unsafe { fuse_unit() },
        ValueKind::List(items) => FuseValue::new(ValueKind::List(items.clone())),
        ValueKind::Data(data) => FuseValue::new(ValueKind::Data(DataValue {
            type_name: data.type_name.clone(),
            fields: data.fields.clone(),
            destructor: None, // read snapshot — no destructor ownership
        })),
        ValueKind::Option(opt) => FuseValue::new(ValueKind::Option(*opt)),
        ValueKind::Result { is_ok, value } => {
            FuseValue::new(ValueKind::Result {
                is_ok: *is_ok,
                value: *value,
            })
        }
        // Reference-like types: return the original handle, not a copy.
        ValueKind::Channel(_) => handle,
        ValueKind::Shared(v) => FuseValue::new(ValueKind::Shared(*v)),
    }
}

/// `read()` returns an immutable snapshot — a clone of the inner value.
/// The caller receives an independent copy; mutations to the Shared storage
/// after this call do not affect the snapshot, and vice-versa.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_shared_read(shared: FuseHandle) -> FuseHandle {
    unsafe {
        match &value_ref(shared).kind {
            ValueKind::Shared(value) => clone_value(*value),
            _ => ptr::null_mut(),
        }
    }
}

/// `write()` returns the live inner handle — direct mutable access to the
/// Shared storage.  Mutations through this handle are immediately visible
/// to subsequent `read()` or `write()` calls.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_shared_write(shared: FuseHandle) -> FuseHandle {
    unsafe {
        match &value_ref(shared).kind {
            ValueKind::Shared(value) => *value,
            _ => ptr::null_mut(),
        }
    }
}

/// `try_write(timeout)` is the Tier 3 dynamic escape hatch.
/// Returns `Ok(inner_handle)` on success, `Err("timeout")` on failure.
///
/// In single-threaded Stage 1 the lock is always free, so the positive path
/// always succeeds.  A timeout of 0 forces the error path so that tests can
/// exercise `Err` handling without real contention.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_shared_try_write(
    shared: FuseHandle,
    timeout: FuseHandle,
) -> FuseHandle {
    unsafe {
        let timeout_val = match &value_ref(timeout).kind {
            ValueKind::Int(v) => *v,
            _ => 1, // non-zero default: succeed
        };
        if timeout_val == 0 {
            return fuse_err(fuse_string_new_utf8(
                b"timeout".as_ptr(),
                7,
            ));
        }
        match &value_ref(shared).kind {
            ValueKind::Shared(value) => fuse_ok(*value),
            _ => fuse_err(fuse_string_new_utf8(
                b"not a Shared value".as_ptr(),
                18,
            )),
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_data_new(
    type_name_ptr: *const u8,
    type_name_len: usize,
    field_count: usize,
    destructor: FuseDestructor,
) -> FuseHandle {
    FuseValue::new(ValueKind::Data(DataValue {
        type_name: read_utf8(type_name_ptr, type_name_len),
        fields: vec![ptr::null_mut(); field_count],
        destructor,
    }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_data_set_field(handle: FuseHandle, index: usize, value: FuseHandle) {
    unsafe {
        if let ValueKind::Data(data) = &mut value_mut(handle).kind {
            if let Some(field) = data.fields.get_mut(index) {
                *field = value;
            }
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_data_get_field(handle: FuseHandle, index: usize) -> FuseHandle {
    unsafe {
        match &value_ref(handle).kind {
            ValueKind::Data(data) => data.fields.get(index).copied().unwrap_or(ptr::null_mut()),
            _ => ptr::null_mut(),
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_release(handle: FuseHandle) {
    if handle.is_null() {
        return;
    }
    let value = unsafe { value_mut(handle) };
    if value.released {
        return;
    }
    value.released = true;
    match &mut value.kind {
        ValueKind::Data(data) => {
            if let Some(destructor) = data.destructor {
                unsafe { destructor(handle) };
            }
        }
        ValueKind::Channel(channel) => {
            while let Some(item) = channel.items.pop_front() {
                unsafe { fuse_release(item) };
            }
            while let Some(item) = channel.pending.pop_front() {
                unsafe { fuse_release(item) };
            }
        }
        ValueKind::Shared(value) => {
            unsafe { fuse_release(*value) };
        }
        _ => {}
    }
}
