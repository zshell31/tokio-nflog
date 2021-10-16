use nflog_sys::*;

use std::borrow::Cow;
use std::ffi::CStr;
use std::io;
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::slice;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{AddressFamily, MacAddr};

pub trait MessageHandler {
    fn handle(&mut self, msg: Message<'_>);
}

pub type L3Protocol = u16;

#[derive(Debug)]
pub struct Message<'a> {
    nfgen_family: u8,
    inner: NonNull<nflog_data>,
    _lifetime: PhantomData<&'a nflog_data>,
}

impl<'a> Message<'a> {
    pub(crate) fn new(nfgen_family: u8, inner: *mut nflog_data) -> io::Result<Self> {
        Ok(Self {
            nfgen_family,
            inner: NonNull::new(inner)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "nullable nflog_data"))?,
            _lifetime: PhantomData,
        })
    }

    pub fn address_family(&self) -> Option<AddressFamily> {
        AddressFamily::from_i32(self.nfgen_family as i32)
    }

    /// Get the hardware link layer type.
    pub fn hwtype(&self) -> u16 {
        unsafe { nflog_get_hwtype(self.inner.as_ptr()) }
    }

    /// Get the hardware link layer header.
    pub fn packet_hwhdr(&self) -> Option<&'a [u8]> {
        let len = unsafe { nflog_get_msg_packet_hwhdrlen(self.inner.as_ptr()) };
        if len == 0 {
            return None;
        }

        let ptr = unsafe { nflog_get_msg_packet_hwhdr(self.inner.as_ptr()) };
        let data: &[u8] = unsafe { slice::from_raw_parts(ptr as *const _, len as usize) };
        Some(data)
    }

    /// Get the hardware address associated with the given packet.
    ///
    /// For ethernet packets, the hardware address returned (if any) will be
    /// the MAC address of the packet *source* host.
    ///
    /// The destination MAC address is not
    /// known until after POSTROUTING and a successful ARP request, so cannot
    /// currently be retrieved.
    pub fn packet_hwaddr(&self) -> Option<MacAddr> {
        let c_hw = unsafe { nflog_get_packet_hw(self.inner.as_ptr()) };
        if c_hw.is_null() {
            return None;
        }

        let c_len = u16::from_be(unsafe { (*c_hw).hw_addrlen });
        if c_len != 6 {
            return None;
        }

        let addr = unsafe { (*c_hw).hw_addr };
        Some(MacAddr::new(
            addr[0], addr[1], addr[2], addr[3], addr[4], addr[5],
        ))
    }

    /// Returns the layer 3 protocol/EtherType of the packet (i.e. 0x0800 is IPv4).
    pub fn l3_proto(&self) -> L3Protocol {
        let packet_hdr = unsafe { *nflog_get_msg_packet_hdr(self.inner.as_ptr()) };
        u16::from_be(packet_hdr.hw_protocol)
    }

    /// Get the packet mark.
    pub fn nfmark(&self) -> u32 {
        unsafe { nflog_get_nfmark(self.inner.as_ptr()) }
    }

    /// Get the packet timestamp.
    pub fn timestamp(&self) -> Option<SystemTime> {
        let mut tv = libc::timeval {
            tv_sec: 0,
            tv_usec: 0,
        };
        let rc = unsafe { nflog_get_timestamp(self.inner.as_ptr(), &mut tv) };
        if rc != 0 {
            return None;
        }

        let tv = Duration::new(tv.tv_sec as u64, tv.tv_usec as u32 * 1000);

        Some(UNIX_EPOCH + tv)
    }

    /// Get the interface that the packet was received through.
    ///
    /// Returns the index of the device the packet was received via.
    /// If the returned index is 0, the packet was locally generated or the
    /// input interface is not known (ie. `POSTROUTING`?).
    pub fn indev(&self) -> u32 {
        unsafe { nflog_get_indev(self.inner.as_ptr()) }
    }

    /// Get the physical interface that the packet was received through.
    ///
    /// Returns the index of the physical device the packet was received via.
    /// If the returned index is 0, the packet was locally generated or the
    /// physical input interface is no longer known (ie. `POSTROUTING`?).
    pub fn physindev(&self) -> u32 {
        unsafe { nflog_get_physindev(self.inner.as_ptr()) }
    }

    /// Get the interface that the packet will be routed out.
    ///
    /// Returns the index of the device the packet will be sent out.
    /// If the returned index is 0, the packet is destined to localhost or
    /// the output interface is not yet known (ie. `PREROUTING`?).
    pub fn outdev(&self) -> u32 {
        unsafe { nflog_get_outdev(self.inner.as_ptr()) }
    }

    /// Get the physical interface that the packet will be routed out.
    ///
    /// Returns the index of the physical device the packet will be sent out.
    /// If the returned index is 0, the packet is destined to localhost or
    /// the physical output interface is not yet known (ie. `PREROUTING`?).
    pub fn physoutdev(&self) -> u32 {
        unsafe { nflog_get_physoutdev(self.inner.as_ptr()) }
    }

    /// Get the packet payload.
    ///
    /// Depending on set_mode, we may not have a payload
    /// The actual amount and type of data retrieved by this function will
    /// depend on the mode set with the [CopyMode](CopyMode).
    pub fn payload(&self) -> Option<&'a [u8]> {
        let mut c_ptr = std::ptr::null_mut();
        let payload_len = unsafe { nflog_get_payload(self.inner.as_ptr(), &mut c_ptr) };
        if payload_len == 0 {
            return None;
        }

        let payload: &[u8] =
            unsafe { std::slice::from_raw_parts(c_ptr as *const u8, payload_len as usize) };

        Some(payload)
    }

    /// Get the logging string prefix (configured using `--nflog-prefix "..."`
    /// in iptables rules).
    pub fn prefix(&self) -> Cow<'a, str> {
        let c_buf: *const libc::c_char = unsafe { nflog_get_prefix(self.inner.as_ptr()) };
        let cstr = unsafe { CStr::from_ptr(c_buf) };
        cstr.to_string_lossy()
    }

    /// Get the UID of the user that has generated the packet.

    pub fn uid(&self) -> Option<u32> {
        let mut uid = 0;
        let rc = unsafe { nflog_get_uid(self.inner.as_ptr(), &mut uid) };
        match rc {
            0 => Some(uid),
            _ => None,
        }
    }

    /// Get the GID of the user that has generated the packet.
    pub fn gid(&self) -> Option<u32> {
        let mut gid = 0;
        let rc = unsafe { nflog_get_gid(self.inner.as_ptr(), &mut gid) };
        match rc {
            0 => Some(gid),
            _ => None,
        }
    }

    /// Get the local nflog sequence number.
    ///
    /// You must enable this using [Flags::SEQUENCE](Flags::SEQUENCE)
    pub fn local_seqnum(&self) -> Option<u32> {
        let mut seq = 0;
        let rc = unsafe { nflog_get_seq(self.inner.as_ptr(), &mut seq) };
        match rc {
            0 => Some(seq),
            _ => None,
        }
    }

    /// Get the global nflog sequence number.
    ///
    /// You must enable this using [Flags::GLOBAL_SEQUENCE](Flags::GLOBAL_SEQUENCE)
    pub fn global_seqnum(&self) -> Option<u32> {
        let mut seq = 0;
        let rc = unsafe { nflog_get_seq_global(self.inner.as_ptr(), &mut seq) };
        match rc {
            0 => Some(seq),
            _ => None,
        }
    }
}
