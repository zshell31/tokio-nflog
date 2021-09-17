use nflog_sys::*;

use std::borrow::Cow;
use std::ffi::CStr;
use std::marker::PhantomData;
use std::ptr::NonNull;

#[derive(Debug)]
pub struct Message<'a> {
    inner: NonNull<nflog_data>,
    _lifetime: PhantomData<&'a nflog_data>,
}

impl<'a> Message<'a> {
    pub(crate) unsafe fn new(inner: *mut nflog_data) -> Self {
        Message {
            inner: NonNull::new(inner).expect("non-null nflog_data"),
            _lifetime: PhantomData,
        }
    }

    pub fn payload(&self) -> &'a [u8] {
        let mut c_ptr = std::ptr::null_mut();
        let payload_len = unsafe { nflog_get_payload(self.inner.as_ptr(), &mut c_ptr) };
        let payload: &[u8] =
            unsafe { std::slice::from_raw_parts(c_ptr as *const u8, payload_len as usize) };

        payload
    }

    pub fn nfmark(&self) -> u32 {
        unsafe { nflog_get_nfmark(self.inner.as_ptr()) }
    }

    pub fn prefix(&self) -> Cow<'_, str> {
        let c_buf: *const libc::c_char = unsafe { nflog_get_prefix(self.inner.as_ptr()) };
        let cstr = unsafe { CStr::from_ptr(c_buf) };
        cstr.to_string_lossy()
    }

    pub fn l3_proto(&self) -> u16 {
        let packet_hdr = unsafe { *nflog_get_msg_packet_hdr(self.inner.as_ptr()) };
        u16::from_be(packet_hdr.hw_protocol)
    }
}
