#[macro_use]
mod macros;

mod message;
mod queue_handle;

use bitflags::bitflags;
use bytes::{BufMut, BytesMut};
use futures::{future, ready};
use nflog_sys::*;
use std::os::unix::net::UnixDatagram;
use std::os::unix::prelude::FromRawFd;
use std::ptr::NonNull;
use std::task::{Context, Poll};
use std::{io, mem::MaybeUninit};
use tokio::io::ReadBuf;
use tokio::net::UnixDatagram as TokioUnixDatagram;

use queue_handle::QueueHandle;

pub use message::{L3Protocol, Message, MessageHandler};
pub use nix::sys::socket::AddressFamily;
pub use pnet_base::MacAddr;

const NFLOG_BUF_SIZE: usize = 150000;

#[derive(Clone, Copy)]
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
        const SEQUENCE = NFULNL_CFG_F_SEQ;
        const GLOBAL_SEQUENCE = NFULNL_CFG_F_SEQ_GLOBAL;
    }
}

pub struct QueueConfig {
    pub address_families: Vec<AddressFamily>,
    pub group_num: u16,
    pub buffer_size: usize,
    pub unbind: bool,

    pub copy_mode: Option<CopyMode>,
    pub range: Option<u32>,
    pub flags: Option<Flags>,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            address_families: vec![AddressFamily::Inet, AddressFamily::Inet6],
            group_num: 0,
            buffer_size: NFLOG_BUF_SIZE,
            unbind: false,

            copy_mode: None,
            range: None,
            flags: None,
        }
    }
}

impl QueueConfig {
    pub fn build<H>(self, handler: H) -> io::Result<Queue<H>>
    where
        H: MessageHandler,
    {
        Queue::create(self, handler)
    }
}

pub struct Queue<H> {
    handle: QueueHandle,
    handler: NonNull<H>,
    config: QueueConfig,
}

// Handler is only used in callback, but not in Queue/Socket itself.
// So it's safe to share pointer to handler with callback.
// TODO:
// restrict access to handler inside Queue/Socket at type level
unsafe impl<H> Send for Queue<H> {}

impl<H> Queue<H>
where
    H: MessageHandler,
{
    pub fn create(config: QueueConfig, handler: H) -> io::Result<Self> {
        let mut handle = QueueHandle::open()?;

        for address_family in &config.address_families {
            if config.unbind {
                handle.unbind(*address_family)?;
            }
            handle.bind(*address_family)?;
        }

        handle.bind_group(config.group_num)?;

        let handler = unsafe { NonNull::new_unchecked(Box::into_raw(Box::new(handler))) };

        let mut queue = Self {
            handle,
            config,
            handler,
        };

        if let (Some(mode), Some(range)) = (queue.config.copy_mode, queue.config.range) {
            queue.set_mode(mode, range)?;
        }
        if let Some(flags) = queue.config.flags {
            queue.set_flags(flags)?;
        }

        Ok(queue)
    }

    pub fn config(&self) -> &QueueConfig {
        &self.config
    }

    pub fn set_mode(&mut self, mode: CopyMode, range: u32) -> io::Result<()> {
        self.handle.set_mode(mode, range)?;
        self.config.copy_mode = Some(mode);
        self.config.range = Some(range);

        Ok(())
    }

    pub fn set_flags(&mut self, flags: Flags) -> io::Result<()> {
        self.handle.set_flags(flags)?;
        self.config.flags = Some(flags);

        Ok(())
    }

    pub fn socket(self) -> io::Result<QueueSocket<H>> {
        self.register_callback()?;
        QueueSocket::new(self)
    }

    fn register_callback(&self) -> io::Result<()> {
        let group_handle = self.handle.group_handle()?;
        let handler = self.handler.as_ptr();

        unsafe {
            nflog_callback_register(
                group_handle.as_ptr(),
                Some(callback::<H>),
                handler as *mut _,
            )
        };

        Ok(())
    }
}

pub struct QueueSocket<H> {
    socket: TokioUnixDatagram,
    queue: Queue<H>,
    buffer: BytesMut,
}

impl<H> QueueSocket<H> {
    fn new(queue: Queue<H>) -> io::Result<Self> {
        let fd = queue.handle.fd();
        let socket = unsafe { UnixDatagram::from_raw_fd(fd) };
        let socket = TokioUnixDatagram::from_std(socket)?;

        let buffer = BytesMut::with_capacity(queue.config.buffer_size);

        Ok(Self {
            socket,
            queue,
            buffer,
        })
    }

    pub fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.buffer.clear();

        let n = unsafe {
            let buf = &mut *(self.buffer.chunk_mut() as *mut _ as *mut [MaybeUninit<u8>]);
            let mut read = ReadBuf::uninit(buf);
            let ptr = read.filled().as_ptr();

            ready!(self.socket.poll_recv(cx, &mut read))?;

            assert_eq!(ptr, read.filled().as_ptr());

            let n = read.filled().len();
            self.buffer.advance_mut(n);
            n
        };

        if n > 0 {
            let buf = self.buffer.as_ptr();
            unsafe {
                nflog_handle_packet(
                    self.queue.handle.as_ptr(),
                    buf as *mut libc::c_char,
                    n as libc::c_int,
                );
            };
        }

        Poll::Ready(Ok(()))
    }

    pub async fn recv(&mut self) -> io::Result<()> {
        future::poll_fn(|cx| self.poll_recv(cx)).await
    }

    pub async fn listen(&mut self) -> io::Result<()> {
        loop {
            self.recv().await?;
        }
    }
}

impl<H> Drop for Queue<H> {
    fn drop(&mut self) {
        let _ = unsafe { Box::from_raw(self.handler.as_ptr()) };
    }
}

extern "C" fn callback<H: MessageHandler>(
    _gh: *mut nflog_g_handle,
    nfmsg: *mut nfgenmsg,
    nfd: *mut nflog_data,
    data: *mut std::os::raw::c_void,
) -> libc::c_int {
    if data.is_null() {
        return 1;
    }

    let result = std::panic::catch_unwind(|| {
        if nfmsg.is_null() {
            panic!("nullable nfgenmsg");
        }

        let nfgenmsg = unsafe { &mut *nfmsg };
        let msg = match Message::new(nfgenmsg.nfgen_family, nfd) {
            Ok(msg) => msg,
            Err(e) => panic!("{}", e),
        };
        let mut handler = unsafe { Box::from_raw(data as *mut H) };

        handler.as_mut().handle(msg);

        Box::leak(handler);
    });

    match result {
        Ok(_) => 0,
        Err(_) => 1,
    }
}
