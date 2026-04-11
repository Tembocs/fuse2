use std::fmt::Write;
use std::ptr;
use std::slice;
use std::collections::VecDeque;
use std::cell::Cell;
use std::cell::RefCell;
use std::collections::HashMap;

pub type FuseHandle = *mut FuseValue;

pub type FuseDestructor = Option<unsafe extern "C" fn(FuseHandle)>;

pub struct FuseValue {
    released: bool,
    kind: ValueKind,
}

enum ValueKind {
    Int(i64),
    Float(f64),
    Float32(f32),
    Int8(i8),
    UInt8(u8),
    Int32(i32),
    UInt32(u32),
    UInt64(u64),
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
    closed: bool,
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
        ValueKind::Float(value) => { let s = value.to_string(); if s.contains('.') { s } else { format!("{s}.0") } },
        ValueKind::Float32(value) => { let s = value.to_string(); if s.contains('.') { s } else { format!("{s}.0") } },
        ValueKind::Int8(value) => value.to_string(),
        ValueKind::UInt8(value) => value.to_string(),
        ValueKind::Int32(value) => value.to_string(),
        ValueKind::UInt32(value) => value.to_string(),
        ValueKind::UInt64(value) => value.to_string(),
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

fn numeric_binary(lhs: FuseHandle, rhs: FuseHandle, int_op: fn(i64, i64) -> i64, float_op: fn(f64, f64) -> f64) -> FuseHandle {
    unsafe {
        match (&value_ref(lhs).kind, &value_ref(rhs).kind) {
            (ValueKind::Int(left), ValueKind::Int(right)) => fuse_int(int_op(*left, *right)),
            (ValueKind::Float(left), ValueKind::Float(right)) => fuse_float(float_op(*left, *right)),
            (ValueKind::Int(left), ValueKind::Float(right)) => fuse_float(float_op(*left as f64, *right)),
            (ValueKind::Float(left), ValueKind::Int(right)) => fuse_float(float_op(*left, *right as f64)),
            (ValueKind::Float32(left), ValueKind::Float32(right)) => fuse_rt_f32_new(float_op(*left as f64, *right as f64)),
            _ => fuse_unit(),
        }
    }
}

fn numeric_compare(lhs: FuseHandle, rhs: FuseHandle, int_op: fn(i64, i64) -> bool, float_op: fn(f64, f64) -> bool) -> FuseHandle {
    unsafe {
        match (&value_ref(lhs).kind, &value_ref(rhs).kind) {
            (ValueKind::Int(left), ValueKind::Int(right)) => fuse_bool(int_op(*left, *right)),
            (ValueKind::Float(left), ValueKind::Float(right)) => fuse_bool(float_op(*left, *right)),
            (ValueKind::Int(left), ValueKind::Float(right)) => fuse_bool(float_op(*left as f64, *right)),
            (ValueKind::Float(left), ValueKind::Int(right)) => fuse_bool(float_op(*left, *right as f64)),
            (ValueKind::Float32(left), ValueKind::Float32(right)) => fuse_bool(float_op(*left as f64, *right as f64)),
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
            (ValueKind::Float(left), ValueKind::Float(right)) => fuse_float(left + right),
            (ValueKind::Int(left), ValueKind::Float(right)) => fuse_float(*left as f64 + right),
            (ValueKind::Float(left), ValueKind::Int(right)) => fuse_float(left + *right as f64),
            _ => fuse_concat(lhs, rhs),
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_sub(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    numeric_binary(lhs, rhs, |left, right| left - right, |left, right| left - right)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_mul(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    numeric_binary(lhs, rhs, |left, right| left * right, |left, right| left * right)
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
    numeric_binary(lhs, rhs, |left, right| left % right, |left, right| left % right)
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
    numeric_compare(lhs, rhs, |left, right| left < right, |left, right| left < right)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_le(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    numeric_compare(lhs, rhs, |left, right| left <= right, |left, right| left <= right)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_gt(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    numeric_compare(lhs, rhs, |left, right| left > right, |left, right| left > right)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_ge(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    numeric_compare(lhs, rhs, |left, right| left >= right, |left, right| left >= right)
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
            ValueKind::Float32(value) => *value != 0.0,
            ValueKind::Int8(value) => *value != 0,
            ValueKind::UInt8(value) => *value != 0,
            ValueKind::Int32(value) => *value != 0,
            ValueKind::UInt32(value) => *value != 0,
            ValueKind::UInt64(value) => *value != 0,
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

/// Extract the raw i64 value from an Int handle. Returns 0 for non-Int values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_extract_int(handle: FuseHandle) -> i64 {
    extract_int(handle)
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
pub unsafe extern "C" fn fuse_enum_add_payload(handle: FuseHandle, payload: FuseHandle) {
    unsafe {
        if let ValueKind::Enum(e) = &mut value_mut(handle).kind {
            e.payloads.push(payload);
        }
    }
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

/// Like fuse_list_get but takes a boxed Int index (FuseHandle).
/// Used by Fuse-level code where the index is a Fuse Int value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_list_get_handle(list: FuseHandle, index: FuseHandle) -> FuseHandle {
    unsafe {
        let i = match &(*index).kind {
            ValueKind::Int(n) => *n as usize,
            _ => return ptr::null_mut(),
        };
        fuse_list_get(list, i)
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

// --- Map FFI helpers ---

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_map_get(map: FuseHandle, key: FuseHandle) -> FuseHandle {
    unsafe {
        let raw = fuse_map_get(map, key);
        if raw.is_null() { fuse_none() } else { fuse_some(raw) }
    }
}

// --- SIMD scalar fallback helpers ---

unsafe fn simd_extract_f64(h: FuseHandle) -> f64 {
    unsafe {
        match &value_ref(h).kind {
            ValueKind::Float(v) => *v,
            ValueKind::Int(v) => *v as f64,
            _ => 0.0,
        }
    }
}

unsafe fn simd_extract_i64(h: FuseHandle) -> i64 {
    unsafe {
        match &value_ref(h).kind {
            ValueKind::Int(v) => *v,
            ValueKind::Float(v) => *v as i64,
            _ => 0,
        }
    }
}

unsafe fn simd_is_float_list(items: &[FuseHandle]) -> bool {
    unsafe {
        items.iter().any(|h| matches!(&value_ref(*h).kind, ValueKind::Float(_)))
    }
}

unsafe fn simd_list_items(handle: FuseHandle) -> &'static [FuseHandle] {
    unsafe {
        match &value_ref(handle).kind {
            ValueKind::List(items) => items.as_slice(),
            _ => &[],
        }
    }
}

// --- SIMD operations ---

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_sum(list: FuseHandle) -> FuseHandle {
    unsafe {
        let items = simd_list_items(list);
        if items.is_empty() {
            return fuse_int(0);
        }
        if simd_is_float_list(items) {
            let total: f64 = items.iter().map(|h| simd_extract_f64(*h)).sum();
            fuse_float(total)
        } else {
            let total: i64 = items.iter().map(|h| simd_extract_i64(*h)).sum();
            fuse_int(total)
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_dot(a: FuseHandle, b: FuseHandle) -> FuseHandle {
    unsafe {
        let items_a = simd_list_items(a);
        let items_b = simd_list_items(b);
        let len = items_a.len().min(items_b.len());
        if len == 0 {
            return fuse_int(0);
        }
        let is_float = simd_is_float_list(items_a) || simd_is_float_list(items_b);
        if is_float {
            let total: f64 = (0..len)
                .map(|i| simd_extract_f64(items_a[i]) * simd_extract_f64(items_b[i]))
                .sum();
            fuse_float(total)
        } else {
            let total: i64 = (0..len)
                .map(|i| simd_extract_i64(items_a[i]) * simd_extract_i64(items_b[i]))
                .sum();
            fuse_int(total)
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_add(a: FuseHandle, b: FuseHandle) -> FuseHandle {
    unsafe { simd_elementwise_op(a, b, |x, y| x + y, |x, y| x + y) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_sub(a: FuseHandle, b: FuseHandle) -> FuseHandle {
    unsafe { simd_elementwise_op(a, b, |x, y| x - y, |x, y| x - y) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_mul(a: FuseHandle, b: FuseHandle) -> FuseHandle {
    unsafe { simd_elementwise_op(a, b, |x, y| x * y, |x, y| x * y) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_div(a: FuseHandle, b: FuseHandle) -> FuseHandle {
    unsafe {
        simd_elementwise_op(
            a, b,
            |x, y| if y != 0 { x / y } else { 0 },
            |x, y| if y != 0.0 { x / y } else { 0.0 },
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_min(a: FuseHandle, b: FuseHandle) -> FuseHandle {
    unsafe {
        simd_elementwise_op(a, b, |x, y| x.min(y), |x: f64, y: f64| x.min(y))
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_max(a: FuseHandle, b: FuseHandle) -> FuseHandle {
    unsafe {
        simd_elementwise_op(a, b, |x, y| x.max(y), |x: f64, y: f64| x.max(y))
    }
}

unsafe fn simd_elementwise_op(
    a: FuseHandle,
    b: FuseHandle,
    int_op: impl Fn(i64, i64) -> i64,
    float_op: impl Fn(f64, f64) -> f64,
) -> FuseHandle {
    unsafe {
        let items_a = simd_list_items(a);
        let items_b = simd_list_items(b);
        let len = items_a.len().min(items_b.len());
        let result = fuse_list_new();
        let is_float = simd_is_float_list(items_a) || simd_is_float_list(items_b);
        for i in 0..len {
            if is_float {
                let v = float_op(simd_extract_f64(items_a[i]), simd_extract_f64(items_b[i]));
                fuse_list_push(result, fuse_float(v));
            } else {
                let v = int_op(simd_extract_i64(items_a[i]), simd_extract_i64(items_b[i]));
                fuse_list_push(result, fuse_int(v));
            }
        }
        result
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_abs(list: FuseHandle) -> FuseHandle {
    unsafe {
        let items = simd_list_items(list);
        let result = fuse_list_new();
        let is_float = simd_is_float_list(items);
        for item in items {
            if is_float {
                fuse_list_push(result, fuse_float(simd_extract_f64(*item).abs()));
            } else {
                fuse_list_push(result, fuse_int(simd_extract_i64(*item).abs()));
            }
        }
        result
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_sqrt(list: FuseHandle) -> FuseHandle {
    unsafe {
        let items = simd_list_items(list);
        let result = fuse_list_new();
        for item in items {
            let v = simd_extract_f64(*item).sqrt();
            fuse_list_push(result, fuse_float(v));
        }
        result
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_broadcast(value: FuseHandle, lanes: i64) -> FuseHandle {
    unsafe {
        let result = fuse_list_new();
        for _ in 0..lanes {
            // Push the same handle — values are immutable at this level
            fuse_list_push(result, value);
        }
        result
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_get(list: FuseHandle, index: FuseHandle) -> FuseHandle {
    unsafe {
        let idx = extract_int(index) as usize;
        let items = simd_list_items(list);
        items.get(idx).copied().unwrap_or(fuse_int(0))
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_len(list: FuseHandle) -> FuseHandle {
    unsafe {
        let items = simd_list_items(list);
        fuse_int(items.len() as i64)
    }
}

// --- SIMD raw extraction helpers (used by inline Cranelift vector codegen) ---

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_extract_raw_f64(h: FuseHandle) -> f64 {
    if h.is_null() { return 0.0; }
    unsafe { simd_extract_f64(h) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_extract_raw_f32(h: FuseHandle) -> f32 {
    if h.is_null() { return 0.0; }
    unsafe { simd_extract_f64(h) as f32 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_extract_raw_i64(h: FuseHandle) -> i64 {
    if h.is_null() { return 0; }
    unsafe { simd_extract_i64(h) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_extract_raw_i32(h: FuseHandle) -> i32 {
    if h.is_null() { return 0; }
    unsafe { simd_extract_i64(h) as i32 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_new() -> FuseHandle {
    FuseValue::new(ValueKind::Channel(ChannelValue {
        items: VecDeque::new(),
        pending: VecDeque::new(),
        capacity: None,
        closed: false,
    }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_bounded(capacity: usize) -> FuseHandle {
    FuseValue::new(ValueKind::Channel(ChannelValue {
        items: VecDeque::new(),
        pending: VecDeque::new(),
        capacity: Some(capacity),
        closed: false,
    }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_send(chan: FuseHandle, value: FuseHandle) -> FuseHandle {
    unsafe {
        if let ValueKind::Channel(channel) = &mut value_mut(chan).kind {
            if channel.closed {
                let msg = "channel closed";
                return fuse_err(fuse_string_new_utf8(msg.as_ptr(), msg.len()));
            }
            let is_full = channel
                .capacity
                .is_some_and(|capacity| channel.items.len() >= capacity);
            if !is_full {
                channel.items.push_back(value);
            } else {
                channel.pending.push_back(value);
            }
            return fuse_ok(fuse_unit());
        }
        let msg = "not a channel";
        fuse_err(fuse_string_new_utf8(msg.as_ptr(), msg.len()))
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_recv(chan: FuseHandle) -> FuseHandle {
    unsafe {
        match &mut value_mut(chan).kind {
            ValueKind::Channel(channel) => {
                if let Some(value) = channel.items.pop_front() {
                    let can_promote = channel
                        .capacity
                        .is_none_or(|capacity| channel.items.len() < capacity);
                    if can_promote {
                        if let Some(next) = channel.pending.pop_front() {
                            channel.items.push_back(next);
                        }
                    }
                    return fuse_ok(value);
                }
                let msg = if channel.closed { "channel closed" } else { "channel empty" };
                fuse_err(fuse_string_new_utf8(msg.as_ptr(), msg.len()))
            }
            _ => {
                let msg = "not a channel";
                fuse_err(fuse_string_new_utf8(msg.as_ptr(), msg.len()))
            }
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_try_recv(chan: FuseHandle) -> FuseHandle {
    unsafe {
        match &mut value_mut(chan).kind {
            ValueKind::Channel(channel) => {
                if let Some(value) = channel.items.pop_front() {
                    let can_promote = channel
                        .capacity
                        .is_none_or(|capacity| channel.items.len() < capacity);
                    if can_promote {
                        if let Some(next) = channel.pending.pop_front() {
                            channel.items.push_back(next);
                        }
                    }
                    fuse_some(value)
                } else {
                    fuse_none()
                }
            }
            _ => fuse_none(),
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_close(chan: FuseHandle) {
    unsafe {
        if let ValueKind::Channel(channel) = &mut value_mut(chan).kind {
            channel.closed = true;
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_is_closed(chan: FuseHandle) -> bool {
    unsafe {
        match &value_ref(chan).kind {
            ValueKind::Channel(channel) => channel.closed,
            _ => false,
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_len(chan: FuseHandle) -> i64 {
    unsafe {
        match &value_ref(chan).kind {
            ValueKind::Channel(channel) => channel.items.len() as i64,
            _ => 0,
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_cap(chan: FuseHandle) -> FuseHandle {
    unsafe {
        match &value_ref(chan).kind {
            ValueKind::Channel(channel) => match channel.capacity {
                Some(cap) => fuse_some(fuse_int(cap as i64)),
                None => fuse_none(),
            },
            _ => fuse_none(),
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
        ValueKind::Float32(v) => unsafe { fuse_rt_f32_new(*v as f64) },
        ValueKind::Int8(v) => FuseValue::new(ValueKind::Int8(*v)),
        ValueKind::UInt8(v) => FuseValue::new(ValueKind::UInt8(*v)),
        ValueKind::Int32(v) => FuseValue::new(ValueKind::Int32(*v)),
        ValueKind::UInt32(v) => FuseValue::new(ValueKind::UInt32(*v)),
        ValueKind::UInt64(v) => FuseValue::new(ValueKind::UInt64(*v)),
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

/// `try_read(timeout)` — like `try_write` but returns a clone (snapshot).
/// In single-threaded Stage 1 the lock is always free, so the positive path
/// always succeeds.  A timeout of 0 forces the error path for testing.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_shared_try_read(
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
            ValueKind::Shared(value) => fuse_ok(clone_value(*value)),
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
        return fuse_int(s.len() as i64);
    }
    fuse_int(0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_char_count(handle: FuseHandle) -> FuseHandle {
    if let ValueKind::String(s) = &(*handle).kind {
        return fuse_int(s.chars().count() as i64);
    }
    fuse_int(0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_string_byte_at(handle: FuseHandle, index: FuseHandle) -> FuseHandle {
    if let (ValueKind::String(s), ValueKind::Int(i)) = (&(*handle).kind, &(*index).kind) {
        let idx = *i as usize;
        if idx < s.len() {
            return fuse_int(s.as_bytes()[idx] as i64);
        }
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
// --- Float32 FFI helpers ---

/// Handle-based wrapper: accepts a FuseHandle containing a Float and
/// returns a FuseHandle containing a Float32. Used by Fuse extern fn
/// declarations where all values are handles.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_f32_box(handle: FuseHandle) -> FuseHandle {
    let f = unsafe { extract_float(handle) };
    FuseValue::new(ValueKind::Float32(f as f32))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_f32_new(value: f64) -> FuseHandle {
    FuseValue::new(ValueKind::Float32(value as f32))
}

unsafe fn extract_f32(handle: FuseHandle) -> f32 {
    unsafe {
        match &(*handle).kind {
            ValueKind::Float32(v) => *v,
            ValueKind::Float(v) => *v as f32,
            ValueKind::Int(v) => *v as f32,
            _ => 0.0,
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_f32_value(handle: FuseHandle) -> f64 {
    extract_f32(handle) as f64
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_f32_add(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_rt_f32_new((extract_f32(lhs) + extract_f32(rhs)) as f64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_f32_sub(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_rt_f32_new((extract_f32(lhs) - extract_f32(rhs)) as f64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_f32_mul(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_rt_f32_new((extract_f32(lhs) * extract_f32(rhs)) as f64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_f32_div(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_rt_f32_new((extract_f32(lhs) / extract_f32(rhs)) as f64)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_f32_eq(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_f32(lhs) == extract_f32(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_f32_lt(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_f32(lhs) < extract_f32(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_f32_gt(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_f32(lhs) > extract_f32(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_f32_le(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_f32(lhs) <= extract_f32(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_f32_ge(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_f32(lhs) >= extract_f32(rhs))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_f32_to_string(handle: FuseHandle) -> FuseHandle {
    let s = {
        let v = extract_f32(handle).to_string();
        if v.contains('.') { v } else { format!("{v}.0") }
    };
    fuse_string_new_utf8(s.as_ptr(), s.len())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_f32_abs(handle: FuseHandle) -> FuseHandle {
    fuse_rt_f32_new(extract_f32(handle).abs() as f64)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_f32_sqrt(handle: FuseHandle) -> FuseHandle {
    fuse_rt_f32_new(extract_f32(handle).sqrt() as f64)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_f32_to_int(handle: FuseHandle) -> FuseHandle {
    fuse_int(extract_f32(handle) as i64)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_f32_to_float(handle: FuseHandle) -> FuseHandle {
    fuse_float(extract_f32(handle) as f64)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_f32_from_int(handle: FuseHandle) -> FuseHandle {
    let n = unsafe { match &(*handle).kind { ValueKind::Int(v) => *v, _ => 0 } };
    fuse_rt_f32_new(n as f64)
}

// --- Sized integer FFI ---
//
// Design: all sized integers cross the Cranelift ABI boundary as `i64`.
// The runtime narrows on store (truncating cast) and widens on load (sign-
// or zero-extending cast). Arithmetic uses wrapping semantics — overflow is
// well-defined and never panics. Division and remainder by zero return 0.

// ---- helpers ----

fn extract_i8(handle: FuseHandle) -> i8 {
    unsafe { match &(*handle).kind { ValueKind::Int8(v) => *v, ValueKind::Int(v) => *v as i8, _ => 0 } }
}
fn extract_u8(handle: FuseHandle) -> u8 {
    unsafe { match &(*handle).kind { ValueKind::UInt8(v) => *v, ValueKind::Int(v) => *v as u8, _ => 0 } }
}
fn extract_i32(handle: FuseHandle) -> i32 {
    unsafe { match &(*handle).kind { ValueKind::Int32(v) => *v, ValueKind::Int(v) => *v as i32, _ => 0 } }
}
fn extract_u32(handle: FuseHandle) -> u32 {
    unsafe { match &(*handle).kind { ValueKind::UInt32(v) => *v, ValueKind::Int(v) => *v as u32, _ => 0 } }
}
fn extract_u64(handle: FuseHandle) -> u64 {
    unsafe { match &(*handle).kind { ValueKind::UInt64(v) => *v, ValueKind::Int(v) => *v as u64, _ => 0 } }
}

// ---- Box functions: convert a FuseHandle (boxed Int) to a sized integer ----

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i8_box(handle: FuseHandle) -> FuseHandle {
    FuseValue::new(ValueKind::Int8(extract_i8(handle)))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u8_box(handle: FuseHandle) -> FuseHandle {
    FuseValue::new(ValueKind::UInt8(extract_u8(handle)))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i32_box(handle: FuseHandle) -> FuseHandle {
    FuseValue::new(ValueKind::Int32(extract_i32(handle)))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u32_box(handle: FuseHandle) -> FuseHandle {
    FuseValue::new(ValueKind::UInt32(extract_u32(handle)))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u64_box(handle: FuseHandle) -> FuseHandle {
    FuseValue::new(ValueKind::UInt64(extract_u64(handle)))
}

// ---- Conversion functions: sized integers ↔ Int and between sizes ----

// toInt: sized → Int (always lossless for all five types within i64 range)

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i8_to_int(handle: FuseHandle) -> FuseHandle {
    fuse_int(extract_i8(handle) as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u8_to_int(handle: FuseHandle) -> FuseHandle {
    fuse_int(extract_u8(handle) as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i32_to_int(handle: FuseHandle) -> FuseHandle {
    fuse_int(extract_i32(handle) as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u32_to_int(handle: FuseHandle) -> FuseHandle {
    fuse_int(extract_u32(handle) as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u64_to_int(handle: FuseHandle) -> FuseHandle {
    fuse_int(extract_u64(handle) as i64)
}

// Widening: Int8 → Int32
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i8_to_i32(handle: FuseHandle) -> FuseHandle {
    FuseValue::new(ValueKind::Int32(extract_i8(handle) as i32))
}
// Widening: UInt8 → Int32, UInt32, UInt64
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u8_to_i32(handle: FuseHandle) -> FuseHandle {
    FuseValue::new(ValueKind::Int32(extract_u8(handle) as i32))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u8_to_u32(handle: FuseHandle) -> FuseHandle {
    FuseValue::new(ValueKind::UInt32(extract_u8(handle) as u32))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u8_to_u64(handle: FuseHandle) -> FuseHandle {
    FuseValue::new(ValueKind::UInt64(extract_u8(handle) as u64))
}
// Widening: Int32 → UInt64 (when non-negative, handled by caller)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i32_to_u64(handle: FuseHandle) -> FuseHandle {
    FuseValue::new(ValueKind::UInt64(extract_i32(handle) as u64))
}
// Widening: UInt32 → UInt64
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u32_to_u64(handle: FuseHandle) -> FuseHandle {
    FuseValue::new(ValueKind::UInt64(extract_u32(handle) as u64))
}

// ---- Int8 ----

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i8_new(value: i64) -> FuseHandle {
    FuseValue::new(ValueKind::Int8(value as i8))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i8_value(handle: FuseHandle) -> i64 {
    extract_i8(handle) as i64
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i8_add(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_rt_i8_new(extract_i8(lhs).wrapping_add(extract_i8(rhs)) as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i8_sub(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_rt_i8_new(extract_i8(lhs).wrapping_sub(extract_i8(rhs)) as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i8_mul(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_rt_i8_new(extract_i8(lhs).wrapping_mul(extract_i8(rhs)) as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i8_div(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    let r = extract_i8(rhs);
    fuse_rt_i8_new(if r == 0 { 0 } else { extract_i8(lhs).wrapping_div(r) } as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i8_mod(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    let r = extract_i8(rhs);
    fuse_rt_i8_new(if r == 0 { 0 } else { extract_i8(lhs).wrapping_rem(r) } as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i8_eq(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_i8(lhs) == extract_i8(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i8_lt(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_i8(lhs) < extract_i8(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i8_le(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_i8(lhs) <= extract_i8(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i8_gt(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_i8(lhs) > extract_i8(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i8_ge(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_i8(lhs) >= extract_i8(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i8_to_string(handle: FuseHandle) -> FuseHandle {
    let s = extract_i8(handle).to_string();
    fuse_string_new_utf8(s.as_ptr(), s.len())
}

// ---- UInt8 ----

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u8_new(value: i64) -> FuseHandle {
    FuseValue::new(ValueKind::UInt8(value as u8))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u8_value(handle: FuseHandle) -> i64 {
    extract_u8(handle) as i64
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u8_add(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_rt_u8_new(extract_u8(lhs).wrapping_add(extract_u8(rhs)) as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u8_sub(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_rt_u8_new(extract_u8(lhs).wrapping_sub(extract_u8(rhs)) as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u8_mul(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_rt_u8_new(extract_u8(lhs).wrapping_mul(extract_u8(rhs)) as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u8_div(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    let r = extract_u8(rhs);
    fuse_rt_u8_new(if r == 0 { 0 } else { extract_u8(lhs).wrapping_div(r) } as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u8_mod(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    let r = extract_u8(rhs);
    fuse_rt_u8_new(if r == 0 { 0 } else { extract_u8(lhs).wrapping_rem(r) } as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u8_eq(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_u8(lhs) == extract_u8(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u8_lt(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_u8(lhs) < extract_u8(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u8_le(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_u8(lhs) <= extract_u8(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u8_gt(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_u8(lhs) > extract_u8(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u8_ge(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_u8(lhs) >= extract_u8(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u8_to_string(handle: FuseHandle) -> FuseHandle {
    let s = extract_u8(handle).to_string();
    fuse_string_new_utf8(s.as_ptr(), s.len())
}

// ---- Int32 ----

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i32_new(value: i64) -> FuseHandle {
    FuseValue::new(ValueKind::Int32(value as i32))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i32_value(handle: FuseHandle) -> i64 {
    extract_i32(handle) as i64
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i32_add(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_rt_i32_new(extract_i32(lhs).wrapping_add(extract_i32(rhs)) as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i32_sub(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_rt_i32_new(extract_i32(lhs).wrapping_sub(extract_i32(rhs)) as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i32_mul(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_rt_i32_new(extract_i32(lhs).wrapping_mul(extract_i32(rhs)) as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i32_div(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    let r = extract_i32(rhs);
    fuse_rt_i32_new(if r == 0 { 0 } else { extract_i32(lhs).wrapping_div(r) } as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i32_mod(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    let r = extract_i32(rhs);
    fuse_rt_i32_new(if r == 0 { 0 } else { extract_i32(lhs).wrapping_rem(r) } as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i32_eq(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_i32(lhs) == extract_i32(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i32_lt(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_i32(lhs) < extract_i32(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i32_le(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_i32(lhs) <= extract_i32(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i32_gt(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_i32(lhs) > extract_i32(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i32_ge(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_i32(lhs) >= extract_i32(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_i32_to_string(handle: FuseHandle) -> FuseHandle {
    let s = extract_i32(handle).to_string();
    fuse_string_new_utf8(s.as_ptr(), s.len())
}

// ---- UInt32 ----

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u32_new(value: i64) -> FuseHandle {
    FuseValue::new(ValueKind::UInt32(value as u32))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u32_value(handle: FuseHandle) -> i64 {
    extract_u32(handle) as i64
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u32_add(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_rt_u32_new(extract_u32(lhs).wrapping_add(extract_u32(rhs)) as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u32_sub(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_rt_u32_new(extract_u32(lhs).wrapping_sub(extract_u32(rhs)) as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u32_mul(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_rt_u32_new(extract_u32(lhs).wrapping_mul(extract_u32(rhs)) as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u32_div(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    let r = extract_u32(rhs);
    fuse_rt_u32_new(if r == 0 { 0 } else { extract_u32(lhs).wrapping_div(r) } as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u32_mod(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    let r = extract_u32(rhs);
    fuse_rt_u32_new(if r == 0 { 0 } else { extract_u32(lhs).wrapping_rem(r) } as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u32_eq(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_u32(lhs) == extract_u32(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u32_lt(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_u32(lhs) < extract_u32(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u32_le(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_u32(lhs) <= extract_u32(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u32_gt(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_u32(lhs) > extract_u32(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u32_ge(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_u32(lhs) >= extract_u32(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u32_to_string(handle: FuseHandle) -> FuseHandle {
    let s = extract_u32(handle).to_string();
    fuse_string_new_utf8(s.as_ptr(), s.len())
}

// ---- UInt64 ----

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u64_new(value: i64) -> FuseHandle {
    FuseValue::new(ValueKind::UInt64(value as u64))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u64_value(handle: FuseHandle) -> i64 {
    extract_u64(handle) as i64
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u64_add(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_rt_u64_new(extract_u64(lhs).wrapping_add(extract_u64(rhs)) as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u64_sub(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_rt_u64_new(extract_u64(lhs).wrapping_sub(extract_u64(rhs)) as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u64_mul(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_rt_u64_new(extract_u64(lhs).wrapping_mul(extract_u64(rhs)) as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u64_div(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    let r = extract_u64(rhs);
    fuse_rt_u64_new(if r == 0 { 0 } else { extract_u64(lhs).wrapping_div(r) } as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u64_mod(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    let r = extract_u64(rhs);
    fuse_rt_u64_new(if r == 0 { 0 } else { extract_u64(lhs).wrapping_rem(r) } as i64)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u64_eq(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_u64(lhs) == extract_u64(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u64_lt(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_u64(lhs) < extract_u64(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u64_le(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_u64(lhs) <= extract_u64(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u64_gt(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_u64(lhs) > extract_u64(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u64_ge(lhs: FuseHandle, rhs: FuseHandle) -> FuseHandle {
    fuse_bool(extract_u64(lhs) >= extract_u64(rhs))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_u64_to_string(handle: FuseHandle) -> FuseHandle {
    let s = extract_u64(handle).to_string();
    fuse_string_new_utf8(s.as_ptr(), s.len())
}

// --- IO FFI helpers ---

#[cfg(not(target_arch = "wasm32"))]
unsafe fn make_io_error(msg: &str, code: i64) -> FuseHandle {
    let type_name = b"IOError";
    let data = fuse_data_new(type_name.as_ptr(), type_name.len(), 2, None);
    fuse_data_set_field(data, 0, fuse_string_new_utf8(msg.as_ptr(), msg.len()));
    fuse_data_set_field(data, 1, fuse_int(code));
    data
}

#[cfg(not(target_arch = "wasm32"))]
unsafe fn io_error_code(e: &std::io::Error) -> i64 {
    match e.kind() {
        std::io::ErrorKind::NotFound => 1,
        std::io::ErrorKind::PermissionDenied => 2,
        std::io::ErrorKind::AlreadyExists => 3,
        std::io::ErrorKind::Interrupted => 7,
        _ => 0,
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_io_read_file(path: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    match std::fs::read_to_string(p) {
        Ok(s) => fuse_ok(fuse_string_new_utf8(s.as_ptr(), s.len())),
        Err(e) => { let msg = format!("io: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_io_write_file(path: FuseHandle, content: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    let c = extract_string(content);
    match std::fs::write(p, c) {
        Ok(()) => fuse_ok(fuse_unit()),
        Err(e) => { let msg = format!("io: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
unsafe extern "C" fn fuse_rt_file_destructor(_handle: FuseHandle) {
    // The file handle is cleaned up when the data class is dropped.
    // In a production runtime, this would close the file descriptor.
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_file_close(_file: FuseHandle) -> FuseHandle {
    fuse_ok(fuse_unit())
}

// --- Path FFI ---

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_path_separator() -> FuseHandle {
    let sep = if cfg!(windows) { "\\" } else { "/" };
    fuse_string_new_utf8(sep.as_ptr(), sep.len())
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_exists(path: FuseHandle) -> FuseHandle {
    fuse_bool(std::path::Path::new(extract_string(path)).exists())
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_is_file(path: FuseHandle) -> FuseHandle {
    fuse_bool(std::path::Path::new(extract_string(path)).is_file())
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_is_dir(path: FuseHandle) -> FuseHandle {
    fuse_bool(std::path::Path::new(extract_string(path)).is_dir())
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_stat(path: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    match std::fs::metadata(p) {
        Ok(meta) => fuse_ok(metadata_to_file_info(p, &meta)),
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_mkdir(path: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    match std::fs::create_dir(p) {
        Ok(()) => fuse_ok(fuse_unit()),
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_mkdir_all(path: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    match std::fs::create_dir_all(p) {
        Ok(()) => fuse_ok(fuse_unit()),
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_create_file(path: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    match std::fs::OpenOptions::new().write(true).create_new(true).open(p) {
        Ok(_) => fuse_ok(fuse_unit()),
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_copy_file(src: FuseHandle, dst: FuseHandle) -> FuseHandle {
    let s = extract_string(src);
    let d = extract_string(dst);
    match std::fs::copy(s, d) {
        Ok(_) => fuse_ok(fuse_unit()),
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_copy_dir(src: FuseHandle, dst: FuseHandle) -> FuseHandle {
    let s = extract_string(src);
    let d = extract_string(dst);
    match copy_dir_recursive(std::path::Path::new(s), std::path::Path::new(d)) {
        Ok(()) => fuse_ok(fuse_unit()),
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_rename(src: FuseHandle, dst: FuseHandle) -> FuseHandle {
    let s = extract_string(src);
    let d = extract_string(dst);
    match std::fs::rename(s, d) {
        Ok(()) => fuse_ok(fuse_unit()),
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_remove_file(path: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    match std::fs::remove_file(p) {
        Ok(()) => fuse_ok(fuse_unit()),
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_remove_dir(path: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    match std::fs::remove_dir(p) {
        Ok(()) => fuse_ok(fuse_unit()),
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_remove_dir_all(path: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    match std::fs::remove_dir_all(p) {
        Ok(()) => fuse_ok(fuse_unit()),
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_temp_dir() -> FuseHandle {
    let s = std::env::temp_dir().to_string_lossy().to_string();
    fuse_string_new_utf8(s.as_ptr(), s.len())
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_os_read_dir_recursive(path: FuseHandle) -> FuseHandle {
    let p = extract_string(path);
    let list = fuse_list_new();
    match read_dir_recursive_impl(std::path::Path::new(p), list) {
        Ok(()) => fuse_ok(list),
        Err(e) => { let msg = format!("os: {e}"); fuse_err(make_io_error(&msg, io_error_code(&e))) }
    }
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_env_get(name: FuseHandle) -> FuseHandle {
    let key = extract_string(name);
    match std::env::var(key) {
        Ok(val) => fuse_some(fuse_string_new_utf8(val.as_ptr(), val.len())),
        Err(_) => fuse_none(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_env_set(name: FuseHandle, value: FuseHandle) -> FuseHandle {
    let key = extract_string(name);
    let val = extract_string(value);
    // SAFETY: set_var is unsafe in Rust 2024 due to thread-safety concerns,
    // but Fuse's single-threaded evaluator makes this safe in practice.
    unsafe { std::env::set_var(key, val); }
    fuse_ok(fuse_unit())
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_env_remove(name: FuseHandle) -> FuseHandle {
    let key = extract_string(name);
    unsafe { std::env::remove_var(key); }
    fuse_ok(fuse_unit())
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_env_has(name: FuseHandle) -> FuseHandle {
    let key = extract_string(name);
    fuse_bool(std::env::var(key).is_ok())
}

// --- Process FFI ---

#[cfg(not(target_arch = "wasm32"))]
unsafe fn make_process_error(msg: &str, code: i64) -> FuseHandle {
    let type_name = b"ProcessError";
    let data = fuse_data_new(type_name.as_ptr(), type_name.len(), 2, None);
    fuse_data_set_field(data, 0, fuse_string_new_utf8(msg.as_ptr(), msg.len()));
    fuse_data_set_field(data, 1, fuse_int(code));
    data
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

// --- HTTP FFI ---

#[cfg(not(target_arch = "wasm32"))]
unsafe fn make_http_error(msg: &str, code: i64) -> FuseHandle {
    let type_name = b"HttpError";
    let data = fuse_data_new(type_name.as_ptr(), type_name.len(), 2, None);
    fuse_data_set_field(data, 0, fuse_string_new_utf8(msg.as_ptr(), msg.len()));
    fuse_data_set_field(data, 1, fuse_int(code));
    data
}

#[cfg(not(target_arch = "wasm32"))]
unsafe fn http_error_code(e: &ureq::Error) -> i64 {
    match e {
        ureq::Error::Transport(_) => 3,
        _ => 0,
    }
}

#[cfg(not(target_arch = "wasm32"))]
unsafe fn response_to_handle(status: u16, body: &str, headers: &[(String, String)]) -> FuseHandle {
    let type_name = b"Response";
    let data = fuse_data_new(type_name.as_ptr(), type_name.len(), 3, None);
    fuse_data_set_field(data, 0, fuse_int(status as i64));
    let header_map = fuse_map_new();
    for (k, v) in headers {
        fuse_map_set(header_map, fuse_string_new_utf8(k.as_ptr(), k.len()),
                     fuse_string_new_utf8(v.as_ptr(), v.len()));
    }
    fuse_data_set_field(data, 1, header_map);
    fuse_data_set_field(data, 2, fuse_string_new_utf8(body.as_ptr(), body.len()));
    data
}

#[cfg(not(target_arch = "wasm32"))]
unsafe fn do_http_request(method: &str, url: &str, body: Option<&str>, content_type: Option<&str>) -> FuseHandle {
    let request = match method {
        "GET" => ureq::get(url),
        "POST" => ureq::post(url),
        "PUT" => ureq::put(url),
        "DELETE" => ureq::delete(url),
        _ => ureq::get(url),
    };
    let result = if let Some(ct) = content_type {
        let req = request.set("Content-Type", ct);
        if let Some(b) = body { req.send_string(b) } else { req.call() }
    } else {
        if let Some(b) = body { request.send_string(b) } else { request.call() }
    };
    match result {
        Ok(response) => {
            let status = response.status();
            let mut headers = Vec::new();
            for name in response.headers_names() {
                if let Some(val) = response.header(&name) {
                    headers.push((name, val.to_string()));
                }
            }
            let body_str = response.into_string().unwrap_or_default();
            fuse_ok(response_to_handle(status, &body_str, &headers))
        }
        Err(ureq::Error::Status(code, response)) => {
            let mut headers = Vec::new();
            for name in response.headers_names() {
                if let Some(val) = response.header(&name) {
                    headers.push((name, val.to_string()));
                }
            }
            let body_str = response.into_string().unwrap_or_default();
            fuse_ok(response_to_handle(code, &body_str, &headers))
        }
        Err(e) => {
            let msg = format!("http: {e}");
            fuse_err(make_http_error(&msg, http_error_code(&e)))
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_http_get(url: FuseHandle) -> FuseHandle {
    do_http_request("GET", extract_string(url), None, None)
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_http_post(url: FuseHandle, body: FuseHandle) -> FuseHandle {
    do_http_request("POST", extract_string(url), Some(extract_string(body)), None)
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_http_post_json(url: FuseHandle, body: FuseHandle) -> FuseHandle {
    do_http_request("POST", extract_string(url), Some(extract_string(body)), Some("application/json"))
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_http_put(url: FuseHandle, body: FuseHandle) -> FuseHandle {
    do_http_request("PUT", extract_string(url), Some(extract_string(body)), None)
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_http_delete(url: FuseHandle) -> FuseHandle {
    do_http_request("DELETE", extract_string(url), None, None)
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_http_request(method: FuseHandle, url: FuseHandle, body: FuseHandle, headers_keys: FuseHandle, headers_vals: FuseHandle) -> FuseHandle {
    let m = extract_string(method);
    let u = extract_string(url);
    let b = extract_string(body);
    let mut request = match m {
        "GET" => ureq::get(u),
        "POST" => ureq::post(u),
        "PUT" => ureq::put(u),
        "DELETE" => ureq::delete(u),
        "PATCH" => ureq::patch(u),
        "HEAD" => ureq::head(u),
        _ => ureq::get(u),
    };
    // Apply headers
    if let (ValueKind::List(keys), ValueKind::List(vals)) = (&(*headers_keys).kind, &(*headers_vals).kind) {
        for (k, v) in keys.iter().zip(vals.iter()) {
            if let (ValueKind::String(ks), ValueKind::String(vs)) = (&(**k).kind, &(**v).kind) {
                request = request.set(ks.as_str(), vs.as_str());
            }
        }
    }
    let result = if b.is_empty() { request.call() } else { request.send_string(b) };
    match result {
        Ok(response) => {
            let status = response.status();
            let mut hdrs = Vec::new();
            for name in response.headers_names() {
                if let Some(val) = response.header(&name) {
                    hdrs.push((name, val.to_string()));
                }
            }
            let body_str = response.into_string().unwrap_or_default();
            fuse_ok(response_to_handle(status, &body_str, &hdrs))
        }
        Err(ureq::Error::Status(code, response)) => {
            let mut hdrs = Vec::new();
            for name in response.headers_names() {
                if let Some(val) = response.header(&name) {
                    hdrs.push((name, val.to_string()));
                }
            }
            let body_str = response.into_string().unwrap_or_default();
            fuse_ok(response_to_handle(code, &body_str, &hdrs))
        }
        Err(e) => {
            let msg = format!("http: {e}");
            fuse_err(make_http_error(&msg, http_error_code(&e)))
        }
    }
}

// --- JSON FFI ---

// JsonValue is represented as a data class with tag + value fields:
//   field 0: tag (Int) — 0=null, 1=bool, 2=number, 3=string, 4=array, 5=object
//   field 1: value (dynamic — Bool, Float, String, List, or Map depending on tag)
unsafe fn json_value_new(tag: i64, value: FuseHandle) -> FuseHandle {
    let type_name = b"JsonValue";
    let data = fuse_data_new(type_name.as_ptr(), type_name.len(), 2, None);
    fuse_data_set_field(data, 0, fuse_int(tag));
    fuse_data_set_field(data, 1, value);
    data
}

unsafe fn json_parse_value(s: &str, pos: &mut usize) -> Result<FuseHandle, String> {
    skip_whitespace(s, pos);
    if *pos >= s.len() { return Err("unexpected end of input".to_string()); }
    let ch = s.as_bytes()[*pos];
    match ch {
        b'"' => {
            let st = json_parse_string(s, pos)?;
            Ok(json_value_new(3, fuse_string_new_utf8(st.as_ptr(), st.len())))
        }
        b't' | b'f' => {
            if s[*pos..].starts_with("true") {
                *pos += 4; Ok(json_value_new(1, fuse_bool(true)))
            } else if s[*pos..].starts_with("false") {
                *pos += 5; Ok(json_value_new(1, fuse_bool(false)))
            } else { Err(format!("unexpected token at {}", *pos)) }
        }
        b'n' => {
            if s[*pos..].starts_with("null") {
                *pos += 4; Ok(json_value_new(0, fuse_unit()))
            } else { Err(format!("unexpected token at {}", *pos)) }
        }
        b'-' | b'0'..=b'9' => {
            let start = *pos;
            if s.as_bytes()[*pos] == b'-' { *pos += 1; }
            while *pos < s.len() && s.as_bytes()[*pos].is_ascii_digit() { *pos += 1; }
            if *pos < s.len() && s.as_bytes()[*pos] == b'.' {
                *pos += 1;
                while *pos < s.len() && s.as_bytes()[*pos].is_ascii_digit() { *pos += 1; }
            }
            if *pos < s.len() && (s.as_bytes()[*pos] == b'e' || s.as_bytes()[*pos] == b'E') {
                *pos += 1;
                if *pos < s.len() && (s.as_bytes()[*pos] == b'+' || s.as_bytes()[*pos] == b'-') { *pos += 1; }
                while *pos < s.len() && s.as_bytes()[*pos].is_ascii_digit() { *pos += 1; }
            }
            let num_str = &s[start..*pos];
            let num: f64 = num_str.parse().map_err(|e| format!("invalid number: {e}"))?;
            Ok(json_value_new(2, fuse_float(num)))
        }
        b'[' => {
            *pos += 1;
            let list = fuse_list_new();
            skip_whitespace(s, pos);
            if *pos < s.len() && s.as_bytes()[*pos] == b']' {
                *pos += 1; return Ok(json_value_new(4, list));
            }
            loop {
                let item = json_parse_value(s, pos)?;
                fuse_list_push(list, item);
                skip_whitespace(s, pos);
                if *pos >= s.len() { return Err("unterminated array".to_string()); }
                if s.as_bytes()[*pos] == b']' { *pos += 1; break; }
                if s.as_bytes()[*pos] != b',' { return Err(format!("expected ',' or ']' at {}", *pos)); }
                *pos += 1;
            }
            Ok(json_value_new(4, list))
        }
        b'{' => {
            *pos += 1;
            let map = fuse_map_new();
            skip_whitespace(s, pos);
            if *pos < s.len() && s.as_bytes()[*pos] == b'}' {
                *pos += 1; return Ok(json_value_new(5, map));
            }
            loop {
                skip_whitespace(s, pos);
                if *pos >= s.len() || s.as_bytes()[*pos] != b'"' {
                    return Err(format!("expected string key at {}", *pos));
                }
                let key = json_parse_string(s, pos)?;
                skip_whitespace(s, pos);
                if *pos >= s.len() || s.as_bytes()[*pos] != b':' {
                    return Err(format!("expected ':' at {}", *pos));
                }
                *pos += 1;
                let val = json_parse_value(s, pos)?;
                let k = fuse_string_new_utf8(key.as_ptr(), key.len());
                fuse_map_set(map, k, val);
                skip_whitespace(s, pos);
                if *pos >= s.len() { return Err("unterminated object".to_string()); }
                if s.as_bytes()[*pos] == b'}' { *pos += 1; break; }
                if s.as_bytes()[*pos] != b',' { return Err(format!("expected ',' or '}}' at {}", *pos)); }
                *pos += 1;
            }
            Ok(json_value_new(5, map))
        }
        _ => Err(format!("unexpected character '{}' at {}", ch as char, *pos))
    }
}

fn skip_whitespace(s: &str, pos: &mut usize) {
    while *pos < s.len() && matches!(s.as_bytes()[*pos], b' ' | b'\t' | b'\n' | b'\r') {
        *pos += 1;
    }
}

fn json_parse_string(s: &str, pos: &mut usize) -> Result<String, String> {
    if *pos >= s.len() || s.as_bytes()[*pos] != b'"' {
        return Err(format!("expected '\"' at {}", *pos));
    }
    *pos += 1;
    let mut result = String::new();
    while *pos < s.len() {
        let ch = s.as_bytes()[*pos];
        if ch == b'"' { *pos += 1; return Ok(result); }
        if ch == b'\\' {
            *pos += 1;
            if *pos >= s.len() { return Err("unterminated string escape".to_string()); }
            match s.as_bytes()[*pos] {
                b'"' => result.push('"'),
                b'\\' => result.push('\\'),
                b'/' => result.push('/'),
                b'n' => result.push('\n'),
                b'r' => result.push('\r'),
                b't' => result.push('\t'),
                b'b' => result.push('\u{0008}'),
                b'f' => result.push('\u{000C}'),
                b'u' => {
                    *pos += 1;
                    if *pos + 4 > s.len() { return Err("incomplete unicode escape".to_string()); }
                    let hex = &s[*pos..*pos+4];
                    let cp = u32::from_str_radix(hex, 16).map_err(|_| "invalid unicode escape".to_string())?;
                    if let Some(c) = char::from_u32(cp) { result.push(c); }
                    *pos += 3; // will be incremented by 1 at end of loop
                }
                c => { result.push(c as char); }
            }
        } else {
            result.push(ch as char);
        }
        *pos += 1;
    }
    Err("unterminated string".to_string())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_json_parse(input: FuseHandle) -> FuseHandle {
    let s = extract_string(input);
    let mut pos = 0usize;
    match json_parse_value(s, &mut pos) {
        Ok(value) => fuse_ok(value),
        Err(msg) => {
            let type_name = b"JsonError";
            let err = fuse_data_new(type_name.as_ptr(), type_name.len(), 3, None);
            fuse_data_set_field(err, 0, fuse_string_new_utf8(msg.as_ptr(), msg.len()));
            fuse_data_set_field(err, 1, fuse_int(1));
            fuse_data_set_field(err, 2, fuse_int(pos as i64));
            fuse_err(err)
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_json_stringify(value: FuseHandle) -> FuseHandle {
    let s = json_stringify_impl(value, false, 0, 0);
    fuse_string_new_utf8(s.as_ptr(), s.len())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_json_stringify_pretty(value: FuseHandle, indent: FuseHandle) -> FuseHandle {
    let ind = match &(*indent).kind { ValueKind::Int(n) => *n as usize, _ => 2 };
    let s = json_stringify_impl(value, true, ind, 0);
    fuse_string_new_utf8(s.as_ptr(), s.len())
}

unsafe fn json_stringify_impl(handle: FuseHandle, pretty: bool, indent: usize, depth: usize) -> String {
    if let ValueKind::Data(dv) = &(*handle).kind {
        if dv.fields.len() >= 2 {
            let tag = match &(*dv.fields[0]).kind { ValueKind::Int(n) => *n, _ => -1 };
            let val = dv.fields[1];
            return match tag {
                0 => "null".to_string(),
                1 => match &(*val).kind { ValueKind::Bool(b) => b.to_string(), _ => "false".to_string() },
                2 => match &(*val).kind {
                    ValueKind::Float(f) => {
                        if *f == f.floor() && f.is_finite() { format!("{f:.1}") } else { f.to_string() }
                    }
                    ValueKind::Int(n) => format!("{n}.0"),
                    _ => "0".to_string()
                },
                3 => {
                    let s = match &(*val).kind { ValueKind::String(s) => s.as_str(), _ => "" };
                    format!("\"{}\"", json_escape(s))
                }
                4 => {
                    if let ValueKind::List(items) = &(*val).kind {
                        if items.is_empty() { return "[]".to_string(); }
                        let mut parts = Vec::new();
                        for item in items {
                            parts.push(json_stringify_impl(*item, pretty, indent, depth + 1));
                        }
                        if pretty {
                            let pad = " ".repeat(indent * (depth + 1));
                            let pad_close = " ".repeat(indent * depth);
                            format!("[\n{pad}{}\n{pad_close}]", parts.join(&format!(",\n{pad}")))
                        } else {
                            format!("[{}]", parts.join(","))
                        }
                    } else { "[]".to_string() }
                }
                5 => {
                    if let ValueKind::Map(map) = &(*val).kind {
                        if map.entries.is_empty() { return "{}".to_string(); }
                        let mut parts = Vec::new();
                        for (k, v) in &map.entries {
                            let ks = match &(**k).kind { ValueKind::String(s) => s.clone(), _ => String::new() };
                            let vs = json_stringify_impl(*v, pretty, indent, depth + 1);
                            if pretty {
                                parts.push(format!("\"{}\": {}", json_escape(&ks), vs));
                            } else {
                                parts.push(format!("\"{}\":{}", json_escape(&ks), vs));
                            }
                        }
                        if pretty {
                            let pad = " ".repeat(indent * (depth + 1));
                            let pad_close = " ".repeat(indent * depth);
                            format!("{{\n{pad}{}\n{pad_close}}}", parts.join(&format!(",\n{pad}")))
                        } else {
                            format!("{{{}}}", parts.join(","))
                        }
                    } else { "{}".to_string() }
                }
                _ => "null".to_string(),
            };
        }
    }
    "null".to_string()
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

// --- Net FFI ---

#[cfg(not(target_arch = "wasm32"))]
unsafe fn make_net_error(msg: &str, code: i64) -> FuseHandle {
    let type_name = b"NetError";
    let data = fuse_data_new(type_name.as_ptr(), type_name.len(), 2, None);
    fuse_data_set_field(data, 0, fuse_string_new_utf8(msg.as_ptr(), msg.len()));
    fuse_data_set_field(data, 1, fuse_int(code));
    data
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
unsafe fn wrap_tcp_stream(stream: std::net::TcpStream) -> FuseHandle {
    let boxed: Box<dyn std::any::Any> = Box::new(stream);
    let ptr = Box::into_raw(boxed);
    let type_name = b"TcpStream";
    let handle = fuse_data_new(type_name.as_ptr(), type_name.len(), 1, None);
    fuse_data_set_field(handle, 0, ptr as FuseHandle);
    handle
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_net_tcp_connect(addr: FuseHandle, port: FuseHandle) -> FuseHandle {
    let a = extract_string(addr);
    let p = match &(*port).kind { ValueKind::Int(n) => *n as u16, _ => 0 };
    match std::net::TcpStream::connect((a, p)) {
        Ok(stream) => fuse_ok(wrap_tcp_stream(stream)),
        Err(e) => { let msg = format!("net: {e}"); fuse_err(make_net_error(&msg, net_error_code(&e))) }
    }
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_net_tcp_close(_stream: FuseHandle) -> FuseHandle {
    // Drop happens when the data class is ASAP-destroyed.
    fuse_ok(fuse_unit())
}

// --- TcpListener ---

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_net_tcp_listener_close(_listener: FuseHandle) -> FuseHandle {
    fuse_ok(fuse_unit())
}

// --- UdpSocket ---

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_time_instant_now() -> FuseHandle {
    let nanos = std::time::Instant::now().elapsed().as_nanos() as i64;
    // Use a thread-local base instant for monotonic measurement
#[cfg(not(target_arch = "wasm32"))]
    thread_local! {
        static BASE: std::time::Instant = std::time::Instant::now();
    }
    let n = BASE.with(|base| base.elapsed().as_nanos() as i64);
    fuse_int(n)
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_time_system_now() -> FuseHandle {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    fuse_int(secs)
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_time_elapsed_nanos(start_nanos: FuseHandle) -> FuseHandle {
#[cfg(not(target_arch = "wasm32"))]
    thread_local! {
        static BASE: std::time::Instant = std::time::Instant::now();
    }
    let now = BASE.with(|base| base.elapsed().as_nanos() as i64);
    let start = match &(*start_nanos).kind { ValueKind::Int(n) => *n, _ => 0 };
    fuse_int(now - start)
}

// --- Timer FFI ---

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_timer_sleep_ms(ms_handle: FuseHandle) -> FuseHandle {
    let ms = extract_int(ms_handle);
    if ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(ms as u64));
    }
    fuse_unit()
}

// --- Sys FFI ---

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_sys_args() -> FuseHandle {
    let list = fuse_list_new();
    for arg in std::env::args() {
        fuse_list_push(list, fuse_string_new_utf8(arg.as_ptr(), arg.len()));
    }
    list
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_sys_exit(code: FuseHandle) -> FuseHandle {
    let c = match &(*code).kind { ValueKind::Int(n) => *n as i32, _ => 1 };
    std::process::exit(c);
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_sys_pid() -> FuseHandle {
    fuse_int(std::process::id() as i64)
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_sys_platform() -> FuseHandle {
    let p = if cfg!(target_os = "windows") { "windows" }
        else if cfg!(target_os = "macos") { "macos" }
        else if cfg!(target_os = "linux") { "linux" }
        else { "unknown" };
    fuse_string_new_utf8(p.as_ptr(), p.len())
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_sys_arch() -> FuseHandle {
    let a = if cfg!(target_arch = "x86_64") { "x86_64" }
        else if cfg!(target_arch = "aarch64") { "aarch64" }
        else if cfg!(target_arch = "x86") { "x86" }
        else { "unknown" };
    fuse_string_new_utf8(a.as_ptr(), a.len())
}

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_sys_num_cpus() -> FuseHandle {
    fuse_int(std::thread::available_parallelism().map(|n| n.get() as i64).unwrap_or(1))
}

#[cfg(not(target_arch = "wasm32"))]
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

pub fn extract_int(handle: FuseHandle) -> i64 {
    unsafe { match &(*handle).kind { ValueKind::Int(n) => *n, _ => 0 } }
}

/// Extract a `Vec<i64>` from a Fuse `List<Int>` handle. Returns an
/// empty vector if the handle is null, not a list, or contains
/// non-Int elements. Used by the Cranelift FFI wrappers (`ins_call`,
/// `ins_jump`, etc.) to marshal Value-id arrays from Fuse callers:
/// stage 2 compiles list-literal arguments like `[v1, v2]` to Fuse
/// `List<Int>` handles where each element is a boxed Int holding a
/// Cranelift Value id, so the FFI layer needs to walk the list and
/// extract the ids rather than dereferencing a raw pointer. The
/// cranelift-ffi smoke test's raw-pointer convention predates stage
/// 2 and is no longer supported for array arguments — call sites
/// that want to pass a primitive array must first wrap it in a
/// `fuse_list` / `fuse_list_push` chain.
pub fn extract_int_list(handle: FuseHandle) -> Vec<i64> {
    if handle.is_null() {
        return Vec::new();
    }
    unsafe {
        match &(*handle).kind {
            ValueKind::List(items) => items
                .iter()
                .map(|h| {
                    if h.is_null() {
                        0
                    } else {
                        match &(**h).kind {
                            ValueKind::Int(n) => *n,
                            _ => 0,
                        }
                    }
                })
                .collect(),
            _ => Vec::new(),
        }
    }
}

fn extract_string(handle: FuseHandle) -> &'static str {
    unsafe { match &(*handle).kind { ValueKind::String(s) => s.as_str(), _ => "" } }
}

/// Extract a `&str` from a Fuse `String` handle. Returns an empty
/// string for null handles or non-string values. Public so
/// cranelift-ffi's `str_from_raw` helper can route Fuse `String`
/// handles through here instead of treating its pointer argument as
/// a raw `*const u8` — stage 2 call sites always pass `String`
/// handles because the self-hosted compiler has no way to produce a
/// raw byte pointer.
pub fn extract_string_pub(handle: FuseHandle) -> &'static str {
    if handle.is_null() {
        return "";
    }
    unsafe {
        match &(*handle).kind {
            ValueKind::String(s) => s.as_str(),
            _ => "",
        }
    }
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

// ---------------------------------------------------------------------------
// Test assertion runtime support
// ---------------------------------------------------------------------------

// --- Panic infrastructure (setjmp/longjmp) ---

/// Platform jmp_buf: MSVC x64 uses [i64; 16].
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
type JmpBuf = [i64; 16];
/// Fallback for other platforms.
#[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
type JmpBuf = [i64; 32];

#[cfg(not(target_arch = "wasm32"))]
unsafe extern "C" {
    #[cfg(target_os = "windows")]
    #[link_name = "_setjmp"]
    fn c_setjmp(env: *mut JmpBuf, frame: *const u8) -> i32;
    #[cfg(not(target_os = "windows"))]
    #[link_name = "setjmp"]
    fn c_setjmp(env: *mut JmpBuf) -> i32;
    fn longjmp(env: *mut JmpBuf, val: i32) -> !;
}

#[cfg(target_os = "windows")]
#[cfg(not(target_arch = "wasm32"))]
unsafe fn platform_setjmp(buf: *mut JmpBuf) -> i32 {
    unsafe { c_setjmp(buf, ptr::null()) }
}
#[cfg(not(target_os = "windows"))]
#[cfg(not(target_arch = "wasm32"))]
unsafe fn platform_setjmp(buf: *mut JmpBuf) -> i32 {
    unsafe { c_setjmp(buf) }
}

#[cfg(not(target_arch = "wasm32"))]
thread_local! {
    static PANIC_JMP: Cell<*mut JmpBuf> = const { Cell::new(ptr::null_mut()) };
}

/// Trigger a Fuse panic.  When called inside an `assertPanics` context this
/// performs a longjmp back to the recovery point.  Otherwise exits with 101.
#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_panic() {
    PANIC_JMP.with(|cell| {
        let buf = cell.get();
        if !buf.is_null() {
            unsafe { longjmp(buf, 1) };
        }
    });
    std::process::exit(101);
}

/// assertEq — compare two opaque handles for equality (via string repr).
/// On mismatch prints a diagnostic and exits with code 1.
#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_test_assert_eq(
    a: FuseHandle,
    b: FuseHandle,
    msg: FuseHandle,
) -> FuseHandle {
    let a_str = unsafe { clone_to_string(a) };
    let b_str = unsafe { clone_to_string(b) };
    if a_str != b_str {
        let m = unsafe { clone_to_string(msg) };
        eprintln!("[FAIL] assertEq: {m}");
        eprintln!("  expected: {b_str}");
        eprintln!("  actual:   {a_str}");
        std::process::exit(1);
    }
    FuseValue::new(ValueKind::Unit)
}

/// assertNe — compare two opaque handles for inequality (via string repr).
/// On match prints a diagnostic and exits with code 1.
#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_test_assert_ne(
    a: FuseHandle,
    b: FuseHandle,
    msg: FuseHandle,
) -> FuseHandle {
    let a_str = unsafe { clone_to_string(a) };
    let b_str = unsafe { clone_to_string(b) };
    if a_str == b_str {
        let m = unsafe { clone_to_string(msg) };
        eprintln!("[FAIL] assertNe: {m}");
        eprintln!("  both values: {a_str}");
        std::process::exit(1);
    }
    FuseValue::new(ValueKind::Unit)
}

/// assertApprox — compare two Floats within an epsilon tolerance.
/// On failure prints a diagnostic and exits with code 1.
#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_test_assert_approx(
    a: FuseHandle,
    b: FuseHandle,
    epsilon: FuseHandle,
    msg: FuseHandle,
) -> FuseHandle {
    let av = match &unsafe { value_ref(a) }.kind {
        ValueKind::Float(f) => *f,
        ValueKind::Int(n) => *n as f64,
        _ => 0.0,
    };
    let bv = match &unsafe { value_ref(b) }.kind {
        ValueKind::Float(f) => *f,
        ValueKind::Int(n) => *n as f64,
        _ => 0.0,
    };
    let ev = match &unsafe { value_ref(epsilon) }.kind {
        ValueKind::Float(f) => *f,
        ValueKind::Int(n) => *n as f64,
        _ => 0.0,
    };
    if (av - bv).abs() > ev {
        let m = unsafe { clone_to_string(msg) };
        eprintln!("[FAIL] assertApprox: {m}");
        eprintln!("  expected: {bv} ± {ev}");
        eprintln!("  actual:   {av}");
        std::process::exit(1);
    }
    FuseValue::new(ValueKind::Unit)
}

/// assertPanics — call a Fuse closure and verify it panics.
/// The closure is a Fuse List whose element 0 is a raw function pointer
/// and the remaining elements are captured variables.
/// Uses setjmp/longjmp: fuse_rt_panic() performs longjmp back here when
/// the closure panics.
#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_test_assert_panics(closure: FuseHandle) -> FuseHandle {
    let fn_ptr_raw: FuseHandle = {
        let list_val = unsafe { value_ref(closure) };
        match &list_val.kind {
            ValueKind::List(items) if !items.is_empty() => items[0],
            _ => {
                eprintln!("[FAIL] assertPanics: argument is not a valid closure");
                std::process::exit(1);
            }
        }
    };

    // The function pointer has the Fuse closure ABI:
    //   extern "C" fn(env: FuseHandle) -> FuseHandle
    let fn_ptr: unsafe extern "C" fn(FuseHandle) -> FuseHandle =
        unsafe { std::mem::transmute(fn_ptr_raw) };

    // Save previous recovery point and install ours.
    let prev = PANIC_JMP.with(|cell| cell.get());
    let mut jmp_buf: JmpBuf = unsafe { std::mem::zeroed() };

    let caught = unsafe { platform_setjmp(&mut jmp_buf as *mut _) };
    if caught == 0 {
        // First call — set recovery point and call the closure.
        PANIC_JMP.with(|cell| cell.set(&mut jmp_buf as *mut _));
        unsafe { fn_ptr(closure) };
        // Function returned normally — assertion fails.
        PANIC_JMP.with(|cell| cell.set(prev));
        eprintln!("[FAIL] assertPanics: expected panic but function returned normally");
        std::process::exit(1);
    } else {
        // longjmp fired — panic was caught, assertion passes.
        PANIC_JMP.with(|cell| cell.set(prev));
        FuseValue::new(ValueKind::Unit)
    }
}

// ---------------------------------------------------------------------------
// Logging runtime support
// ---------------------------------------------------------------------------

thread_local! {
    static LOG_GLOBAL_LEVEL: Cell<i64> = const { Cell::new(2) }; // default: Info
}

/// Print a string to stderr (no newline — caller includes it).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_log_eprintln(msg: FuseHandle) -> FuseHandle {
    let s = unsafe { clone_to_string(msg) };
    eprintln!("{s}");
    FuseValue::new(ValueKind::Unit)
}

/// Return the current UTC timestamp as an ISO 8601 string (compact form).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_log_timestamp() -> FuseHandle {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Break epoch seconds into date/time components (UTC).
    let days = (secs / 86400) as i64;
    let day_secs = (secs % 86400) as i64;
    let hour = day_secs / 3600;
    let minute = (day_secs % 3600) / 60;
    let second = day_secs % 60;

    // Civil date from day count (algorithm from Howard Hinnant).
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    let ts = format!("{y:04}-{m:02}-{d:02}T{hour:02}:{minute:02}:{second:02}Z");
    FuseValue::new(ValueKind::String(ts))
}

/// Get the global log level (0=Trace .. 4=Error).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_log_global_level() -> FuseHandle {
    let level = LOG_GLOBAL_LEVEL.with(|cell| cell.get());
    unsafe { fuse_int(level) }
}

/// Set the global log level.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_log_set_global_level(level: FuseHandle) -> FuseHandle {
    let n = unsafe { extract_int(level) };
    LOG_GLOBAL_LEVEL.with(|cell| cell.set(n));
    FuseValue::new(ValueKind::Unit)
}

/// Append a line to a file (for Logger.toFile output).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_log_append_file(
    path: FuseHandle,
    msg: FuseHandle,
) -> FuseHandle {
    let p = unsafe { clone_to_string(path) };
    let m = unsafe { clone_to_string(msg) };
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        let _ = writeln!(f, "{m}");
    }
    FuseValue::new(ValueKind::Unit)
}

// ---------------------------------------------------------------------------
// Regex runtime support
// ---------------------------------------------------------------------------

thread_local! {
    static REGEX_STORE: RefCell<HashMap<i64, regex::Regex>> = RefCell::new(HashMap::new());
    static REGEX_NEXT_ID: Cell<i64> = const { Cell::new(1) };
}

/// Helper: build a Match data class (text: String, start: Int, end: Int).
unsafe fn make_match(text: &str, start: usize, end: usize) -> FuseHandle {
    let tn = b"Match";
    let data = fuse_data_new(tn.as_ptr(), tn.len(), 3, None);
    fuse_data_set_field(data, 0, fuse_string_new_utf8(text.as_ptr(), text.len()));
    fuse_data_set_field(data, 1, fuse_int(start as i64));
    fuse_data_set_field(data, 2, fuse_int(end as i64));
    data
}

/// Compile a regex pattern. Returns Result<Int, String> where Int is
/// the handle for subsequent operations.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_regex_compile(pattern: FuseHandle) -> FuseHandle {
    let pat = unsafe { clone_to_string(pattern) };
    match regex::Regex::new(&pat) {
        Ok(re) => {
            let id = REGEX_NEXT_ID.with(|c| { let id = c.get(); c.set(id + 1); id });
            REGEX_STORE.with(|store| store.borrow_mut().insert(id, re));
            unsafe { fuse_ok(fuse_int(id)) }
        }
        Err(e) => {
            let msg = format!("{e}");
            unsafe { fuse_err(fuse_string_new_utf8(msg.as_ptr(), msg.len())) }
        }
    }
}

/// Test whether the regex matches anywhere in the text.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_regex_is_match(
    handle: FuseHandle,
    text: FuseHandle,
) -> FuseHandle {
    let id = extract_int(handle);
    let txt = unsafe { clone_to_string(text) };
    let matched = REGEX_STORE.with(|store| {
        store.borrow().get(&id).map(|re| re.is_match(&txt)).unwrap_or(false)
    });
    unsafe { fuse_bool(matched) }
}

/// Find the first match. Returns Option<Match>.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_regex_find(
    handle: FuseHandle,
    text: FuseHandle,
) -> FuseHandle {
    let id = extract_int(handle);
    let txt = unsafe { clone_to_string(text) };
    let result = REGEX_STORE.with(|store| {
        store.borrow().get(&id).and_then(|re| {
            re.find(&txt).map(|m| (m.as_str().to_string(), m.start(), m.end()))
        })
    });
    match result {
        Some((matched, start, end)) => unsafe { fuse_some(make_match(&matched, start, end)) },
        None => unsafe { fuse_none() },
    }
}

/// Find all matches. Returns List<Match>.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_regex_find_all(
    handle: FuseHandle,
    text: FuseHandle,
) -> FuseHandle {
    let id = extract_int(handle);
    let txt = unsafe { clone_to_string(text) };
    let list = unsafe { fuse_list_new() };
    REGEX_STORE.with(|store| {
        if let Some(re) = store.borrow().get(&id) {
            for m in re.find_iter(&txt) {
                let item = unsafe { make_match(m.as_str(), m.start(), m.end()) };
                unsafe { fuse_list_push(list, item) };
            }
        }
    });
    list
}

/// Replace the first match. Returns String.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_regex_replace(
    handle: FuseHandle,
    text: FuseHandle,
    replacement: FuseHandle,
) -> FuseHandle {
    let id = extract_int(handle);
    let txt = unsafe { clone_to_string(text) };
    let rep = unsafe { clone_to_string(replacement) };
    let result = REGEX_STORE.with(|store| {
        store.borrow().get(&id).map(|re| re.replace(&txt, rep.as_str()).to_string())
            .unwrap_or(txt.clone())
    });
    unsafe { fuse_string_new_utf8(result.as_ptr(), result.len()) }
}

/// Replace all matches. Returns String.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_regex_replace_all(
    handle: FuseHandle,
    text: FuseHandle,
    replacement: FuseHandle,
) -> FuseHandle {
    let id = extract_int(handle);
    let txt = unsafe { clone_to_string(text) };
    let rep = unsafe { clone_to_string(replacement) };
    let result = REGEX_STORE.with(|store| {
        store.borrow().get(&id).map(|re| re.replace_all(&txt, rep.as_str()).to_string())
            .unwrap_or(txt.clone())
    });
    unsafe { fuse_string_new_utf8(result.as_ptr(), result.len()) }
}

/// Split text by regex. Returns List<String>.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_regex_split(
    handle: FuseHandle,
    text: FuseHandle,
) -> FuseHandle {
    let id = extract_int(handle);
    let txt = unsafe { clone_to_string(text) };
    let list = unsafe { fuse_list_new() };
    REGEX_STORE.with(|store| {
        if let Some(re) = store.borrow().get(&id) {
            for part in re.split(&txt) {
                let s = unsafe { fuse_string_new_utf8(part.as_ptr(), part.len()) };
                unsafe { fuse_list_push(list, s) };
            }
        }
    });
    list
}

/// Capture groups from the first match. Returns Option<List<String>>.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_regex_captures(
    handle: FuseHandle,
    text: FuseHandle,
) -> FuseHandle {
    let id = extract_int(handle);
    let txt = unsafe { clone_to_string(text) };
    let result = REGEX_STORE.with(|store| {
        store.borrow().get(&id).and_then(|re| {
            re.captures(&txt).map(|caps| {
                let list = unsafe { fuse_list_new() };
                for i in 0..caps.len() {
                    let s = caps.get(i).map(|m| m.as_str()).unwrap_or("");
                    unsafe { fuse_list_push(list, fuse_string_new_utf8(s.as_ptr(), s.len())) };
                }
                list
            })
        })
    });
    match result {
        Some(list) => unsafe { fuse_some(list) },
        None => unsafe { fuse_none() },
    }
}

// ---------------------------------------------------------------------------
// TOML runtime support
// ---------------------------------------------------------------------------

/// Convert a toml::Value into a FuseHandle representing a TomlValue enum.
unsafe fn toml_value_to_fuse(val: &toml::Value) -> FuseHandle {
    let tn = b"TomlValue";
    match val {
        toml::Value::Boolean(b) => {
            let vn = b"Bool";
            fuse_enum_new(tn.as_ptr(), tn.len(), 0, vn.as_ptr(), vn.len(), fuse_bool(*b))
        }
        toml::Value::Integer(n) => {
            let vn = b"Int";
            fuse_enum_new(tn.as_ptr(), tn.len(), 1, vn.as_ptr(), vn.len(), fuse_int(*n))
        }
        toml::Value::Float(f) => {
            let vn = b"Float";
            fuse_enum_new(tn.as_ptr(), tn.len(), 2, vn.as_ptr(), vn.len(), fuse_float(*f))
        }
        toml::Value::String(s) => {
            let vn = b"Str";
            fuse_enum_new(tn.as_ptr(), tn.len(), 3, vn.as_ptr(), vn.len(),
                fuse_string_new_utf8(s.as_ptr(), s.len()))
        }
        toml::Value::Datetime(dt) => {
            let vn = b"DateTime";
            let s = dt.to_string();
            fuse_enum_new(tn.as_ptr(), tn.len(), 4, vn.as_ptr(), vn.len(),
                fuse_string_new_utf8(s.as_ptr(), s.len()))
        }
        toml::Value::Array(arr) => {
            let vn = b"Array";
            let list = fuse_list_new();
            for item in arr {
                fuse_list_push(list, toml_value_to_fuse(item));
            }
            fuse_enum_new(tn.as_ptr(), tn.len(), 5, vn.as_ptr(), vn.len(), list)
        }
        toml::Value::Table(tbl) => {
            let vn = b"Table";
            let map = fuse_map_new();
            for (k, v) in tbl {
                let key = fuse_string_new_utf8(k.as_ptr(), k.len());
                fuse_map_set(map, key, toml_value_to_fuse(v));
            }
            fuse_enum_new(tn.as_ptr(), tn.len(), 6, vn.as_ptr(), vn.len(), map)
        }
    }
}

/// Convert a FuseHandle (TomlValue enum) back to a toml::Value.
unsafe fn fuse_to_toml_value(handle: FuseHandle) -> toml::Value {
    let val = value_ref(handle);
    match &val.kind {
        ValueKind::Enum(e) => match e.variant_tag {
            0 => { // Bool
                let payload = e.payloads.first().copied().unwrap_or(ptr::null_mut());
                match &value_ref(payload).kind {
                    ValueKind::Bool(b) => toml::Value::Boolean(*b),
                    _ => toml::Value::Boolean(false),
                }
            }
            1 => { // Int
                let payload = e.payloads.first().copied().unwrap_or(ptr::null_mut());
                toml::Value::Integer(extract_int(payload))
            }
            2 => { // Float
                let payload = e.payloads.first().copied().unwrap_or(ptr::null_mut());
                match &value_ref(payload).kind {
                    ValueKind::Float(f) => toml::Value::Float(*f),
                    ValueKind::Int(n) => toml::Value::Float(*n as f64),
                    _ => toml::Value::Float(0.0),
                }
            }
            3 => { // Str
                let payload = e.payloads.first().copied().unwrap_or(ptr::null_mut());
                toml::Value::String(clone_to_string(payload))
            }
            4 => { // DateTime
                let payload = e.payloads.first().copied().unwrap_or(ptr::null_mut());
                let s = clone_to_string(payload);
                s.parse::<toml::value::Datetime>()
                    .map(toml::Value::Datetime)
                    .unwrap_or_else(|_| toml::Value::String(s))
            }
            5 => { // Array
                let payload = e.payloads.first().copied().unwrap_or(ptr::null_mut());
                let mut arr = Vec::new();
                if let ValueKind::List(items) = &value_ref(payload).kind {
                    for item in items {
                        arr.push(fuse_to_toml_value(*item));
                    }
                }
                toml::Value::Array(arr)
            }
            6 => { // Table
                let payload = e.payloads.first().copied().unwrap_or(ptr::null_mut());
                let mut tbl = toml::map::Map::new();
                if let ValueKind::Map(map) = &value_ref(payload).kind {
                    for (k, v) in &map.entries {
                        tbl.insert(clone_to_string(*k), fuse_to_toml_value(*v));
                    }
                }
                toml::Value::Table(tbl)
            }
            _ => toml::Value::String(clone_to_string(handle)),
        }
        _ => toml::Value::String(clone_to_string(handle)),
    }
}

/// Helper: construct a TomlError data class.
unsafe fn make_toml_error(msg: &str, line: i64, col: i64) -> FuseHandle {
    let tn = b"TomlError";
    let data = fuse_data_new(tn.as_ptr(), tn.len(), 3, None);
    fuse_data_set_field(data, 0, fuse_string_new_utf8(msg.as_ptr(), msg.len()));
    fuse_data_set_field(data, 1, fuse_int(line));
    fuse_data_set_field(data, 2, fuse_int(col));
    data
}

/// Parse a TOML string. Returns Result<TomlValue, TomlError>.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_toml_parse(input: FuseHandle) -> FuseHandle {
    let s = unsafe { clone_to_string(input) };
    match s.parse::<toml::Table>() {
        Ok(table) => {
            let val = toml::Value::Table(table);
            unsafe { fuse_ok(toml_value_to_fuse(&val)) }
        }
        Err(e) => {
            let msg = format!("{e}");
            // toml crate doesn't expose line/col directly; use 0.
            unsafe { fuse_err(make_toml_error(&msg, 0, 0)) }
        }
    }
}

/// Serialize a TomlValue to a TOML string. Returns String.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_toml_stringify(value: FuseHandle) -> FuseHandle {
    let tv = unsafe { fuse_to_toml_value(value) };
    let s = match &tv {
        toml::Value::Table(t) => toml::to_string(t).unwrap_or_default(),
        _ => toml::to_string(&tv).unwrap_or_default(),
    };
    unsafe { fuse_string_new_utf8(s.as_ptr(), s.len()) }
}

// ---------------------------------------------------------------------------
// YAML runtime support
// ---------------------------------------------------------------------------

/// Convert a serde_yaml::Value into a FuseHandle representing a YamlValue enum.
unsafe fn yaml_value_to_fuse(val: &serde_yaml::Value) -> FuseHandle {
    let tn = b"YamlValue";
    match val {
        serde_yaml::Value::Null => {
            let vn = b"Null";
            fuse_enum_new(tn.as_ptr(), tn.len(), 0, vn.as_ptr(), vn.len(), ptr::null_mut())
        }
        serde_yaml::Value::Bool(b) => {
            let vn = b"Bool";
            fuse_enum_new(tn.as_ptr(), tn.len(), 1, vn.as_ptr(), vn.len(), fuse_bool(*b))
        }
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                let vn = b"Int";
                fuse_enum_new(tn.as_ptr(), tn.len(), 2, vn.as_ptr(), vn.len(), fuse_int(i))
            } else if let Some(f) = n.as_f64() {
                let vn = b"Float";
                fuse_enum_new(tn.as_ptr(), tn.len(), 3, vn.as_ptr(), vn.len(), fuse_float(f))
            } else {
                let vn = b"Int";
                fuse_enum_new(tn.as_ptr(), tn.len(), 2, vn.as_ptr(), vn.len(), fuse_int(0))
            }
        }
        serde_yaml::Value::String(s) => {
            let vn = b"Str";
            fuse_enum_new(tn.as_ptr(), tn.len(), 4, vn.as_ptr(), vn.len(),
                fuse_string_new_utf8(s.as_ptr(), s.len()))
        }
        serde_yaml::Value::Sequence(seq) => {
            let vn = b"Seq";
            let list = fuse_list_new();
            for item in seq {
                fuse_list_push(list, yaml_value_to_fuse(item));
            }
            fuse_enum_new(tn.as_ptr(), tn.len(), 5, vn.as_ptr(), vn.len(), list)
        }
        serde_yaml::Value::Mapping(map) => {
            let vn = b"Map";
            let fuse_map = fuse_map_new();
            for (k, v) in map {
                let key_str = match k {
                    serde_yaml::Value::String(s) => s.clone(),
                    other => serde_yaml::to_string(other).unwrap_or_default().trim().to_string(),
                };
                let key = fuse_string_new_utf8(key_str.as_ptr(), key_str.len());
                fuse_map_set(fuse_map, key, yaml_value_to_fuse(v));
            }
            fuse_enum_new(tn.as_ptr(), tn.len(), 6, vn.as_ptr(), vn.len(), fuse_map)
        }
        serde_yaml::Value::Tagged(tagged) => {
            // Treat tagged values as their inner value.
            yaml_value_to_fuse(&tagged.value)
        }
    }
}

/// Convert a FuseHandle (YamlValue enum) back to a serde_yaml::Value.
unsafe fn fuse_to_yaml_value(handle: FuseHandle) -> serde_yaml::Value {
    let val = value_ref(handle);
    match &val.kind {
        ValueKind::Enum(e) => match e.variant_tag {
            0 => serde_yaml::Value::Null, // Null
            1 => { // Bool
                let p = e.payloads.first().copied().unwrap_or(ptr::null_mut());
                match &value_ref(p).kind {
                    ValueKind::Bool(b) => serde_yaml::Value::Bool(*b),
                    _ => serde_yaml::Value::Bool(false),
                }
            }
            2 => { // Int
                let p = e.payloads.first().copied().unwrap_or(ptr::null_mut());
                serde_yaml::Value::Number(serde_yaml::Number::from(extract_int(p)))
            }
            3 => { // Float
                let p = e.payloads.first().copied().unwrap_or(ptr::null_mut());
                let f = match &value_ref(p).kind {
                    ValueKind::Float(f) => *f,
                    ValueKind::Int(n) => *n as f64,
                    _ => 0.0,
                };
                serde_yaml::Value::Number(serde_yaml::Number::from(f))
            }
            4 => { // Str
                let p = e.payloads.first().copied().unwrap_or(ptr::null_mut());
                serde_yaml::Value::String(clone_to_string(p))
            }
            5 => { // Seq
                let p = e.payloads.first().copied().unwrap_or(ptr::null_mut());
                let mut seq = Vec::new();
                if let ValueKind::List(items) = &value_ref(p).kind {
                    for item in items {
                        seq.push(fuse_to_yaml_value(*item));
                    }
                }
                serde_yaml::Value::Sequence(seq)
            }
            6 => { // Map
                let p = e.payloads.first().copied().unwrap_or(ptr::null_mut());
                let mut mapping = serde_yaml::Mapping::new();
                if let ValueKind::Map(map) = &value_ref(p).kind {
                    for (k, v) in &map.entries {
                        mapping.insert(
                            serde_yaml::Value::String(clone_to_string(*k)),
                            fuse_to_yaml_value(*v),
                        );
                    }
                }
                serde_yaml::Value::Mapping(mapping)
            }
            _ => serde_yaml::Value::String(clone_to_string(handle)),
        }
        _ => serde_yaml::Value::String(clone_to_string(handle)),
    }
}

/// Helper: construct a YamlError data class.
unsafe fn make_yaml_error(msg: &str, line: i64, col: i64) -> FuseHandle {
    let tn = b"YamlError";
    let data = fuse_data_new(tn.as_ptr(), tn.len(), 3, None);
    fuse_data_set_field(data, 0, fuse_string_new_utf8(msg.as_ptr(), msg.len()));
    fuse_data_set_field(data, 1, fuse_int(line));
    fuse_data_set_field(data, 2, fuse_int(col));
    data
}

/// Parse a YAML string. Returns Result<YamlValue, YamlError>.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_yaml_parse(input: FuseHandle) -> FuseHandle {
    let s = unsafe { clone_to_string(input) };
    match serde_yaml::from_str::<serde_yaml::Value>(&s) {
        Ok(val) => unsafe { fuse_ok(yaml_value_to_fuse(&val)) },
        Err(e) => {
            let msg = format!("{e}");
            let loc = e.location();
            let line = loc.as_ref().map(|l| l.line() as i64).unwrap_or(0);
            let col = loc.as_ref().map(|l| l.column() as i64).unwrap_or(0);
            unsafe { fuse_err(make_yaml_error(&msg, line, col)) }
        }
    }
}

/// Serialize a YamlValue to a YAML string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_yaml_stringify(value: FuseHandle) -> FuseHandle {
    let yv = unsafe { fuse_to_yaml_value(value) };
    let s = serde_yaml::to_string(&yv).unwrap_or_default();
    unsafe { fuse_string_new_utf8(s.as_ptr(), s.len()) }
}

/// Serialize a YamlValue to a pretty YAML string (same as stringify for YAML).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_yaml_stringify_pretty(value: FuseHandle) -> FuseHandle {
    // YAML is already human-readable by default; same as stringify.
    unsafe { fuse_rt_yaml_stringify(value) }
}

// ---------------------------------------------------------------------------
// JSON Schema runtime support
// ---------------------------------------------------------------------------

/// Internal JSON value for schema validation (avoids re-reading FuseHandles).
#[derive(Clone, Debug)]
enum JVal {
    Null,
    JBool(bool),
    JNumber(f64),
    JStr(String),
    JArray(Vec<JVal>),
    JObject(Vec<(String, JVal)>),
}

impl JVal {
    fn type_name(&self) -> &'static str {
        match self {
            JVal::Null => "null",
            JVal::JBool(_) => "boolean",
            JVal::JNumber(n) => if n.fract() == 0.0 { "integer" } else { "number" },
            JVal::JStr(_) => "string",
            JVal::JArray(_) => "array",
            JVal::JObject(_) => "object",
        }
    }
    fn get(&self, key: &str) -> Option<&JVal> {
        if let JVal::JObject(entries) = self {
            entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
        } else {
            None
        }
    }
    fn as_str(&self) -> Option<&str> {
        if let JVal::JStr(s) = self { Some(s) } else { None }
    }
    fn as_f64(&self) -> Option<f64> {
        if let JVal::JNumber(n) = self { Some(*n) } else { None }
    }
    fn as_bool(&self) -> Option<bool> {
        if let JVal::JBool(b) = self { Some(*b) } else { None }
    }
    fn as_array(&self) -> Option<&[JVal]> {
        if let JVal::JArray(a) = self { Some(a) } else { None }
    }
    fn as_object(&self) -> Option<&[(String, JVal)]> {
        if let JVal::JObject(o) = self { Some(o) } else { None }
    }
    fn eq_val(&self, other: &JVal) -> bool {
        match (self, other) {
            (JVal::Null, JVal::Null) => true,
            (JVal::JBool(a), JVal::JBool(b)) => a == b,
            (JVal::JNumber(a), JVal::JNumber(b)) => a == b,
            (JVal::JStr(a), JVal::JStr(b)) => a == b,
            _ => false,
        }
    }
}

/// Convert a FuseHandle (JsonValue enum) to a JVal.
unsafe fn fuse_json_to_jval(handle: FuseHandle) -> JVal {
    if handle.is_null() { return JVal::Null; }
    let val = value_ref(handle);
    match &val.kind {
        ValueKind::Enum(e) => match e.variant_name.as_str() {
            "Null" => JVal::Null,
            "JBool" => {
                let p = e.payloads.first().copied().unwrap_or(ptr::null_mut());
                match &value_ref(p).kind {
                    ValueKind::Bool(b) => JVal::JBool(*b),
                    _ => JVal::JBool(false),
                }
            }
            "JNumber" => {
                let p = e.payloads.first().copied().unwrap_or(ptr::null_mut());
                match &value_ref(p).kind {
                    ValueKind::Float(f) => JVal::JNumber(*f),
                    ValueKind::Int(n) => JVal::JNumber(*n as f64),
                    _ => JVal::JNumber(0.0),
                }
            }
            "JStr" => {
                let p = e.payloads.first().copied().unwrap_or(ptr::null_mut());
                JVal::JStr(clone_to_string(p))
            }
            "JArray" => {
                let p = e.payloads.first().copied().unwrap_or(ptr::null_mut());
                let mut items = Vec::new();
                if let ValueKind::List(list) = &value_ref(p).kind {
                    for item in list { items.push(fuse_json_to_jval(*item)); }
                }
                JVal::JArray(items)
            }
            "JObject" => {
                let p = e.payloads.first().copied().unwrap_or(ptr::null_mut());
                let mut entries = Vec::new();
                if let ValueKind::List(list) = &value_ref(p).kind {
                    // JObject stores a list of [key, value] pairs (as list of lists).
                    for pair_handle in list {
                        if let ValueKind::List(pair) = &value_ref(*pair_handle).kind {
                            if pair.len() >= 2 {
                                // Key is a JsonValue.JStr — extract the inner string.
                                let key_jval = fuse_json_to_jval(pair[0]);
                                let key = match key_jval {
                                    JVal::JStr(s) => s,
                                    _ => clone_to_string(pair[0]),
                                };
                                entries.push((key, fuse_json_to_jval(pair[1])));
                            }
                        }
                    }
                }
                // Also handle Map representation.
                if entries.is_empty() {
                    if let ValueKind::Map(map) = &value_ref(p).kind {
                        for (k, v) in &map.entries {
                            entries.push((clone_to_string(*k), fuse_json_to_jval(*v)));
                        }
                    }
                }
                JVal::JObject(entries)
            }
            _ => JVal::Null,
        }
        // Handle raw primitives (if passed directly instead of as enum).
        ValueKind::Bool(b) => JVal::JBool(*b),
        ValueKind::Int(n) => JVal::JNumber(*n as f64),
        ValueKind::Float(f) => JVal::JNumber(*f),
        ValueKind::String(s) => JVal::JStr(s.clone()),
        ValueKind::List(_) => {
            let mut items = Vec::new();
            if let ValueKind::List(list) = &val.kind {
                for item in list { items.push(fuse_json_to_jval(*item)); }
            }
            JVal::JArray(items)
        }
        _ => JVal::Null,
    }
}

/// Validate a value against a JSON Schema (JVal).  Pushes errors to `errs`.
fn json_schema_validate(schema: &JVal, value: &JVal, path: &str, errs: &mut Vec<(String, String)>) {
    // "type" keyword
    if let Some(ty) = schema.get("type") {
        let actual = value.type_name();
        match ty {
            JVal::JStr(expected) => {
                let ok = expected == actual
                    || (expected == "number" && actual == "integer");
                if !ok {
                    errs.push((path.to_string(), format!("expected type {expected}, got {actual}")));
                    return;
                }
            }
            JVal::JArray(types) => {
                let ok = types.iter().any(|t| {
                    if let JVal::JStr(s) = t {
                        s == actual || (s == "number" && actual == "integer")
                    } else { false }
                });
                if !ok {
                    errs.push((path.to_string(), format!("type {actual} not in allowed types")));
                    return;
                }
            }
            _ => {}
        }
    }
    // "enum" keyword
    if let Some(JVal::JArray(options)) = schema.get("enum") {
        if !options.iter().any(|o| o.eq_val(value)) {
            errs.push((path.to_string(), "value not in enum".to_string()));
        }
    }
    // "const" keyword
    if let Some(constant) = schema.get("const") {
        if !constant.eq_val(value) {
            errs.push((path.to_string(), "value does not match const".to_string()));
        }
    }
    // String constraints
    if let JVal::JStr(s) = value {
        if let Some(JVal::JNumber(n)) = schema.get("minLength") {
            if (s.len() as f64) < *n { errs.push((path.to_string(), format!("string shorter than minLength {n}"))); }
        }
        if let Some(JVal::JNumber(n)) = schema.get("maxLength") {
            if (s.len() as f64) > *n { errs.push((path.to_string(), format!("string longer than maxLength {n}"))); }
        }
    }
    // Number constraints
    if let JVal::JNumber(n) = value {
        if let Some(JVal::JNumber(min)) = schema.get("minimum") {
            if n < min { errs.push((path.to_string(), format!("value {n} < minimum {min}"))); }
        }
        if let Some(JVal::JNumber(max)) = schema.get("maximum") {
            if n > max { errs.push((path.to_string(), format!("value {n} > maximum {max}"))); }
        }
        if let Some(JVal::JNumber(emin)) = schema.get("exclusiveMinimum") {
            if n <= emin { errs.push((path.to_string(), format!("value {n} <= exclusiveMinimum {emin}"))); }
        }
        if let Some(JVal::JNumber(emax)) = schema.get("exclusiveMaximum") {
            if n >= emax { errs.push((path.to_string(), format!("value {n} >= exclusiveMaximum {emax}"))); }
        }
    }
    // Array constraints
    if let JVal::JArray(items) = value {
        if let Some(JVal::JNumber(n)) = schema.get("minItems") {
            if (items.len() as f64) < *n { errs.push((path.to_string(), format!("array has fewer than minItems {n}"))); }
        }
        if let Some(JVal::JNumber(n)) = schema.get("maxItems") {
            if (items.len() as f64) > *n { errs.push((path.to_string(), format!("array has more than maxItems {n}"))); }
        }
        if let Some(item_schema) = schema.get("items") {
            for (i, item) in items.iter().enumerate() {
                let item_path = format!("{path}[{i}]");
                json_schema_validate(item_schema, item, &item_path, errs);
            }
        }
    }
    // Object constraints
    if let JVal::JObject(entries) = value {
        if let Some(JVal::JArray(req)) = schema.get("required") {
            for r in req {
                if let JVal::JStr(name) = r {
                    if !entries.iter().any(|(k, _)| k == name) {
                        errs.push((path.to_string(), format!("missing required property \"{name}\"")));
                    }
                }
            }
        }
        if let Some(JVal::JObject(props)) = schema.get("properties") {
            for (key, prop_schema) in props {
                if let Some((_, val)) = entries.iter().find(|(k, _)| k == key) {
                    let prop_path = if path.is_empty() { key.clone() } else { format!("{path}.{key}") };
                    json_schema_validate(prop_schema, val, &prop_path, errs);
                }
            }
        }
    }
}

thread_local! {
    static SCHEMA_STORE: RefCell<HashMap<i64, JVal>> = RefCell::new(HashMap::new());
    static SCHEMA_NEXT_ID: Cell<i64> = const { Cell::new(1) };
}

/// Compile a JSON Schema from a JsonValue.  Returns Result<Int, String>.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_json_schema_compile(schema: FuseHandle) -> FuseHandle {
    let jval = unsafe { fuse_json_to_jval(schema) };
    if !matches!(jval, JVal::JObject(_)) {
        let msg = "schema must be a JSON object";
        return unsafe { fuse_err(fuse_string_new_utf8(msg.as_ptr(), msg.len())) };
    }
    let id = SCHEMA_NEXT_ID.with(|c| { let id = c.get(); c.set(id + 1); id });
    SCHEMA_STORE.with(|store| store.borrow_mut().insert(id, jval));
    unsafe { fuse_ok(fuse_int(id)) }
}

/// Helper: convert serde_json::Value to JVal.
fn serde_json_to_jval(v: &serde_yaml::Value) -> JVal {
    match v {
        serde_yaml::Value::Null => JVal::Null,
        serde_yaml::Value::Bool(b) => JVal::JBool(*b),
        serde_yaml::Value::Number(n) => JVal::JNumber(n.as_f64().unwrap_or(0.0)),
        serde_yaml::Value::String(s) => JVal::JStr(s.clone()),
        serde_yaml::Value::Sequence(arr) => JVal::JArray(arr.iter().map(serde_json_to_jval).collect()),
        serde_yaml::Value::Mapping(obj) => {
            let entries = obj.iter().map(|(k, v)| {
                let key = match k {
                    serde_yaml::Value::String(s) => s.clone(),
                    other => format!("{other:?}"),
                };
                (key, serde_json_to_jval(v))
            }).collect();
            JVal::JObject(entries)
        }
        serde_yaml::Value::Tagged(t) => serde_json_to_jval(&t.value),
    }
}

/// Compile a JSON Schema from a JSON string.  Returns Result<Int, String>.
/// This bypasses the Fuse JSON parser and uses serde_yaml (JSON-compatible).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_json_schema_compile_str(input: FuseHandle) -> FuseHandle {
    let s = unsafe { clone_to_string(input) };
    let parsed: Result<serde_yaml::Value, _> = serde_yaml::from_str(&s);
    match parsed {
        Ok(val) => {
            let jval = serde_json_to_jval(&val);
            if !matches!(jval, JVal::JObject(_)) {
                let msg = "schema must be a JSON object";
                return unsafe { fuse_err(fuse_string_new_utf8(msg.as_ptr(), msg.len())) };
            }
            let id = SCHEMA_NEXT_ID.with(|c| { let id = c.get(); c.set(id + 1); id });
            SCHEMA_STORE.with(|store| store.borrow_mut().insert(id, jval));
            unsafe { fuse_ok(fuse_int(id)) }
        }
        Err(e) => {
            let msg = format!("{e}");
            unsafe { fuse_err(fuse_string_new_utf8(msg.as_ptr(), msg.len())) }
        }
    }
}

/// Validate a JSON string against a compiled schema.
/// Returns Result<Unit, List<ValidationError>>.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_json_schema_validate_str(
    schema_id: FuseHandle,
    input: FuseHandle,
) -> FuseHandle {
    let id = extract_int(schema_id);
    let s = unsafe { clone_to_string(input) };
    let parsed: Result<serde_yaml::Value, _> = serde_yaml::from_str(&s);
    let jval = match parsed {
        Ok(val) => serde_json_to_jval(&val),
        Err(e) => {
            let msg = format!("invalid JSON: {e}");
            let list = unsafe { fuse_list_new() };
            let tn = b"ValidationError";
            let data = unsafe { fuse_data_new(tn.as_ptr(), tn.len(), 2, None) };
            let empty = b"";
            unsafe { fuse_data_set_field(data, 0, fuse_string_new_utf8(empty.as_ptr(), 0)) };
            unsafe { fuse_data_set_field(data, 1, fuse_string_new_utf8(msg.as_ptr(), msg.len())) };
            unsafe { fuse_list_push(list, data) };
            return unsafe { fuse_err(list) };
        }
    };
    let errors = SCHEMA_STORE.with(|store| {
        let store = store.borrow();
        let Some(schema) = store.get(&id) else {
            return vec![("".to_string(), "invalid schema handle".to_string())];
        };
        let mut errs = Vec::new();
        json_schema_validate(schema, &jval, "", &mut errs);
        errs
    });
    if errors.is_empty() {
        return unsafe { fuse_ok(fuse_unit()) };
    }
    let list = unsafe { fuse_list_new() };
    for (path, msg) in &errors {
        let tn = b"ValidationError";
        let data = unsafe { fuse_data_new(tn.as_ptr(), tn.len(), 2, None) };
        unsafe { fuse_data_set_field(data, 0, fuse_string_new_utf8(path.as_ptr(), path.len())) };
        unsafe { fuse_data_set_field(data, 1, fuse_string_new_utf8(msg.as_ptr(), msg.len())) };
        unsafe { fuse_list_push(list, data) };
    }
    unsafe { fuse_err(list) }
}

/// Validate a JsonValue against a compiled schema.
/// Returns Result<Unit, List<ValidationError>>.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_json_schema_validate(
    schema_id: FuseHandle,
    value: FuseHandle,
) -> FuseHandle {
    let id = extract_int(schema_id);
    let jval = unsafe { fuse_json_to_jval(value) };
    let errors = SCHEMA_STORE.with(|store| {
        let store = store.borrow();
        let Some(schema) = store.get(&id) else {
            return vec![("".to_string(), "invalid schema handle".to_string())];
        };
        let mut errs = Vec::new();
        json_schema_validate(schema, &jval, "", &mut errs);
        errs
    });
    if errors.is_empty() {
        return unsafe { fuse_ok(fuse_unit()) };
    }
    // Build List<ValidationError>.
    let err_list = unsafe { fuse_list_new() };
    for (path, msg) in &errors {
        let tn = b"ValidationError";
        let data = unsafe { fuse_data_new(tn.as_ptr(), tn.len(), 2, None) };
        unsafe { fuse_data_set_field(data, 0, fuse_string_new_utf8(path.as_ptr(), path.len())) };
        unsafe { fuse_data_set_field(data, 1, fuse_string_new_utf8(msg.as_ptr(), msg.len())) };
        unsafe { fuse_list_push(err_list, data) };
    }
    unsafe { fuse_err(err_list) }
}

// ---------------------------------------------------------------------------
// Crypto runtime support
// ---------------------------------------------------------------------------

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// SHA-256 hash of a string.  Returns hex-encoded hash.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_crypto_sha256(data: FuseHandle) -> FuseHandle {
    use sha2::Digest;
    let s = unsafe { clone_to_string(data) };
    let hash = sha2::Sha256::digest(s.as_bytes());
    let hex = hex_encode(&hash);
    unsafe { fuse_string_new_utf8(hex.as_ptr(), hex.len()) }
}

/// SHA-256 hash of a byte list.  Returns List<Int>.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_crypto_sha256_bytes(data: FuseHandle) -> FuseHandle {
    use sha2::Digest;
    let mut bytes = Vec::new();
    if let ValueKind::List(items) = &unsafe { value_ref(data) }.kind {
        for item in items {
            bytes.push(extract_int(*item) as u8);
        }
    }
    let hash = sha2::Sha256::digest(&bytes);
    let list = unsafe { fuse_list_new() };
    for b in hash.as_slice() {
        unsafe { fuse_list_push(list, fuse_int(*b as i64)) };
    }
    list
}

/// SHA-512 hash.  Returns hex string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_crypto_sha512(data: FuseHandle) -> FuseHandle {
    use sha2::Digest;
    let s = unsafe { clone_to_string(data) };
    let hash = sha2::Sha512::digest(s.as_bytes());
    let hex = hex_encode(&hash);
    unsafe { fuse_string_new_utf8(hex.as_ptr(), hex.len()) }
}

/// MD5 hash (legacy).  Returns hex string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_crypto_md5(data: FuseHandle) -> FuseHandle {
    use md5::Digest;
    let s = unsafe { clone_to_string(data) };
    let hash = md5::Md5::digest(s.as_bytes());
    let hex = hex_encode(&hash);
    unsafe { fuse_string_new_utf8(hex.as_ptr(), hex.len()) }
}

/// BLAKE3 hash.  Returns hex string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_crypto_blake3(data: FuseHandle) -> FuseHandle {
    let s = unsafe { clone_to_string(data) };
    let hash = blake3::hash(s.as_bytes());
    let hex = hash.to_hex().to_string();
    unsafe { fuse_string_new_utf8(hex.as_ptr(), hex.len()) }
}

/// HMAC-SHA256.  Returns hex string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_crypto_hmac_sha256(
    key: FuseHandle,
    data: FuseHandle,
) -> FuseHandle {
    use hmac::{Hmac, Mac};
    type HmacSha256 = Hmac<sha2::Sha256>;
    let k = unsafe { clone_to_string(key) };
    let d = unsafe { clone_to_string(data) };
    let mut mac = HmacSha256::new_from_slice(k.as_bytes()).unwrap();
    mac.update(d.as_bytes());
    let result = mac.finalize().into_bytes();
    let hex = hex_encode(&result);
    unsafe { fuse_string_new_utf8(hex.as_ptr(), hex.len()) }
}

/// Constant-time string comparison.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_crypto_constant_time_eq(
    a: FuseHandle,
    b: FuseHandle,
) -> FuseHandle {
    let sa = unsafe { clone_to_string(a) };
    let sb = unsafe { clone_to_string(b) };
    let eq = if sa.len() != sb.len() {
        false
    } else {
        let mut result = 0u8;
        for (x, y) in sa.as_bytes().iter().zip(sb.as_bytes()) {
            result |= x ^ y;
        }
        result == 0
    };
    unsafe { fuse_bool(eq) }
}

/// Cryptographically secure random bytes.  Returns List<Int>.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_crypto_random_bytes(n: FuseHandle) -> FuseHandle {
    let count = extract_int(n) as usize;
    let mut buf = vec![0u8; count];
    getrandom::getrandom(&mut buf).unwrap_or(());
    let list = unsafe { fuse_list_new() };
    for b in &buf {
        unsafe { fuse_list_push(list, fuse_int(*b as i64)) };
    }
    list
}

/// Random hex string (n bytes → 2n hex chars).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_crypto_random_hex(n: FuseHandle) -> FuseHandle {
    let count = extract_int(n) as usize;
    let mut buf = vec![0u8; count];
    getrandom::getrandom(&mut buf).unwrap_or(());
    let hex = hex_encode(&buf);
    unsafe { fuse_string_new_utf8(hex.as_ptr(), hex.len()) }
}

// ---------------------------------------------------------------------------
// HTTP Server runtime support
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
struct Route {
    method: String,
    path: String,
    closure: FuseHandle, // Fuse closure list: [fn_ptr, captures...]
}

#[cfg(not(target_arch = "wasm32"))]
thread_local! {
    static HTTP_ROUTES: RefCell<Vec<Route>> = RefCell::new(Vec::new());
}

/// Register a route.  `closure` is a Fuse closure (List handle).
#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_http_server_route(
    method: FuseHandle,
    path: FuseHandle,
    closure: FuseHandle,
) -> FuseHandle {
    let m = unsafe { clone_to_string(method) };
    let p = unsafe { clone_to_string(path) };
    HTTP_ROUTES.with(|routes| {
        routes.borrow_mut().push(Route { method: m, path: p, closure });
    });
    FuseValue::new(ValueKind::Unit)
}

/// Build a Request data class from tiny_http::Request parts.
#[cfg(not(target_arch = "wasm32"))]
unsafe fn make_request(
    method: &str,
    path: &str,
    query: &str,
    body: &str,
    headers: &[(String, String)],
) -> FuseHandle {
    let tn = b"Request";
    let data = fuse_data_new(tn.as_ptr(), tn.len(), 5, None);
    // field 0: method
    fuse_data_set_field(data, 0, fuse_string_new_utf8(method.as_ptr(), method.len()));
    // field 1: path
    fuse_data_set_field(data, 1, fuse_string_new_utf8(path.as_ptr(), path.len()));
    // field 2: headers (Map<String, String>)
    let hdr_map = fuse_map_new();
    for (k, v) in headers {
        fuse_map_set(hdr_map,
            fuse_string_new_utf8(k.as_ptr(), k.len()),
            fuse_string_new_utf8(v.as_ptr(), v.len()));
    }
    fuse_data_set_field(data, 2, hdr_map);
    // field 3: query (Map<String, String>)
    let q_map = fuse_map_new();
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            fuse_map_set(q_map,
                fuse_string_new_utf8(k.as_ptr(), k.len()),
                fuse_string_new_utf8(v.as_ptr(), v.len()));
        }
    }
    fuse_data_set_field(data, 3, q_map);
    // field 4: body
    fuse_data_set_field(data, 4, fuse_string_new_utf8(body.as_ptr(), body.len()));
    data
}

/// Extract Response fields from a Fuse data class (status, body, contentType).
#[cfg(not(target_arch = "wasm32"))]
unsafe fn extract_response(handle: FuseHandle) -> (u16, String, String) {
    let val = value_ref(handle);
    if let ValueKind::Data(d) = &val.kind {
        let status = if d.fields.len() > 0 { extract_int(d.fields[0]) as u16 } else { 200 };
        let body = if d.fields.len() > 1 { clone_to_string(d.fields[1]) } else { String::new() };
        let ct = if d.fields.len() > 2 { clone_to_string(d.fields[2]) } else { "text/plain".to_string() };
        (status, body, ct)
    } else {
        (200, clone_to_string(handle), "text/plain".to_string())
    }
}

/// Call a Fuse handler closure with a request, returning a response handle.
#[cfg(not(target_arch = "wasm32"))]
unsafe fn call_handler(closure: FuseHandle, request: FuseHandle) -> FuseHandle {
    let fn_ptr_raw: FuseHandle = {
        let list_val = value_ref(closure);
        match &list_val.kind {
            ValueKind::List(items) if !items.is_empty() => items[0],
            _ => return fuse_unit(),
        }
    };
    let fn_ptr: unsafe extern "C" fn(FuseHandle, FuseHandle) -> FuseHandle =
        std::mem::transmute(fn_ptr_raw);
    fn_ptr(closure, request)
}

/// Start the HTTP server.  Blocks until the server is stopped.
#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_rt_http_server_listen(
    host: FuseHandle,
    port: FuseHandle,
    _threads: FuseHandle,
) -> FuseHandle {
    let h = unsafe { clone_to_string(host) };
    let p = extract_int(port);
    let addr = format!("{h}:{p}");
    let server = match tiny_http::Server::http(&addr) {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("http_server: {e}");
            return unsafe { fuse_err(fuse_string_new_utf8(msg.as_ptr(), msg.len())) };
        }
    };
    eprintln!("Fuse HTTP server listening on {addr}");
    for mut request in server.incoming_requests() {
        let method = request.method().to_string();
        let raw_url = request.url().to_string();
        let (path, query) = raw_url.split_once('?').unwrap_or((&raw_url, ""));
        let headers: Vec<(String, String)> = request.headers()
            .iter()
            .map(|h| (h.field.as_str().as_str().to_string(), h.value.as_str().to_string()))
            .collect();
        let mut body_buf = String::new();
        let _ = request.as_reader().read_to_string(&mut body_buf);

        // Find matching route.
        let handler = HTTP_ROUTES.with(|routes| {
            let routes = routes.borrow();
            routes.iter().find(|r| r.method == method && r.path == path)
                .map(|r| r.closure)
        });

        if let Some(closure) = handler {
            let req_handle = unsafe { make_request(&method, path, query, &body_buf, &headers) };
            let resp_handle = unsafe { call_handler(closure, req_handle) };
            let (status, body, content_type) = unsafe { extract_response(resp_handle) };
            let response = tiny_http::Response::from_string(body)
                .with_status_code(status)
                .with_header(
                    tiny_http::Header::from_bytes(
                        b"Content-Type" as &[u8],
                        content_type.as_bytes(),
                    ).unwrap()
                );
            let _ = request.respond(response);
        } else {
            let response = tiny_http::Response::from_string("Not Found")
                .with_status_code(404);
            let _ = request.respond(response);
        }
    }
    unsafe { fuse_ok(fuse_unit()) }
}
