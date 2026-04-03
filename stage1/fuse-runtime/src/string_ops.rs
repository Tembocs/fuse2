use crate::value::FuseHandle;

pub unsafe fn fuse_string_to_upper(handle: FuseHandle) -> FuseHandle {
    // Safety: the runtime handle originates from generated code.
    unsafe { crate::value::fuse_to_upper(handle) }
}

pub unsafe fn fuse_string_is_empty(handle: FuseHandle) -> FuseHandle {
    // Safety: the runtime handle originates from generated code.
    unsafe { crate::value::fuse_string_is_empty(handle) }
}
