mod message;

use bitflags::bitflags;
use nflog_sys::*;
use std::io::{self, Read};
use std::marker::PhantomData;
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net::UnixDatagram;
use std::os::unix::prelude::FromRawFd;
use std::ptr::NonNull;
use tokio::io::unix::AsyncFd;

pub use message::Message;

#[repr(u8)]
pub enum CopyMode {
    /// Do not copy packet contents nor metadata
    None = NFULNL_COPY_NONE,
    /// Copy only packet metadata, not payload
    Meta = NFULNL_COPY_META,
    /// Copy packet metadata and not payload
    Packet = NFULNL_COPY_PACKET,
}

bitflags! {
    /// Configuration Flags
    pub struct Flags: u16 {
        const Sequence = NFULNL_CFG_F_SEQ;
        const GlobalSequence = NFULNL_CFG_F_SEQ_GLOBAL;
    }
}

#[derive(Debug)]
pub struct Queue {
    handle: NonNull<nflog_handle>,
}

macro_rules! wrap_in_result {
    ($e:expr) => {{
        let result = unsafe { $e };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }};
}

impl Queue {
    pub fn open() -> io::Result<Self> {
        let handle = unsafe { nflog_open() };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }

        Ok(Queue {
            handle: unsafe { NonNull::new_unchecked(handle) },
        })
    }

    pub fn unbind(&self, protocol_family: libc::c_int) -> io::Result<()> {
        wrap_in_result!(nflog_unbind_pf(
            self.handle.as_ptr(),
            protocol_family as u16
        ))
    }

    pub fn bind(&self, protocol_family: libc::c_int) -> io::Result<()> {
        wrap_in_result!(nflog_bind_pf(self.handle.as_ptr(), protocol_family as u16))
    }

    pub fn bind_group(&self, num: u16) -> io::Result<Group> {
        let group_handle = unsafe { nflog_bind_group(self.handle.as_ptr(), num) };
        if group_handle.is_null() {
            return Err(io::Error::last_os_error());
        }

        Ok(Group {
            handle: unsafe { NonNull::new_unchecked(group_handle) },
            group: num,
            queue_lifetime: PhantomData,
            has_callback: false,
        })
    }

    pub fn fd(&self) -> RawFd {
        unsafe { nflog_fd(self.handle.as_ptr()) }
    }

    pub fn listen(self) -> io::Result<QueueListener> {
        QueueListener::new(self)
    }

    pub fn run_loop(&self) -> ! {
        let fd = self.fd();
        let mut buf = vec![0u8; 0x10000];
        let buf_ptr = buf.as_mut_ptr() as *mut libc::c_void;
        let buf_len = buf.len() as libc::size_t;

        loop {
            let rc = unsafe { libc::recv(fd, buf_ptr, buf_len, 0) };
            if rc < 0 {
                panic!("error in recv: {:?}", ::std::io::Error::last_os_error());
            };

            unsafe {
                nflog_handle_packet(
                    self.handle.as_ptr(),
                    buf_ptr as *mut libc::c_char,
                    rc as libc::c_int,
                )
            };
        }
    }
}

impl AsRawFd for Queue {
    fn as_raw_fd(&self) -> RawFd {
        self.fd()
    }
}

pub struct QueueListener {
    socket: AsyncFd<UnixDatagram>,
    inner: Queue,
}

impl QueueListener {
    fn new(queue: Queue) -> io::Result<Self> {
        Ok(Self {
            socket: AsyncFd::new(unsafe { UnixDatagram::from_raw_fd(queue.fd()) })?,
            inner: queue,
        })
    }

    pub fn into_inner(self) -> Queue {
        self.inner
    }

    pub async fn recv(&self, out: &mut [u8]) -> io::Result<usize> {
        loop {
            let mut guard = self.socket.readable().await?;

            match guard.try_io(|inner| inner.get_ref().recv(out)) {
                Ok(result) => return result,
                Err(_would_block) => continue,
            }
        }
    }
}

extern "C" fn callback(
    _gh: *mut nflog_g_handle,
    _nfmsg: *mut nfgenmsg,
    nfd: *mut nflog_data,
    data: *mut std::os::raw::c_void,
) -> libc::c_int {
    if data.is_null() {
        return 1;
    }

    println!("callback");
    println!("data: {:p}", data as *const u64);
    println!("trait obj data: {:#x}", unsafe { *(data as *const u64) });
    println!("trait obj vtable: {:#x}", unsafe {
        *(data as *const u64).add(1)
    });

    let result = std::panic::catch_unwind(|| {
        let boxed_cb = unsafe { Box::from_raw(data as *mut Box<dyn Fn(Message)>) };
        // println!(
        //     "boxed: {:p} {}",
        //     boxed_cb,
        //     std::mem::size_of::<Box<Box<dyn Fn(Message)>>>()
        // );
        let cb = &*boxed_cb;
        // println!(
        //     "boxed inner: {:p} {}",
        //     cb,
        //     std::mem::size_of::<Box<dyn Fn(Message)>>()
        // );

        let msg = unsafe { Message::new(nfd) };
        cb(msg);
        //Box::leak(boxed_cb);
    });

    match result {
        Ok(_) => 0,
        Err(_) => 1,
    }
}

pub struct Group<'a> {
    handle: NonNull<nflog_g_handle>,
    group: u16,
    queue_lifetime: PhantomData<&'a Queue>,
    has_callback: bool,
}

impl<'a> Group<'a> {
    pub fn set_mode(&mut self, mode: CopyMode, range: u32) {
        unsafe {
            nflog_set_mode(self.handle.as_ptr(), mode as u8, range);
        }
    }

    pub fn set_flags(&mut self, flags: Flags) {
        unsafe {
            nflog_set_flags(self.handle.as_ptr(), flags.bits());
        }
    }

    pub fn set_callback<F: Fn(Message) + 'static>(&mut self, f: F) {
        //println!("registration");
        let boxed_cb = Box::new(f) as Box<dyn Fn(Message)>;
        //println!("boxed inner: {:p}", boxed_cb);
        let cb = Box::new(boxed_cb);
        //println!("boxed: {:p}", cb);
        unsafe {
            nflog_callback_register(
                self.handle.as_ptr(),
                Some(callback),
                Box::into_raw(cb) as *mut _,
            )
        };
        self.has_callback = true;
    }
}
