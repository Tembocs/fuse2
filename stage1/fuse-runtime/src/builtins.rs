use crate::value::FuseHandle;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_builtin_println(handle: FuseHandle) {
    // Safety: the runtime handle originates from generated code.
    unsafe { crate::value::fuse_println(handle) };
}
