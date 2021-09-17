use nflog::{CopyMode, Flags, Message, Queue};
use pnet_packet::ipv4::{Ipv4, Ipv4Packet};
use pnet_packet::FromPacket;
use serde::Serialize;
use std::net::Ipv4Addr;

#[derive(Serialize)]
struct Ipv4PacketInfo {
    #[serde(rename = "src_ip")]
    src: Ipv4Addr,
    #[serde(rename = "dest_ip")]
    dst: Ipv4Addr,
    #[serde(rename = "ip.protocol")]
    protocol: u8,
    #[serde(rename = "ip.ttl")]
    ttl: u8,
    #[serde(rename = "ip.totlen")]
    totlen: u16,
}

impl From<Ipv4> for Ipv4PacketInfo {
    fn from(packet: Ipv4) -> Self {
        Ipv4PacketInfo {
            src: packet.source,
            dst: packet.destination,
            protocol: packet.next_level_protocol.0,
            ttl: packet.ttl,
            totlen: packet.total_length,
        }
    }
}

#[derive(Serialize)]
struct PacketInfo {
    #[serde(flatten)]
    ipv4: Option<Ipv4PacketInfo>,
}

fn parse_packet(payload: &[u8]) -> anyhow::Result<PacketInfo> {
    let ipv4 = Ipv4Packet::new(payload)
        .ok_or_else(|| anyhow::anyhow!("Cannot parse payload as ipv4 packet"))?;
    let ipv4: Ipv4PacketInfo = ipv4.from_packet().into();

    Ok(PacketInfo { ipv4: Some(ipv4) })
}

#[derive(Serialize)]
struct FullPacketInfo<'a> {
    #[serde(rename = "oob.prefix")]
    prefix: &'a str,
    #[serde(rename = "oob.mark")]
    mark: u32,
    #[serde(rename = "oob.protocol")]
    protocol: u16,
    #[serde(flatten)]
    packet_info: PacketInfo,
}

fn log_callback(msg: Message) {
    let payload = msg.payload();
    let packet_info = parse_packet(payload);
    match packet_info {
        Ok(packet_info) => {
            let full = FullPacketInfo {
                prefix: &*msg.prefix(),
                mark: msg.nfmark(),
                protocol: msg.l3_proto(),
                packet_info,
            };
            println!("{}", serde_json::to_string_pretty(&full).unwrap());
        }
        Err(e) => {
            println!("err: {}", e);
        }
    };
}

fn main() {
    let queue = Queue::open().unwrap();
    let _ = queue.unbind(libc::AF_INET);
    let mut group = queue.bind_group(0).unwrap();
    group.set_mode(CopyMode::Packet, 0xffff);
    group.set_flags(Flags::Sequence);
    group.set_callback(log_callback);
    queue.run_loop();
}
