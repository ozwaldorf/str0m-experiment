#![allow(unused)]

mod driver;
mod schema;
mod signal;

use arrayref::array_ref;
use bytes::{BufMut, Bytes, BytesMut};
use driver::ConnectionMap;
use log::{info, warn};
use schema::HandshakeRequestFrame;
use std::net::IpAddr;
use triomphe::Arc;

use anyhow::Result;
use dashmap::DashMap;
use tokio::sync::mpsc::{channel, Receiver};

use crate::{driver::WebRtcDriver, signal::start_signaling_server};

const HTTP_PORT: u16 = 4210;
const UDP_PORT: u16 = 4211;
const WEBRTC_MAX_LEN: usize = 32 << 10;
const BLOCKSIZE: usize = 256 << 10;
const VIDEO_EXAMPLE: &[u8] = include_bytes!("../frag_bunny.mp4");
const BLOCK_COUNT_PAYLOAD: &[u8] = &(VIDEO_EXAMPLE.len().div_ceil(BLOCKSIZE) as u32).to_be_bytes();

#[tokio::main]
async fn main() -> Result<()> {
    simple_logger::init_with_env()?;

    log::info!("Binding WebRTC transport");
    let conns = Arc::new(DashMap::new());

    // A bounded channel is used to provide some back pressure for incoming client handshakes.
    let (conn_tx, mut conn_rx) = channel(256);

    // Spawn the driver IO loop
    let driver =
        WebRtcDriver::bind(([127, 0, 0, 1], UDP_PORT).into(), conn_tx, conns.clone()).await?;
    let local_addr = driver.socket.local_addr()?;
    warn!("PEER ADDR: {local_addr}");

    // Spawn a HTTP server for accepting incoming SDP requests.
    let conns2 = conns.clone();
    tokio::spawn(async move {
        start_signaling_server(
            ([127, 0, 0, 1], UDP_PORT).into(),
            ([127, 0, 0, 1], UDP_PORT).into(),
            conns2,
        )
        .await
        .expect("Failed to setup server");
    });

    // Spawn a task for the driver
    tokio::spawn(driver.run());

    loop {
        // accept incoming connection
        let (req, addr, receiver) = conn_rx
            .recv()
            .await
            .expect("failed to recv next connection");

        let sender = WebRtcSender {
            addr,
            conns: conns.clone(),
        };
        let receiver = WebRtcReceiver(receiver);

        // Spawn a task for handling the connection
        tokio::spawn(async move { handle_conn(req, sender, receiver).await });
    }
}

/// Super simple example which sends to the client in chunks, on request
async fn handle_conn(
    _req: HandshakeRequestFrame,
    mut sender: WebRtcSender,
    mut receiver: WebRtcReceiver,
) {
    // send the first frame containing the number of blocks
    sender.send(schema::ResponseFrame::ServicePayload {
        bytes: BLOCK_COUNT_PAYLOAD.into(),
    });

    // Read a request frame
    while let Some(frame) = receiver.recv().await {
        match frame {
            schema::RequestFrame::ServicePayload { bytes } => {
                // we send blocks of 256KB, in 8 chunks of 32KB
                let offset =
                    u32::from_be_bytes(*array_ref!(bytes, 0, 4)) as usize * 8 * WEBRTC_MAX_LEN;

                // send 8 chunks for this block
                for i in 0..8 {
                    let chunk = &VIDEO_EXAMPLE
                        [offset + i * WEBRTC_MAX_LEN..offset + (i + 1) * WEBRTC_MAX_LEN];
                    sender.send(schema::ResponseFrame::ServicePayload {
                        bytes: chunk.into(),
                    });
                }
            }
            schema::RequestFrame::AccessToken { .. } => todo!(),
            schema::RequestFrame::ExtendAccessToken { .. } => todo!(),
            schema::RequestFrame::DeliveryAcknowledgment {} => todo!(),
        }
    }
}

/// Sender for a webrtc connection.
pub struct WebRtcSender {
    addr: IpAddr,
    // TODO: See if a channel sending to the driver loop is more performant than grabbing the
    // instance directly from the dashmap.
    conns: ConnectionMap,
}

macro_rules! webrtc_send {
    ($self:expr, $payload:expr) => {
        if let Some(mut conn) = $self.conns.get_mut(&$self.addr) {
            if let Err(e) = conn.write($payload) {
                warn!("failed to write outgoing payload to rtc instance: {e}");
            }
        }
    };
}

impl WebRtcSender {
    fn send_handshake_response(&mut self, frame: schema::HandshakeResponse) {
        webrtc_send!(self, &frame.encode());
    }

    fn send(&mut self, frame: schema::ResponseFrame) {
        match frame {
            // Chunk outgoing payloads larger than the webrtc max
            schema::ResponseFrame::ServicePayload { bytes } if bytes.len() > WEBRTC_MAX_LEN => {
                info!("sending chunked payload");
                let mut iter = bytes.chunks(WEBRTC_MAX_LEN).peekable();
                while let Some(chunk) = iter.next() {
                    let frame = schema::ResponseFrame::ServicePayload {
                        bytes: chunk.to_vec().into(),
                    };
                    let mut bytes = frame.encode().to_vec();

                    if iter.peek().is_some() {
                        // Set the frame tag to 0x40, signaling that it is a partial chunk of the
                        // service payload, and should be buffered and concatenated with
                        // the next chunk. The last chunk will keep the original frame tag (0x00),
                        // signaling that the payload is complete, which also allows single
                        // chunk payloads to use the normal frame tag.
                        bytes[0] = 0x40;
                    }

                    // Send the chunk
                    webrtc_send!(self, &frame.encode());
                }
            }
            frame => {
                info!("sending complete frame");
                // Other frames will never be larger than the max len, so we just send them.
                webrtc_send!(self, &frame.encode());
            }
        }
    }
}

/// Receiver for a webrtc connection.
pub struct WebRtcReceiver(Receiver<schema::RequestFrame>);

impl WebRtcReceiver {
    async fn recv(&mut self) -> Option<schema::RequestFrame> {
        self.0.recv().await
    }
}
