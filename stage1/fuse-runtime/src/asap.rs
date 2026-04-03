use crate::value::{fuse_release, FuseHandle};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_asap_release(handle: FuseHandle) {
    // Safety: the generated code only passes runtime-owned handles.
    unsafe { fuse_release(handle) };
}
