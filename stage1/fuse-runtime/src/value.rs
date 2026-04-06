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
    Map(MapValue),
    Channel(ChannelValue),
    Shared(FuseHandle),
    Data(DataValue),
    Option(Option<FuseHandle>),
    Result { is_ok: bool, value: FuseHandle },
    Enum(EnumValue),
    Unit,
}

struct MapValue {
    entries: Vec<(FuseHandle, FuseHandle)>,
}

struct EnumValue {
    type_name: String,
    variant_tag: i64,
    variant_name: String,
    payloads: Vec<FuseHandle>,
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
        ValueKind::Map(map) => {
            let mut rendered = String::from("{");
            for (index, (key, value)) in map.entries.iter().enumerate() {
                if index > 0 {
                    rendered.push_str(", ");
                }
                rendered.push_str(&unsafe { clone_to_string(*key) });
                rendered.push_str(": ");
                rendered.push_str(&unsafe { clone_to_string(*value) });
            }
            rendered.push('}');
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
        ValueKind::Enum(e) => {
            if e.payloads.is_empty() {
                e.variant_name.clone()
            } else {
                let mut rendered = format!("{}(", e.variant_name);
                for (index, payload) in e.payloads.iter().enumerate() {
                    if index > 0 {
                        rendered.push_str(", ");
                    }
                    rendered.push_str(&unsafe { clone_to_string(*payload) });
                }
                rendered.push(')');
                rendered
            }
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
            ValueKind::Enum(_) => true,
            ValueKind::Map(map) => !map.entries.is_empty(),
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
pub unsafe extern "C" fn fuse_enum_new(
    type_name_ptr: *const u8,
    type_name_len: usize,
    variant_tag: i64,
    variant_name_ptr: *const u8,
    variant_name_len: usize,
    payload: FuseHandle,
) -> FuseHandle {
    let type_name = read_utf8(type_name_ptr, type_name_len);
    let variant_name = read_utf8(variant_name_ptr, variant_name_len);
    let payloads = if payload.is_null() {
        Vec::new()
    } else {
        vec![payload]
    };
    FuseValue::new(ValueKind::Enum(EnumValue {
        type_name,
        variant_tag,
        variant_name,
        payloads,
    }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_enum_tag(handle: FuseHandle) -> i64 {
    unsafe {
        match &value_ref(handle).kind {
            ValueKind::Enum(e) => e.variant_tag,
            _ => -1,
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_enum_payload(handle: FuseHandle, index: usize) -> FuseHandle {
    unsafe {
        match &value_ref(handle).kind {
            ValueKind::Enum(e) => e.payloads.get(index).copied().unwrap_or(ptr::null_mut()),
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

// ---- Map operations ----

fn map_key_eq(a: FuseHandle, b: FuseHandle) -> bool {
    unsafe { clone_to_string(a) == clone_to_string(b) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_map_new() -> FuseHandle {
    FuseValue::new(ValueKind::Map(MapValue {
        entries: Vec::new(),
    }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_map_set(map: FuseHandle, key: FuseHandle, value: FuseHandle) {
    unsafe {
        if let ValueKind::Map(m) = &mut value_mut(map).kind {
            for entry in m.entries.iter_mut() {
                if map_key_eq(entry.0, key) {
                    entry.1 = value;
                    return;
                }
            }
            m.entries.push((key, value));
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_map_get(map: FuseHandle, key: FuseHandle) -> FuseHandle {
    unsafe {
        match &value_ref(map).kind {
            ValueKind::Map(m) => {
                for (k, v) in &m.entries {
                    if map_key_eq(*k, key) {
                        return *v;
                    }
                }
                ptr::null_mut()
            }
            _ => ptr::null_mut(),
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_map_remove(map: FuseHandle, key: FuseHandle) -> FuseHandle {
    unsafe {
        if let ValueKind::Map(m) = &mut value_mut(map).kind {
            if let Some(pos) = m.entries.iter().position(|(k, _)| map_key_eq(*k, key)) {
                let (_, value) = m.entries.remove(pos);
                return value;
            }
        }
        ptr::null_mut()
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_map_len(map: FuseHandle) -> usize {
    unsafe {
        match &value_ref(map).kind {
            ValueKind::Map(m) => m.entries.len(),
            _ => 0,
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_map_contains(map: FuseHandle, key: FuseHandle) -> bool {
    unsafe {
        match &value_ref(map).kind {
            ValueKind::Map(m) => m.entries.iter().any(|(k, _)| map_key_eq(*k, key)),
            _ => false,
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_map_keys(map: FuseHandle) -> FuseHandle {
    unsafe {
        match &value_ref(map).kind {
            ValueKind::Map(m) => {
                let list = fuse_list_new();
                for (k, _) in &m.entries {
                    fuse_list_push(list, *k);
                }
                list
            }
            _ => fuse_list_new(),
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_map_values(map: FuseHandle) -> FuseHandle {
    unsafe {
        match &value_ref(map).kind {
            ValueKind::Map(m) => {
                let list = fuse_list_new();
                for (_, v) in &m.entries {
                    fuse_list_push(list, *v);
                }
                list
            }
            _ => fuse_list_new(),
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_map_entries(map: FuseHandle) -> FuseHandle {
    unsafe {
        match &value_ref(map).kind {
            ValueKind::Map(m) => {
                let list = fuse_list_new();
                for (k, v) in &m.entries {
                    let pair = fuse_list_new();
                    fuse_list_push(pair, *k);
                    fuse_list_push(pair, *v);
                    fuse_list_push(list, pair);
                }
                list
            }
            _ => fuse_list_new(),
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
        ValueKind::Map(map) => FuseValue::new(ValueKind::Map(MapValue {
            entries: map.entries.clone(),
        })),
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
        ValueKind::Enum(e) => FuseValue::new(ValueKind::Enum(EnumValue {
            type_name: e.type_name.clone(),
            variant_tag: e.variant_tag,
            variant_name: e.variant_name.clone(),
            payloads: e.payloads.clone(),
        })),
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

// ---------------------------------------------------------------------------
// stdlib FFI helpers
// ---------------------------------------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_int_to_float(handle: FuseHandle) -> FuseHandle {
    if let ValueKind::Int(n) = &(*handle).kind {
        return fuse_float(*n as f64);
    }
    fuse_float(0.0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_int_parse(handle: FuseHandle) -> FuseHandle {
    if let ValueKind::String(s) = &(*handle).kind {
        match s.parse::<i64>() {
            Ok(n) => return fuse_ok(fuse_int(n)),
            Err(_) => {
                let msg = format!("int: invalid number: {s}");
                return fuse_err(fuse_string_new_utf8(msg.as_ptr(), msg.len()));
            }
        }
    }
    let msg = "int: expected string";
    fuse_err(fuse_string_new_utf8(msg.as_ptr(), msg.len()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_len(handle: FuseHandle) -> FuseHandle {
    if let ValueKind::String(s) = &(*handle).kind {
        return fuse_int(s.chars().count() as i64);
    }
    fuse_int(0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_char_at(handle: FuseHandle, index: FuseHandle) -> FuseHandle {
    if let (ValueKind::String(s), ValueKind::Int(i)) = (&(*handle).kind, &(*index).kind) {
        if let Some(ch) = s.chars().nth(*i as usize) {
            let ch_str = ch.to_string();
            return fuse_string_new_utf8(ch_str.as_ptr(), ch_str.len());
        }
    }
    fuse_string_new_utf8(b"".as_ptr(), 0)
}

// --- Float FFI helpers ---

unsafe fn extract_float(handle: FuseHandle) -> f64 {
    unsafe { match &(*handle).kind { ValueKind::Float(v) => *v, ValueKind::Int(v) => *v as f64, _ => 0.0 } }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_float_abs(h: FuseHandle) -> FuseHandle { fuse_float(extract_float(h).abs()) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_float_floor(h: FuseHandle) -> FuseHandle { fuse_float(extract_float(h).floor()) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_float_ceil(h: FuseHandle) -> FuseHandle { fuse_float(extract_float(h).ceil()) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_float_round(h: FuseHandle) -> FuseHandle { fuse_float(extract_float(h).round()) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_float_trunc(h: FuseHandle) -> FuseHandle { fuse_float(extract_float(h).trunc()) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_float_fract(h: FuseHandle) -> FuseHandle { fuse_float(extract_float(h).fract()) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_float_sqrt(h: FuseHandle) -> FuseHandle { fuse_float(extract_float(h).sqrt()) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_float_pow(h: FuseHandle, exp: FuseHandle) -> FuseHandle { fuse_float(extract_float(h).powf(extract_float(exp))) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_float_is_nan(h: FuseHandle) -> FuseHandle { fuse_bool(extract_float(h).is_nan()) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_float_is_infinite(h: FuseHandle) -> FuseHandle { fuse_bool(extract_float(h).is_infinite()) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_float_is_finite(h: FuseHandle) -> FuseHandle { fuse_bool(extract_float(h).is_finite()) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_float_to_int(h: FuseHandle) -> FuseHandle { fuse_int(extract_float(h) as i64) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_float_parse(h: FuseHandle) -> FuseHandle {
    if let ValueKind::String(s) = &(*h).kind {
        match s.parse::<f64>() {
            Ok(v) => return fuse_ok(fuse_float(v)),
            Err(_) => {
                let msg = format!("float: invalid number: {s}");
                return fuse_err(fuse_string_new_utf8(msg.as_ptr(), msg.len()));
            }
        }
    }
    let msg = "float: expected string";
    fuse_err(fuse_string_new_utf8(msg.as_ptr(), msg.len()))
}
// --- IO FFI helpers ---

unsafe fn make_io_error(msg: &str, code: i64) -> FuseHandle {
    let type_name = b"IOError";
    let data = fuse_data_new(type_name.as_ptr(), type_name.len(), 2, None);
    fuse_data_set_field(data, 0, fuse_string_new_utf8(msg.as_ptr(), msg.len()));
    fuse_data_set_field(data, 1, fuse_int(code));
    data
}

unsafe fn io_error_code(e: &std::io::Error) -> i64 {
    match e.kind() {
        std::io::ErrorKind::NotFound => 1,
        std::io::ErrorKind::PermissionDenied => 2,
        std::io::ErrorKind::AlreadyExists => 3,
        std::io::ErrorKind::Interrupted => 7,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_io_read_file(path: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    match std::fs::read_to_string(p) {
        Ok(s) => fuse_ok(fuse_string_new_utf8(s.as_ptr(), s.len())),
        Err(e) => { let msg = format!("io: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_io_read_file_bytes(path: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    match std::fs::read(p) {
        Ok(bytes) => {
            let list = fuse_list_new();
            for b in bytes { fuse_list_push(list, fuse_int(b as i64)); }
            fuse_ok(list)
        }
        Err(e) => { let msg = format!("io: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_io_write_file(path: FuseHandle, content: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    let c = extract_string(content);
    match std::fs::write(p, c) {
        Ok(()) => fuse_ok(fuse_unit()),
        Err(e) => { let msg = format!("io: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_io_write_file_bytes(path: FuseHandle, bytes: FuseHandle) -> FuseHandle {
    if let ValueKind::List(items) = &(*bytes).kind {
        let data: Vec<u8> = items.iter().filter_map(|h| match &(**h).kind { ValueKind::Int(n) => Some(*n as u8), _ => None }).collect();
        let p = extract_string(path);
        match std::fs::write(p, &data) {
            Ok(()) => return fuse_ok(fuse_unit()),
            Err(e) => { let msg = format!("io: {e}"); return fuse_err(make_io_error(&msg, io_error_code(&e))); }
        }
    }
    let msg = "io: expected byte list";
    fuse_err(make_io_error(msg, 0))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_io_append_file(path: FuseHandle, content: FuseHandle) -> FuseHandle {
    use std::io::Write;
    let p = extract_string(path);
    let c = extract_string(content);
    match std::fs::OpenOptions::new().append(true).create(true).open(p) {
        Ok(mut f) => match f.write_all(c.as_bytes()) {
            Ok(()) => fuse_ok(fuse_unit()),
            Err(e) => { let msg = format!("io: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
        },
        Err(e) => { let msg = format!("io: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_io_read_line() -> FuseHandle {
    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
        Ok(_) => {
            let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
            fuse_ok(fuse_string_new_utf8(trimmed.as_ptr(), trimmed.len()))
        }
        Err(e) => { let msg = format!("io: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_io_read_all() -> FuseHandle {
    use std::io::Read;
    let mut s = String::new();
    match std::io::stdin().read_to_string(&mut s) {
        Ok(_) => fuse_ok(fuse_string_new_utf8(s.as_ptr(), s.len())),
        Err(e) => { let msg = format!("io: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

// --- File handle FFI ---

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_file_open(path: FuseHandle, mode: FuseHandle) -> FuseHandle {
    use std::io::BufWriter;
    let p = extract_string(path);
    let m = match &(*mode).kind { ValueKind::Int(n) => *n, _ => 0 };
    let result = match m {
        0 => std::fs::File::open(p).map(|f| Box::new(f) as Box<dyn std::any::Any>),
        1 => std::fs::File::create(p).map(|f| Box::new(BufWriter::new(f)) as Box<dyn std::any::Any>),
        2 => std::fs::OpenOptions::new().append(true).create(true).open(p).map(|f| Box::new(BufWriter::new(f)) as Box<dyn std::any::Any>),
        3 => std::fs::OpenOptions::new().read(true).write(true).open(p).map(|f| Box::new(f) as Box<dyn std::any::Any>),
        _ => std::fs::File::open(p).map(|f| Box::new(f) as Box<dyn std::any::Any>),
    };
    match result {
        Ok(handle) => {
            let ptr = Box::into_raw(handle);
            let type_name = b"File";
            let file_handle = fuse_data_new(type_name.as_ptr(), type_name.len(), 1, Some(fuse_rt_file_destructor));
            fuse_data_set_field(file_handle, 0, ptr as FuseHandle);
            fuse_ok(file_handle)
        }
        Err(e) => { let msg = format!("io: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

unsafe extern "C" fn fuse_rt_file_destructor(_handle: FuseHandle) {
    // The file handle is cleaned up when the data class is dropped.
    // In a production runtime, this would close the file descriptor.
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_file_close(_file: FuseHandle) -> FuseHandle {
    fuse_ok(fuse_unit())
}

// --- Path FFI ---

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_path_separator() -> FuseHandle {
    let sep = if cfg!(windows) { "\\" } else { "/" };
    fuse_string_new_utf8(sep.as_ptr(), sep.len())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_path_cwd() -> FuseHandle {
    match std::env::current_dir() {
        Ok(p) => {
            let s = p.to_string_lossy().to_string();
            fuse_ok(fuse_string_new_utf8(s.as_ptr(), s.len()))
        }
        Err(e) => {
            let msg = format!("path: {e}");
            fuse_err(fuse_string_new_utf8(msg.as_ptr(), msg.len()))
        }
    }
}

// --- OS FFI ---

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_exists(path: FuseHandle) -> FuseHandle {
    fuse_bool(std::path::Path::new(extract_string(path)).exists())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_is_file(path: FuseHandle) -> FuseHandle {
    fuse_bool(std::path::Path::new(extract_string(path)).is_file())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_is_dir(path: FuseHandle) -> FuseHandle {
    fuse_bool(std::path::Path::new(extract_string(path)).is_dir())
}

unsafe fn metadata_to_file_info(p: &str, meta: &std::fs::Metadata) -> FuseHandle {
    let type_name = b"FileInfo";
    let data = fuse_data_new(type_name.as_ptr(), type_name.len(), 6, None);
    fuse_data_set_field(data, 0, fuse_string_new_utf8(p.as_ptr(), p.len()));
    // kind
    let kind = if meta.is_file() { 0i64 } else if meta.is_dir() { 1 } else if meta.file_type().is_symlink() { 2 } else { 3 };
    fuse_data_set_field(data, 1, fuse_int(kind));
    fuse_data_set_field(data, 2, fuse_int(meta.len() as i64));
    let modified = meta.modified().ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64).unwrap_or(0);
    fuse_data_set_field(data, 3, fuse_int(modified));
    let created = meta.created().ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64).unwrap_or(0);
    fuse_data_set_field(data, 4, fuse_int(created));
    fuse_data_set_field(data, 5, fuse_bool(meta.permissions().readonly()));
    data
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_stat(path: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    match std::fs::metadata(p) {
        Ok(meta) => fuse_ok(metadata_to_file_info(p, &meta)),
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

unsafe fn dir_entry_to_handle(entry: &std::fs::DirEntry) -> FuseHandle {
    let type_name = b"DirEntry";
    let data = fuse_data_new(type_name.as_ptr(), type_name.len(), 5, None);
    let name = entry.file_name().to_string_lossy().to_string();
    fuse_data_set_field(data, 0, fuse_string_new_utf8(name.as_ptr(), name.len()));
    let path = entry.path().to_string_lossy().to_string();
    fuse_data_set_field(data, 1, fuse_string_new_utf8(path.as_ptr(), path.len()));
    let meta = entry.metadata().ok();
    let kind = meta.as_ref().map(|m| {
        if m.is_file() { 0i64 } else if m.is_dir() { 1 } else if m.file_type().is_symlink() { 2 } else { 3 }
    }).unwrap_or(3);
    fuse_data_set_field(data, 2, fuse_int(kind));
    let size = meta.as_ref().map(|m| m.len() as i64).unwrap_or(0);
    fuse_data_set_field(data, 3, fuse_int(size));
    let modified = meta.as_ref().and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64).unwrap_or(0);
    fuse_data_set_field(data, 4, fuse_int(modified));
    data
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_read_dir(path: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    match std::fs::read_dir(p) {
        Ok(entries) => {
            let list = fuse_list_new();
            for entry in entries.flatten() {
                fuse_list_push(list, dir_entry_to_handle(&entry));
            }
            fuse_ok(list)
        }
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_mkdir(path: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    match std::fs::create_dir(p) {
        Ok(()) => fuse_ok(fuse_unit()),
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_mkdir_all(path: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    match std::fs::create_dir_all(p) {
        Ok(()) => fuse_ok(fuse_unit()),
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_create_file(path: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    match std::fs::OpenOptions::new().write(true).create_new(true).open(p) {
        Ok(_) => fuse_ok(fuse_unit()),
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_copy_file(src: FuseHandle, dst: FuseHandle) -> FuseHandle {
    let s = extract_string(src);
    let d = extract_string(dst);
    match std::fs::copy(s, d) {
        Ok(_) => fuse_ok(fuse_unit()),
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

unsafe fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_copy_dir(src: FuseHandle, dst: FuseHandle) -> FuseHandle {
    let s = extract_string(src);
    let d = extract_string(dst);
    match copy_dir_recursive(std::path::Path::new(s), std::path::Path::new(d)) {
        Ok(()) => fuse_ok(fuse_unit()),
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_rename(src: FuseHandle, dst: FuseHandle) -> FuseHandle {
    let s = extract_string(src);
    let d = extract_string(dst);
    match std::fs::rename(s, d) {
        Ok(()) => fuse_ok(fuse_unit()),
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_remove_file(path: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    match std::fs::remove_file(p) {
        Ok(()) => fuse_ok(fuse_unit()),
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_remove_dir(path: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    match std::fs::remove_dir(p) {
        Ok(()) => fuse_ok(fuse_unit()),
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_remove_dir_all(path: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    match std::fs::remove_dir_all(p) {
        Ok(()) => fuse_ok(fuse_unit()),
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_create_symlink(src: FuseHandle, dst: FuseHandle) -> FuseHandle {
    let s = extract_string(src);
    let d = extract_string(dst);
    #[cfg(unix)]
    let result = std::os::unix::fs::symlink(s, d);
    #[cfg(windows)]
    let result = {
        let src_path = std::path::Path::new(s);
        if src_path.is_dir() {
            std::os::windows::fs::symlink_dir(s, d)
        } else {
            std::os::windows::fs::symlink_file(s, d)
        }
    };
    match result {
        Ok(()) => fuse_ok(fuse_unit()),
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_read_symlink(path: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    match std::fs::read_link(p) {
        Ok(target) => {
            let s = target.to_string_lossy().to_string();
            fuse_ok(fuse_string_new_utf8(s.as_ptr(), s.len()))
        }
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_set_read_only(path: FuseHandle, readonly: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    let ro = match &(*readonly).kind { ValueKind::Bool(b) => *b, _ => false };
    match std::fs::metadata(p) {
        Ok(meta) => {
            let mut perms = meta.permissions();
            perms.set_readonly(ro);
            match std::fs::set_permissions(p, perms) {
                Ok(()) => fuse_ok(fuse_unit()),
                Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
            }
        }
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_temp_dir() -> FuseHandle {
    let s = std::env::temp_dir().to_string_lossy().to_string();
    fuse_string_new_utf8(s.as_ptr(), s.len())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_temp_file(prefix: FuseHandle) -> FuseHandle {
    let pfx = extract_string(prefix);
    let dir = std::env::temp_dir();
    let name = format!("{pfx}{}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
    let path = dir.join(name);
    match std::fs::File::create(&path) {
        Ok(_) => {
            let s = path.to_string_lossy().to_string();
            fuse_ok(fuse_string_new_utf8(s.as_ptr(), s.len()))
        }
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_temp_dir_create(prefix: FuseHandle) -> FuseHandle {
    let pfx = extract_string(prefix);
    let dir = std::env::temp_dir();
    let name = format!("{pfx}{}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
    let path = dir.join(name);
    match std::fs::create_dir(&path) {
        Ok(()) => {
            let s = path.to_string_lossy().to_string();
            fuse_ok(fuse_string_new_utf8(s.as_ptr(), s.len()))
        }
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

unsafe fn read_dir_recursive_impl(root: &std::path::Path, list: FuseHandle) -> std::io::Result<()> {
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        fuse_list_push(list, dir_entry_to_handle(&entry));
        if entry.file_type()?.is_dir() {
            read_dir_recursive_impl(&entry.path(), list)?;
        }
    }
    Ok(())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_read_dir_recursive(path: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    let list = fuse_list_new();
    match read_dir_recursive_impl(std::path::Path::new(p), list) {
        Ok(()) => fuse_ok(list),
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_move(src: FuseHandle, dst: FuseHandle) -> FuseHandle {
    let s = extract_string(src);
    let d = extract_string(dst);
    // Try rename first (atomic on same filesystem)
    match std::fs::rename(s, d) {
        Ok(()) => return fuse_ok(fuse_unit()),
        Err(_) => {}
    }
    // Fallback: copy + remove
    let src_path = std::path::Path::new(s);
    let dst_path = std::path::Path::new(d);
    let result = if src_path.is_dir() {
        copy_dir_recursive(src_path, dst_path).and_then(|()| std::fs::remove_dir_all(src_path))
    } else {
        std::fs::copy(s, d).map(|_| ()).and_then(|()| std::fs::remove_file(s))
    };
    match result {
        Ok(()) => fuse_ok(fuse_unit()),
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

// --- Env FFI ---

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_env_get(name: FuseHandle) -> FuseHandle {
    let key = extract_string(name);
    match std::env::var(key) {
        Ok(val) => fuse_some(fuse_string_new_utf8(val.as_ptr(), val.len())),
        Err(_) => fuse_none(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_env_set(name: FuseHandle, value: FuseHandle) -> FuseHandle {
    let key = extract_string(name);
    let val = extract_string(value);
    // SAFETY: set_var is unsafe in Rust 2024 due to thread-safety concerns,
    // but Fuse's single-threaded evaluator makes this safe in practice.
    unsafe { std::env::set_var(key, val); }
    fuse_ok(fuse_unit())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_env_remove(name: FuseHandle) -> FuseHandle {
    let key = extract_string(name);
    unsafe { std::env::remove_var(key); }
    fuse_ok(fuse_unit())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_env_all() -> FuseHandle {
    let map = fuse_map_new();
    for (key, val) in std::env::vars() {
        let k = fuse_string_new_utf8(key.as_ptr(), key.len());
        let v = fuse_string_new_utf8(val.as_ptr(), val.len());
        fuse_map_set(map, k, v);
    }
    map
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_env_has(name: FuseHandle) -> FuseHandle {
    let key = extract_string(name);
    fuse_bool(std::env::var(key).is_ok())
}

// --- Process FFI ---

unsafe fn make_process_error(msg: &str, code: i64) -> FuseHandle {
    let type_name = b"ProcessError";
    let data = fuse_data_new(type_name.as_ptr(), type_name.len(), 2, None);
    fuse_data_set_field(data, 0, fuse_string_new_utf8(msg.as_ptr(), msg.len()));
    fuse_data_set_field(data, 1, fuse_int(code));
    data
}

unsafe fn output_to_handle(output: &std::process::Output) -> FuseHandle {
    let type_name = b"Output";
    let data = fuse_data_new(type_name.as_ptr(), type_name.len(), 4, None);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1) as i64;
    fuse_data_set_field(data, 0, fuse_string_new_utf8(stdout.as_ptr(), stdout.len()));
    fuse_data_set_field(data, 1, fuse_string_new_utf8(stderr.as_ptr(), stderr.len()));
    fuse_data_set_field(data, 2, fuse_int(code));
    fuse_data_set_field(data, 3, fuse_bool(output.status.success()));
    data
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_process_run(program: FuseHandle, args: FuseHandle) -> FuseHandle {
    let prog = extract_string(program);
    let mut cmd = std::process::Command::new(prog);
    if let ValueKind::List(items) = &(*args).kind {
        for item in items {
            if let ValueKind::String(s) = &(**item).kind {
                cmd.arg(s.as_str());
            }
        }
    }
    match cmd.output() {
        Ok(output) => fuse_ok(output_to_handle(&output)),
        Err(e) => { let msg = format!("process: {e}"); fuse_err(make_process_error(&msg, -1)) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_process_shell(command: FuseHandle) -> FuseHandle {
    let cmd_str = extract_string(command);
    let result = if cfg!(windows) {
        std::process::Command::new("cmd.exe").args(["/C", cmd_str]).output()
    } else {
        std::process::Command::new("sh").args(["-c", cmd_str]).output()
    };
    match result {
        Ok(output) => fuse_ok(output_to_handle(&output)),
        Err(e) => { let msg = format!("process: {e}"); fuse_err(make_process_error(&msg, -1)) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_process_run_with_stdin(
    program: FuseHandle, args: FuseHandle, stdin_data: FuseHandle,
    cwd: FuseHandle, env_keys: FuseHandle, env_vals: FuseHandle,
) -> FuseHandle {
    use std::io::Write;
    let prog = extract_string(program);
    let mut cmd = std::process::Command::new(prog);
    if let ValueKind::List(items) = &(*args).kind {
        for item in items {
            if let ValueKind::String(s) = &(**item).kind {
                cmd.arg(s.as_str());
            }
        }
    }
    // cwd
    let cwd_str = extract_string(cwd);
    if !cwd_str.is_empty() {
        cmd.current_dir(cwd_str);
    }
    // env
    if let (ValueKind::List(keys), ValueKind::List(vals)) = (&(*env_keys).kind, &(*env_vals).kind) {
        for (k, v) in keys.iter().zip(vals.iter()) {
            if let (ValueKind::String(ks), ValueKind::String(vs)) = (&(**k).kind, &(**v).kind) {
                cmd.env(ks.as_str(), vs.as_str());
            }
        }
    }
    // stdin
    let stdin_str = extract_string(stdin_data);
    if !stdin_str.is_empty() {
        cmd.stdin(std::process::Stdio::piped());
    }
    match cmd.spawn() {
        Ok(mut child) => {
            if !stdin_str.is_empty() {
                if let Some(ref mut stdin) = child.stdin {
                    let _ = stdin.write_all(stdin_str.as_bytes());
                }
                child.stdin.take(); // close stdin
            }
            match child.wait_with_output() {
                Ok(output) => fuse_ok(output_to_handle(&output)),
                Err(e) => { let msg = format!("process: {e}"); fuse_err(make_process_error(&msg, -1)) }
            }
        }
        Err(e) => { let msg = format!("process: {e}"); fuse_err(make_process_error(&msg, -1)) }
    }
}

// --- Net FFI ---

unsafe fn make_net_error(msg: &str, code: i64) -> FuseHandle {
    let type_name = b"NetError";
    let data = fuse_data_new(type_name.as_ptr(), type_name.len(), 2, None);
    fuse_data_set_field(data, 0, fuse_string_new_utf8(msg.as_ptr(), msg.len()));
    fuse_data_set_field(data, 1, fuse_int(code));
    data
}

unsafe fn net_error_code(e: &std::io::Error) -> i64 {
    match e.kind() {
        std::io::ErrorKind::ConnectionRefused => 1,
        std::io::ErrorKind::TimedOut => 2,
        std::io::ErrorKind::AddrInUse => 3,
        std::io::ErrorKind::BrokenPipe => 4,
        std::io::ErrorKind::NotConnected => 5,
        _ => 0,
    }
}

unsafe fn wrap_tcp_stream(stream: std::net::TcpStream) -> FuseHandle {
    let boxed: Box<dyn std::any::Any> = Box::new(stream);
    let ptr = Box::into_raw(boxed);
    let type_name = b"TcpStream";
    let handle = fuse_data_new(type_name.as_ptr(), type_name.len(), 1, None);
    fuse_data_set_field(handle, 0, ptr as FuseHandle);
    handle
}

unsafe fn extract_tcp_stream<'a>(handle: FuseHandle) -> Option<&'a mut std::net::TcpStream> {
    if let ValueKind::Data(data) = &(*handle).kind {
        if let Some(field0) = data.fields.first() {
            let ptr = *field0 as *mut dyn std::any::Any;
            if !ptr.is_null() {
                return (*ptr).downcast_mut::<std::net::TcpStream>();
            }
        }
    }
    None
}

// --- TcpStream ---

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_net_tcp_connect(addr: FuseHandle, port: FuseHandle) -> FuseHandle {
    let a = extract_string(addr);
    let p = match &(*port).kind { ValueKind::Int(n) => *n as u16, _ => 0 };
    match std::net::TcpStream::connect((a, p)) {
        Ok(stream) => fuse_ok(wrap_tcp_stream(stream)),
        Err(e) => { let msg = format!("net: {e}"); fuse_err(make_net_error(&msg, net_error_code(&e))) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_net_tcp_connect_timeout(addr: FuseHandle, port: FuseHandle, timeout_ms: FuseHandle) -> FuseHandle {
    let a = extract_string(addr);
    let p = match &(*port).kind { ValueKind::Int(n) => *n as u16, _ => 0 };
    let ms = match &(*timeout_ms).kind { ValueKind::Int(n) => *n as u64, _ => 5000 };
    let socket_addr = format!("{a}:{p}");
    match socket_addr.parse::<std::net::SocketAddr>() {
        Ok(sa) => match std::net::TcpStream::connect_timeout(&sa, std::time::Duration::from_millis(ms)) {
            Ok(stream) => fuse_ok(wrap_tcp_stream(stream)),
            Err(e) => { let msg = format!("net: {e}"); fuse_err(make_net_error(&msg, net_error_code(&e))) }
        },
        Err(e) => { let msg = format!("net: invalid address: {e}"); fuse_err(make_net_error(&msg, 0)) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_net_tcp_read(stream: FuseHandle, max_bytes: FuseHandle) -> FuseHandle {
    use std::io::Read;
    let max = match &(*max_bytes).kind { ValueKind::Int(n) => *n as usize, _ => 4096 };
    if let Some(s) = extract_tcp_stream(stream) {
        let mut buf = vec![0u8; max];
        match s.read(&mut buf) {
            Ok(n) => {
                let list = fuse_list_new();
                for &b in &buf[..n] { fuse_list_push(list, fuse_int(b as i64)); }
                fuse_ok(list)
            }
            Err(e) => { let msg = format!("net: {e}"); fuse_err(make_net_error(&msg, net_error_code(&e))) }
        }
    } else {
        fuse_err(make_net_error("net: invalid stream", 0))
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_net_tcp_read_all(stream: FuseHandle) -> FuseHandle {
    use std::io::Read;
    if let Some(s) = extract_tcp_stream(stream) {
        let mut buf = String::new();
        match s.read_to_string(&mut buf) {
            Ok(_) => fuse_ok(fuse_string_new_utf8(buf.as_ptr(), buf.len())),
            Err(e) => { let msg = format!("net: {e}"); fuse_err(make_net_error(&msg, net_error_code(&e))) }
        }
    } else {
        fuse_err(make_net_error("net: invalid stream", 0))
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_net_tcp_write(stream: FuseHandle, data: FuseHandle) -> FuseHandle {
    use std::io::Write;
    if let Some(s) = extract_tcp_stream(stream) {
        let d = extract_string(data);
        match s.write(d.as_bytes()) {
            Ok(n) => fuse_ok(fuse_int(n as i64)),
            Err(e) => { let msg = format!("net: {e}"); fuse_err(make_net_error(&msg, net_error_code(&e))) }
        }
    } else {
        fuse_err(make_net_error("net: invalid stream", 0))
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_net_tcp_write_bytes(stream: FuseHandle, data: FuseHandle) -> FuseHandle {
    use std::io::Write;
    if let Some(s) = extract_tcp_stream(stream) {
        if let ValueKind::List(items) = &(*data).kind {
            let bytes: Vec<u8> = items.iter().filter_map(|h| match &(**h).kind { ValueKind::Int(n) => Some(*n as u8), _ => None }).collect();
            match s.write(&bytes) {
                Ok(n) => return fuse_ok(fuse_int(n as i64)),
                Err(e) => { let msg = format!("net: {e}"); return fuse_err(make_net_error(&msg, net_error_code(&e))); }
            }
        }
        fuse_err(make_net_error("net: expected byte list", 0))
    } else {
        fuse_err(make_net_error("net: invalid stream", 0))
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_net_tcp_flush(stream: FuseHandle) -> FuseHandle {
    use std::io::Write;
    if let Some(s) = extract_tcp_stream(stream) {
        match s.flush() {
            Ok(()) => fuse_ok(fuse_unit()),
            Err(e) => { let msg = format!("net: {e}"); fuse_err(make_net_error(&msg, net_error_code(&e))) }
        }
    } else {
        fuse_err(make_net_error("net: invalid stream", 0))
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_net_tcp_set_read_timeout(stream: FuseHandle, ms: FuseHandle) -> FuseHandle {
    if let Some(s) = extract_tcp_stream(stream) {
        let timeout = match &(*ms).kind { ValueKind::Int(0) => None, ValueKind::Int(n) => Some(std::time::Duration::from_millis(*n as u64)), _ => None };
        match s.set_read_timeout(timeout) {
            Ok(()) => fuse_ok(fuse_unit()),
            Err(e) => { let msg = format!("net: {e}"); fuse_err(make_net_error(&msg, net_error_code(&e))) }
        }
    } else {
        fuse_err(make_net_error("net: invalid stream", 0))
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_net_tcp_set_write_timeout(stream: FuseHandle, ms: FuseHandle) -> FuseHandle {
    if let Some(s) = extract_tcp_stream(stream) {
        let timeout = match &(*ms).kind { ValueKind::Int(0) => None, ValueKind::Int(n) => Some(std::time::Duration::from_millis(*n as u64)), _ => None };
        match s.set_write_timeout(timeout) {
            Ok(()) => fuse_ok(fuse_unit()),
            Err(e) => { let msg = format!("net: {e}"); fuse_err(make_net_error(&msg, net_error_code(&e))) }
        }
    } else {
        fuse_err(make_net_error("net: invalid stream", 0))
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_net_tcp_local_addr(stream: FuseHandle) -> FuseHandle {
    if let Some(s) = extract_tcp_stream(stream) {
        match s.local_addr() {
            Ok(addr) => { let a = addr.to_string(); fuse_ok(fuse_string_new_utf8(a.as_ptr(), a.len())) }
            Err(e) => { let msg = format!("net: {e}"); fuse_err(make_net_error(&msg, net_error_code(&e))) }
        }
    } else {
        fuse_err(make_net_error("net: invalid stream", 0))
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_net_tcp_peer_addr(stream: FuseHandle) -> FuseHandle {
    if let Some(s) = extract_tcp_stream(stream) {
        match s.peer_addr() {
            Ok(addr) => { let a = addr.to_string(); fuse_ok(fuse_string_new_utf8(a.as_ptr(), a.len())) }
            Err(e) => { let msg = format!("net: {e}"); fuse_err(make_net_error(&msg, net_error_code(&e))) }
        }
    } else {
        fuse_err(make_net_error("net: invalid stream", 0))
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_net_tcp_close(_stream: FuseHandle) -> FuseHandle {
    // Drop happens when the data class is ASAP-destroyed.
    fuse_ok(fuse_unit())
}

// --- TcpListener ---

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_net_tcp_bind(addr: FuseHandle, port: FuseHandle) -> FuseHandle {
    let a = extract_string(addr);
    let p = match &(*port).kind { ValueKind::Int(n) => *n as u16, _ => 0 };
    match std::net::TcpListener::bind((a, p)) {
        Ok(listener) => {
            let boxed: Box<dyn std::any::Any> = Box::new(listener);
            let ptr = Box::into_raw(boxed);
            let type_name = b"TcpListener";
            let handle = fuse_data_new(type_name.as_ptr(), type_name.len(), 1, None);
            fuse_data_set_field(handle, 0, ptr as FuseHandle);
            fuse_ok(handle)
        }
        Err(e) => { let msg = format!("net: {e}"); fuse_err(make_net_error(&msg, net_error_code(&e))) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_net_tcp_accept(listener: FuseHandle) -> FuseHandle {
    if let ValueKind::Data(data) = &(*listener).kind {
        if let Some(field0) = data.fields.first() {
            let ptr = *field0 as *mut dyn std::any::Any;
            if !ptr.is_null() {
                if let Some(l) = (*ptr).downcast_ref::<std::net::TcpListener>() {
                    return match l.accept() {
                        Ok((stream, _addr)) => fuse_ok(wrap_tcp_stream(stream)),
                        Err(e) => { let msg = format!("net: {e}"); fuse_err(make_net_error(&msg, net_error_code(&e))) }
                    };
                }
            }
        }
    }
    fuse_err(make_net_error("net: invalid listener", 0))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_net_tcp_listener_local_addr(listener: FuseHandle) -> FuseHandle {
    if let ValueKind::Data(data) = &(*listener).kind {
        if let Some(field0) = data.fields.first() {
            let ptr = *field0 as *mut dyn std::any::Any;
            if !ptr.is_null() {
                if let Some(l) = (*ptr).downcast_ref::<std::net::TcpListener>() {
                    return match l.local_addr() {
                        Ok(addr) => { let a = addr.to_string(); fuse_ok(fuse_string_new_utf8(a.as_ptr(), a.len())) }
                        Err(e) => { let msg = format!("net: {e}"); fuse_err(make_net_error(&msg, net_error_code(&e))) }
                    };
                }
            }
        }
    }
    fuse_err(make_net_error("net: invalid listener", 0))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_net_tcp_listener_close(_listener: FuseHandle) -> FuseHandle {
    fuse_ok(fuse_unit())
}

// --- UdpSocket ---

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_net_udp_bind(addr: FuseHandle, port: FuseHandle) -> FuseHandle {
    let a = extract_string(addr);
    let p = match &(*port).kind { ValueKind::Int(n) => *n as u16, _ => 0 };
    match std::net::UdpSocket::bind((a, p)) {
        Ok(socket) => {
            let boxed: Box<dyn std::any::Any> = Box::new(socket);
            let ptr = Box::into_raw(boxed);
            let type_name = b"UdpSocket";
            let handle = fuse_data_new(type_name.as_ptr(), type_name.len(), 1, None);
            fuse_data_set_field(handle, 0, ptr as FuseHandle);
            fuse_ok(handle)
        }
        Err(e) => { let msg = format!("net: {e}"); fuse_err(make_net_error(&msg, net_error_code(&e))) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_net_udp_send_to(socket: FuseHandle, payload: FuseHandle, addr: FuseHandle, port: FuseHandle) -> FuseHandle {
    if let ValueKind::Data(dv) = &(*socket).kind {
        if let Some(field0) = dv.fields.first() {
            let ptr = *field0 as *mut dyn std::any::Any;
            if !ptr.is_null() {
                if let Some(s) = (*ptr).downcast_ref::<std::net::UdpSocket>() {
                    if let ValueKind::List(items) = &(*payload).kind {
                        let bytes: Vec<u8> = items.iter().filter_map(|h| match &(**h).kind { ValueKind::Int(n) => Some(*n as u8), _ => None }).collect();
                        let a = extract_string(addr);
                        let p = match &(*port).kind { ValueKind::Int(n) => *n as u16, _ => 0 };
                        return match s.send_to(&bytes, (a, p)) {
                            Ok(n) => fuse_ok(fuse_int(n as i64)),
                            Err(e) => { let msg = format!("net: {e}"); fuse_err(make_net_error(&msg, net_error_code(&e))) }
                        };
                    }
                }
            }
        }
    }
    fuse_err(make_net_error("net: invalid socket", 0))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_net_udp_recv_from(socket: FuseHandle, max_bytes: FuseHandle) -> FuseHandle {
    if let ValueKind::Data(data) = &(*socket).kind {
        if let Some(field0) = data.fields.first() {
            let ptr = *field0 as *mut dyn std::any::Any;
            if !ptr.is_null() {
                if let Some(s) = (*ptr).downcast_ref::<std::net::UdpSocket>() {
                    let max = match &(*max_bytes).kind { ValueKind::Int(n) => *n as usize, _ => 4096 };
                    let mut buf = vec![0u8; max];
                    return match s.recv_from(&mut buf) {
                        Ok((n, addr)) => {
                            let list = fuse_list_new();
                            for &b in &buf[..n] { fuse_list_push(list, fuse_int(b as i64)); }
                            let addr_str = addr.ip().to_string();
                            let port = addr.port() as i64;
                            // Return a 3-element list: [data_list, addr_string, port_int]
                            let result = fuse_list_new();
                            fuse_list_push(result, list);
                            fuse_list_push(result, fuse_string_new_utf8(addr_str.as_ptr(), addr_str.len()));
                            fuse_list_push(result, fuse_int(port));
                            fuse_ok(result)
                        }
                        Err(e) => { let msg = format!("net: {e}"); fuse_err(make_net_error(&msg, net_error_code(&e))) }
                    };
                }
            }
        }
    }
    fuse_err(make_net_error("net: invalid socket", 0))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_net_udp_set_broadcast(socket: FuseHandle, enabled: FuseHandle) -> FuseHandle {
    if let ValueKind::Data(data) = &(*socket).kind {
        if let Some(field0) = data.fields.first() {
            let ptr = *field0 as *mut dyn std::any::Any;
            if !ptr.is_null() {
                if let Some(s) = (*ptr).downcast_ref::<std::net::UdpSocket>() {
                    let en = match &(*enabled).kind { ValueKind::Bool(b) => *b, _ => false };
                    return match s.set_broadcast(en) {
                        Ok(()) => fuse_ok(fuse_unit()),
                        Err(e) => { let msg = format!("net: {e}"); fuse_err(make_net_error(&msg, net_error_code(&e))) }
                    };
                }
            }
        }
    }
    fuse_err(make_net_error("net: invalid socket", 0))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_net_udp_close(_socket: FuseHandle) -> FuseHandle {
    fuse_ok(fuse_unit())
}

// --- Random FFI ---

// Splitmix64 — simple, high-quality PRNG. State is a single i64.
fn splitmix64(state: i64) -> (i64, i64) {
    let s = state.wrapping_add(0x9e3779b97f4a7c15_u64 as i64);
    let mut z = s;
    z = (z ^ (z as u64 >> 30) as i64).wrapping_mul(0xbf58476d1ce4e5b9_u64 as i64);
    z = (z ^ (z as u64 >> 27) as i64).wrapping_mul(0x94d049bb133111eb_u64 as i64);
    z = z ^ (z as u64 >> 31) as i64;
    (s, z) // (new_state, output)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_random_new() -> FuseHandle {
    // Seed from system time nanos
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64;
    fuse_int(seed)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_random_seeded(seed: FuseHandle) -> FuseHandle {
    let s = match &(*seed).kind { ValueKind::Int(n) => *n, _ => 0 };
    fuse_int(s)
}

/// Returns a list [new_state, value] to allow functional state threading.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_random_next_int(state: FuseHandle) -> FuseHandle {
    let s = match &(*state).kind { ValueKind::Int(n) => *n, _ => 0 };
    let (new_state, val) = splitmix64(s);
    let list = fuse_list_new();
    fuse_list_push(list, fuse_int(new_state));
    fuse_list_push(list, fuse_int(val));
    list
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_random_next_float(state: FuseHandle) -> FuseHandle {
    let s = match &(*state).kind { ValueKind::Int(n) => *n, _ => 0 };
    let (new_state, val) = splitmix64(s);
    // Convert to [0.0, 1.0) by using upper 53 bits
    let f = ((val as u64) >> 11) as f64 / (1u64 << 53) as f64;
    let list = fuse_list_new();
    fuse_list_push(list, fuse_int(new_state));
    fuse_list_push(list, fuse_float(f));
    list
}

// --- Time FFI ---

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_time_instant_now() -> FuseHandle {
    let nanos = std::time::Instant::now().elapsed().as_nanos() as i64;
    // Use a thread-local base instant for monotonic measurement
    thread_local! {
        static BASE: std::time::Instant = std::time::Instant::now();
    }
    let n = BASE.with(|base| base.elapsed().as_nanos() as i64);
    fuse_int(n)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_time_system_now() -> FuseHandle {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    fuse_int(secs)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_time_elapsed_nanos(start_nanos: FuseHandle) -> FuseHandle {
    thread_local! {
        static BASE: std::time::Instant = std::time::Instant::now();
    }
    let now = BASE.with(|base| base.elapsed().as_nanos() as i64);
    let start = match &(*start_nanos).kind { ValueKind::Int(n) => *n, _ => 0 };
    fuse_int(now - start)
}

// --- Sys FFI ---

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_sys_args() -> FuseHandle {
    let list = fuse_list_new();
    for arg in std::env::args() {
        fuse_list_push(list, fuse_string_new_utf8(arg.as_ptr(), arg.len()));
    }
    list
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_sys_exit(code: FuseHandle) -> FuseHandle {
    let c = match &(*code).kind { ValueKind::Int(n) => *n as i32, _ => 1 };
    std::process::exit(c);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_sys_cwd() -> FuseHandle {
    match std::env::current_dir() {
        Ok(p) => {
            let s = p.to_string_lossy().to_string();
            fuse_ok(fuse_string_new_utf8(s.as_ptr(), s.len()))
        }
        Err(e) => {
            let msg = format!("sys: {e}");
            fuse_err(fuse_string_new_utf8(msg.as_ptr(), msg.len()))
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_sys_set_cwd(path: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    match std::env::set_current_dir(p) {
        Ok(()) => fuse_ok(fuse_unit()),
        Err(e) => {
            let msg = format!("sys: {e}");
            fuse_err(fuse_string_new_utf8(msg.as_ptr(), msg.len()))
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_sys_pid() -> FuseHandle {
    fuse_int(std::process::id() as i64)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_sys_platform() -> FuseHandle {
    let p = if cfg!(target_os = "windows") { "windows" }
        else if cfg!(target_os = "macos") { "macos" }
        else if cfg!(target_os = "linux") { "linux" }
        else { "unknown" };
    fuse_string_new_utf8(p.as_ptr(), p.len())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_sys_arch() -> FuseHandle {
    let a = if cfg!(target_arch = "x86_64") { "x86_64" }
        else if cfg!(target_arch = "aarch64") { "aarch64" }
        else if cfg!(target_arch = "x86") { "x86" }
        else { "unknown" };
    fuse_string_new_utf8(a.as_ptr(), a.len())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_sys_num_cpus() -> FuseHandle {
    fuse_int(std::thread::available_parallelism().map(|n| n.get() as i64).unwrap_or(1))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_sys_mem_total() -> FuseHandle {
    // No portable Rust API for total RAM. Return 0 as "unknown".
    fuse_int(0)
}

// --- List FFI helpers ---

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_list_len(list: FuseHandle) -> FuseHandle {
    fuse_int(fuse_list_len(list) as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_list_get(list: FuseHandle, index: FuseHandle) -> FuseHandle {
    let i = match &(*index).kind { ValueKind::Int(n) => *n as usize, _ => return fuse_none() };
    let len = fuse_list_len(list);
    if i < len { fuse_some(fuse_list_get(list, i)) } else { fuse_none() }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_list_push(list: FuseHandle, item: FuseHandle) -> FuseHandle {
    fuse_list_push(list, item);
    fuse_unit()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_list_pop(list: FuseHandle) -> FuseHandle {
    if let ValueKind::List(items) = &mut (*list).kind {
        match items.pop() {
            Some(item) => fuse_some(item),
            None => fuse_none(),
        }
    } else { fuse_none() }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_list_set(list: FuseHandle, index: FuseHandle, item: FuseHandle) -> FuseHandle {
    if let (ValueKind::List(items), ValueKind::Int(i)) = (&mut (*list).kind, &(*index).kind) {
        let idx = *i as usize;
        if idx < items.len() { items[idx] = item; }
    }
    fuse_unit()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_list_insert(list: FuseHandle, index: FuseHandle, item: FuseHandle) -> FuseHandle {
    if let (ValueKind::List(items), ValueKind::Int(i)) = (&mut (*list).kind, &(*index).kind) {
        let idx = (*i as usize).min(items.len());
        items.insert(idx, item);
    }
    fuse_unit()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_list_remove_at(list: FuseHandle, index: FuseHandle) -> FuseHandle {
    if let (ValueKind::List(items), ValueKind::Int(i)) = (&mut (*list).kind, &(*index).kind) {
        let idx = *i as usize;
        if idx < items.len() { return fuse_some(items.remove(idx)); }
    }
    fuse_none()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_list_clear(list: FuseHandle) -> FuseHandle {
    if let ValueKind::List(items) = &mut (*list).kind { items.clear(); }
    fuse_unit()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_list_slice(list: FuseHandle, start: FuseHandle, end: FuseHandle) -> FuseHandle {
    let result = fuse_list_new();
    if let ValueKind::List(items) = &(*list).kind {
        let s = match &(*start).kind { ValueKind::Int(n) => (*n as usize).min(items.len()), _ => 0 };
        let e = match &(*end).kind { ValueKind::Int(n) => (*n as usize).min(items.len()), _ => items.len() };
        for i in s..e { fuse_list_push(result, items[i]); }
    }
    result
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_list_concat(a: FuseHandle, b: FuseHandle) -> FuseHandle {
    let result = fuse_list_new();
    if let ValueKind::List(items) = &(*a).kind { for item in items { fuse_list_push(result, *item); } }
    if let ValueKind::List(items) = &(*b).kind { for item in items { fuse_list_push(result, *item); } }
    result
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_list_reverse(list: FuseHandle) -> FuseHandle {
    let result = fuse_list_new();
    if let ValueKind::List(items) = &(*list).kind {
        for item in items.iter().rev() { fuse_list_push(result, *item); }
    }
    result
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_list_reverse_in_place(list: FuseHandle) -> FuseHandle {
    if let ValueKind::List(items) = &mut (*list).kind { items.reverse(); }
    fuse_unit()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_list_join(list: FuseHandle, sep: FuseHandle) -> FuseHandle {
    let separator = extract_string(sep);
    let mut parts = Vec::new();
    if let ValueKind::List(items) = &(*list).kind {
        for item in items {
            let s = match &(**item).kind {
                ValueKind::String(s) => s.clone(),
                ValueKind::Int(n) => n.to_string(),
                ValueKind::Float(n) => { let s = n.to_string(); if s.contains('.') { s } else { format!("{s}.0") } },
                ValueKind::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
                _ => String::new(),
            };
            parts.push(s);
        }
    }
    let result = parts.join(separator);
    fuse_string_new_utf8(result.as_ptr(), result.len())
}

// --- String FFI helpers ---

fn extract_string(handle: FuseHandle) -> &'static str {
    unsafe { match &(*handle).kind { ValueKind::String(s) => s.as_str(), _ => "" } }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_to_lower(h: FuseHandle) -> FuseHandle {
    let s = extract_string(h).to_lowercase();
    fuse_string_new_utf8(s.as_ptr(), s.len())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_contains(h: FuseHandle, sub: FuseHandle) -> FuseHandle {
    fuse_bool(extract_string(h).contains(extract_string(sub)))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_starts_with(h: FuseHandle, prefix: FuseHandle) -> FuseHandle {
    fuse_bool(extract_string(h).starts_with(extract_string(prefix)))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_ends_with(h: FuseHandle, suffix: FuseHandle) -> FuseHandle {
    fuse_bool(extract_string(h).ends_with(extract_string(suffix)))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_index_of(h: FuseHandle, sub: FuseHandle) -> FuseHandle {
    let s = extract_string(h);
    let needle = extract_string(sub);
    let idx = s.char_indices()
        .zip(s.char_indices().skip(needle.len()).map(|(i,_)| i).chain(std::iter::once(s.len())))
        .enumerate()
        .find(|(_, ((start, _), end))| &s[*start..*end] == needle)
        .map(|(char_idx, _)| char_idx as i64)
        .unwrap_or(-1);
    fuse_int(idx)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_last_index_of(h: FuseHandle, sub: FuseHandle) -> FuseHandle {
    let s = extract_string(h);
    let needle = extract_string(sub);
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut result: i64 = -1;
    for (char_idx, (byte_start, _)) in chars.iter().enumerate() {
        let byte_end = chars.get(char_idx + needle.chars().count()).map(|(b,_)| *b).unwrap_or(s.len());
        if &s[*byte_start..byte_end] == needle {
            result = char_idx as i64;
        }
    }
    fuse_int(result)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_trim(h: FuseHandle) -> FuseHandle {
    let s = extract_string(h).trim();
    fuse_string_new_utf8(s.as_ptr(), s.len())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_trim_start(h: FuseHandle) -> FuseHandle {
    let s = extract_string(h).trim_start();
    fuse_string_new_utf8(s.as_ptr(), s.len())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_trim_end(h: FuseHandle) -> FuseHandle {
    let s = extract_string(h).trim_end();
    fuse_string_new_utf8(s.as_ptr(), s.len())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_replace(h: FuseHandle, from: FuseHandle, to: FuseHandle) -> FuseHandle {
    let s = extract_string(h).replace(extract_string(from), extract_string(to));
    fuse_string_new_utf8(s.as_ptr(), s.len())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_replace_first(h: FuseHandle, from: FuseHandle, to: FuseHandle) -> FuseHandle {
    let s = extract_string(h).replacen(extract_string(from), extract_string(to), 1);
    fuse_string_new_utf8(s.as_ptr(), s.len())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_split(h: FuseHandle, sep: FuseHandle) -> FuseHandle {
    let parts: Vec<&str> = extract_string(h).split(extract_string(sep)).collect();
    let list = fuse_list_new();
    for part in parts {
        fuse_list_push(list, fuse_string_new_utf8(part.as_ptr(), part.len()));
    }
    list
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_to_bytes(h: FuseHandle) -> FuseHandle {
    let list = fuse_list_new();
    for byte in extract_string(h).as_bytes() {
        fuse_list_push(list, fuse_int(*byte as i64));
    }
    list
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_from_bytes(list: FuseHandle) -> FuseHandle {
    if let ValueKind::List(items) = &(*list).kind {
        let bytes: Vec<u8> = items.iter().filter_map(|h| {
            match &(**h).kind { ValueKind::Int(n) => Some(*n as u8), _ => None }
        }).collect();
        match String::from_utf8(bytes) {
            Ok(s) => return fuse_ok(fuse_string_new_utf8(s.as_ptr(), s.len())),
            Err(e) => {
                let msg = format!("string: invalid UTF-8: {e}");
                return fuse_err(fuse_string_new_utf8(msg.as_ptr(), msg.len()));
            }
        }
    }
    let msg = "string: expected byte list";
    fuse_err(fuse_string_new_utf8(msg.as_ptr(), msg.len()))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_from_char_code(code: FuseHandle) -> FuseHandle {
    if let ValueKind::Int(n) = &(*code).kind {
        if let Some(ch) = char::from_u32(*n as u32) {
            let s = ch.to_string();
            return fuse_string_new_utf8(s.as_ptr(), s.len());
        }
    }
    fuse_string_new_utf8(b"".as_ptr(), 0)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_chars_list(h: FuseHandle) -> FuseHandle {
    let list = fuse_list_new();
    for ch in extract_string(h).chars() {
        let s = ch.to_string();
        fuse_list_push(list, fuse_string_new_utf8(s.as_ptr(), s.len()));
    }
    list
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_reverse(h: FuseHandle) -> FuseHandle {
    let s: String = extract_string(h).chars().rev().collect();
    fuse_string_new_utf8(s.as_ptr(), s.len())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_compare(a: FuseHandle, b: FuseHandle) -> FuseHandle {
    let result = extract_string(a).cmp(extract_string(b));
    fuse_int(match result { std::cmp::Ordering::Less => -1, std::cmp::Ordering::Equal => 0, std::cmp::Ordering::Greater => 1 })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_byte_len(h: FuseHandle) -> FuseHandle {
    fuse_int(extract_string(h).len() as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_capitalize(h: FuseHandle) -> FuseHandle {
    let s = extract_string(h);
    let mut chars = s.chars();
    let result = match chars.next() {
        Some(first) => {
            let upper: String = first.to_uppercase().collect();
            let lower: String = chars.collect::<String>().to_lowercase();
            format!("{upper}{lower}")
        }
        None => String::new(),
    };
    fuse_string_new_utf8(result.as_ptr(), result.len())
}

// --- Fmt FFI helpers ---

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_float_to_string_scientific(h: FuseHandle, decimals: FuseHandle) -> FuseHandle {
    let v = extract_float(h);
    let d = match &(*decimals).kind { ValueKind::Int(n) => *n as usize, _ => 2 };
    let s = format!("{v:.prec$e}", prec = d);
    fuse_string_new_utf8(s.as_ptr(), s.len())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_slice(h: FuseHandle, start: FuseHandle, end: FuseHandle) -> FuseHandle {
    if let ValueKind::String(s) = &(*h).kind {
        let s_start = match &(*start).kind { ValueKind::Int(n) => *n as usize, _ => 0 };
        let s_end = match &(*end).kind { ValueKind::Int(n) => *n as usize, _ => s.len() };
        let result: String = s.chars().skip(s_start).take(s_end.saturating_sub(s_start)).collect();
        return fuse_string_new_utf8(result.as_ptr(), result.len());
    }
    fuse_string_new_utf8(b"".as_ptr(), 0)
}

// --- Math FFI helpers ---

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_math_sin(h: FuseHandle) -> FuseHandle { fuse_float(extract_float(h).sin()) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_math_cos(h: FuseHandle) -> FuseHandle { fuse_float(extract_float(h).cos()) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_math_tan(h: FuseHandle) -> FuseHandle { fuse_float(extract_float(h).tan()) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_math_asin(h: FuseHandle) -> FuseHandle { fuse_float(extract_float(h).asin()) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_math_acos(h: FuseHandle) -> FuseHandle { fuse_float(extract_float(h).acos()) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_math_atan(h: FuseHandle) -> FuseHandle { fuse_float(extract_float(h).atan()) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_math_atan2(y: FuseHandle, x: FuseHandle) -> FuseHandle { fuse_float(extract_float(y).atan2(extract_float(x))) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_math_exp(h: FuseHandle) -> FuseHandle { fuse_float(extract_float(h).exp()) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_math_exp2(h: FuseHandle) -> FuseHandle { fuse_float(extract_float(h).exp2()) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_math_ln(h: FuseHandle) -> FuseHandle { fuse_float(extract_float(h).ln()) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_math_log2(h: FuseHandle) -> FuseHandle { fuse_float(extract_float(h).log2()) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_math_log10(h: FuseHandle) -> FuseHandle { fuse_float(extract_float(h).log10()) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_math_cbrt(h: FuseHandle) -> FuseHandle { fuse_float(extract_float(h).cbrt()) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_math_hypot(a: FuseHandle, b: FuseHandle) -> FuseHandle { fuse_float(extract_float(a).hypot(extract_float(b))) }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_float_to_string_fixed(h: FuseHandle, decimals: FuseHandle) -> FuseHandle {
    let v = extract_float(h);
    let d = match &(*decimals).kind { ValueKind::Int(n) => *n as usize, _ => 2 };
    let s = format!("{v:.prec$}", prec = d);
    fuse_string_new_utf8(s.as_ptr(), s.len())
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
