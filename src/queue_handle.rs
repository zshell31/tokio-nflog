use nflog_sys::*;
use std::io;
use std::os::unix::prelude::RawFd;
use std::ptr::NonNull;

use super::{AddressFamily, CopyMode, Flags};

pub(crate) struct QueueHandle {
    handle: NonNull<nflog_handle>,
    group_handle: Option<NonNull<nflog_g_handle>>,
}

impl QueueHandle {
    pub(crate) fn open() -> io::Result<Self> {
        let handle = unsafe { nflog_open() };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }

        Ok(QueueHandle {
            handle: unsafe { NonNull::new_unchecked(handle) },
            group_handle: None,
        })
    }

    pub(crate) fn as_ptr(&self) -> *mut nflog_handle {
        self.handle.as_ptr()
    }

    pub(crate) fn bind(&self, address_family: AddressFamily) -> io::Result<()> {
        wrap_io_result!(nflog_bind_pf(self.handle.as_ptr(), address_family as u16))
    }

    pub(crate) fn unbind(&self, address_family: AddressFamily) -> io::Result<()> {
        wrap_io_result!(nflog_unbind_pf(self.handle.as_ptr(), address_family as u16))
    }

    pub(crate) fn bind_group(&mut self, group_num: u16) -> io::Result<()> {
        let group_handle = unsafe { nflog_bind_group(self.handle.as_ptr(), group_num) };
        if group_handle.is_null() {
            return Err(io::Error::last_os_error());
        }

        let group_handle = unsafe { NonNull::new_unchecked(group_handle) };
        self.group_handle = Some(group_handle);

        Ok(())
    }

    pub(crate) fn fd(&self) -> RawFd {
        unsafe { nflog_fd(self.handle.as_ptr()) }
    }

    pub(crate) fn group_handle(&self) -> io::Result<NonNull<nflog_g_handle>> {
        self.group_handle
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "group handle is not initialized"))
    }

    pub(crate) fn set_mode(&mut self, mode: CopyMode, range: u32) -> io::Result<()> {
        let ghandle = self.group_handle()?;

        wrap_io_result!(nflog_set_mode(ghandle.as_ptr(), mode as u8, range))
    }

    pub(crate) fn set_flags(&mut self, flags: Flags) -> io::Result<()> {
        let ghandle = self.group_handle()?;

        wrap_io_result!(nflog_set_flags(ghandle.as_ptr(), flags.bits()))
    }
}

impl Drop for QueueHandle {
    fn drop(&mut self) {
        if let Some(group_handle) = self.group_handle.take() {
            println!("Drop group handle");
            unsafe { nflog_unbind_group(group_handle.as_ptr()) };
        }

        println!("Drop handle");
        unsafe { nflog_close(self.handle.as_ptr()) };
    }
}
