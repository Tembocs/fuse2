use crate::value::{fuse_simd_sum, FuseHandle};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_simd_runtime_sum(list: FuseHandle) -> FuseHandle {
    unsafe { fuse_simd_sum(list) }
}
