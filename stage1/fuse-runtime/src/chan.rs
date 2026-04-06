use crate::value::{
    fuse_chan_bounded, fuse_chan_cap, fuse_chan_close, fuse_chan_is_closed, fuse_chan_len,
    fuse_chan_new, fuse_chan_recv, fuse_chan_send, fuse_chan_try_recv, FuseHandle,
};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_runtime_new() -> FuseHandle {
    unsafe { fuse_chan_new() }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_runtime_bounded(capacity_handle: FuseHandle) -> FuseHandle {
    unsafe {
        let capacity = crate::value::extract_int(capacity_handle) as usize;
        fuse_chan_bounded(capacity)
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_runtime_send(chan: FuseHandle, value: FuseHandle) -> FuseHandle {
    unsafe { fuse_chan_send(chan, value) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_runtime_recv(chan: FuseHandle) -> FuseHandle {
    unsafe { fuse_chan_recv(chan) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_runtime_try_recv(chan: FuseHandle) -> FuseHandle {
    unsafe { fuse_chan_try_recv(chan) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_runtime_close(chan: FuseHandle) {
    unsafe { fuse_chan_close(chan) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_runtime_is_closed(chan: FuseHandle) -> bool {
    unsafe { fuse_chan_is_closed(chan) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_runtime_len(chan: FuseHandle) -> i64 {
    unsafe { fuse_chan_len(chan) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_runtime_cap(chan: FuseHandle) -> FuseHandle {
    unsafe { fuse_chan_cap(chan) }
}
