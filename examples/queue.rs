use std::io;
use tokio_nflog::{AddressFamily, CopyMode, Flags, Message, MessageHandler, QueueConfig};

struct Handler {}

impl MessageHandler for Handler {
    fn handle(&mut self, msg: Message<'_>) {
        let packet = NflogPacket {
            prefix: msg.prefix().to_string(),
        };
        println!("Got {:#?}", packet);
    }
}

#[derive(Debug)]
struct NflogPacket {
    prefix: String,
}

async fn run() -> io::Result<()> {
    let config = QueueConfig {
        address_families: vec![AddressFamily::Inet],
        group_num: 10,
        copy_mode: Some(CopyMode::Packet),
        range: Some(0xffff),
        flags: Some(Flags::SEQUENCE),
        ..Default::default()
    };
    let handler = Handler {};
    let queue = config.build(handler)?;

    println!("Starting nflog listening");

    let mut socket = queue.socket()?;
    socket.listen().await?;

    Ok(())
}

#[tokio::main]
async fn main() -> io::Result<()> {
    run().await
}
