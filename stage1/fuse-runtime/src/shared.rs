use crate::value::{fuse_shared_new, fuse_shared_read, fuse_shared_write, FuseHandle};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_shared_runtime_new(value: FuseHandle) -> FuseHandle {
    unsafe { fuse_shared_new(value) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_shared_runtime_read(shared: FuseHandle) -> FuseHandle {
    unsafe { fuse_shared_read(shared) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_shared_runtime_write(shared: FuseHandle) -> FuseHandle {
    unsafe { fuse_shared_write(shared) }
}
