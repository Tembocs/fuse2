use crate::value::{fuse_chan_bounded, fuse_chan_new, fuse_chan_recv, fuse_chan_send, FuseHandle};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_runtime_new() -> FuseHandle {
    unsafe { fuse_chan_new() }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_runtime_bounded(capacity: usize) -> FuseHandle {
    unsafe { fuse_chan_bounded(capacity) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_runtime_send(chan: FuseHandle, value: FuseHandle) {
    unsafe { fuse_chan_send(chan, value) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fuse_chan_runtime_recv(chan: FuseHandle) -> FuseHandle {
    unsafe { fuse_chan_recv(chan) }
}
