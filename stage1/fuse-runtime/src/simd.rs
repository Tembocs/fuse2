use crate::value::{
    fuse_simd_abs, fuse_simd_add, fuse_simd_broadcast, fuse_simd_div, fuse_simd_dot,
    fuse_simd_get, fuse_simd_len, fuse_simd_max, fuse_simd_min, fuse_simd_mul,
    fuse_simd_sqrt, fuse_simd_sub, fuse_simd_sum, FuseHandle,
};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_runtime_sum(list: FuseHandle) -> FuseHandle {
    unsafe { fuse_simd_sum(list) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_runtime_dot(a: FuseHandle, b: FuseHandle) -> FuseHandle {
    unsafe { fuse_simd_dot(a, b) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_runtime_add(a: FuseHandle, b: FuseHandle) -> FuseHandle {
    unsafe { fuse_simd_add(a, b) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_runtime_sub(a: FuseHandle, b: FuseHandle) -> FuseHandle {
    unsafe { fuse_simd_sub(a, b) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_runtime_mul(a: FuseHandle, b: FuseHandle) -> FuseHandle {
    unsafe { fuse_simd_mul(a, b) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_runtime_div(a: FuseHandle, b: FuseHandle) -> FuseHandle {
    unsafe { fuse_simd_div(a, b) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_runtime_min(a: FuseHandle, b: FuseHandle) -> FuseHandle {
    unsafe { fuse_simd_min(a, b) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_runtime_max(a: FuseHandle, b: FuseHandle) -> FuseHandle {
    unsafe { fuse_simd_max(a, b) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_runtime_abs(list: FuseHandle) -> FuseHandle {
    unsafe { fuse_simd_abs(list) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_runtime_sqrt(list: FuseHandle) -> FuseHandle {
    unsafe { fuse_simd_sqrt(list) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_runtime_broadcast(value: FuseHandle, lanes: i64) -> FuseHandle {
    unsafe { fuse_simd_broadcast(value, lanes) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_runtime_get(list: FuseHandle, index: FuseHandle) -> FuseHandle {
    unsafe { fuse_simd_get(list, index) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_runtime_len(list: FuseHandle) -> FuseHandle {
    unsafe { fuse_simd_len(list) }
}
