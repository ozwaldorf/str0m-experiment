#![allow(unused)]

mod driver;
mod schema;
mod signal;
mod transport;

use arrayref::array_ref;
use bytes::{BufMut, Bytes, BytesMut};
use driver::ConnectionMap;
use log::{info, warn};
use schema::HandshakeRequestFrame;
use std::net::IpAddr;
use transport::{WebRtcReceiver, WebRtcSender};
use triomphe::Arc;

use anyhow::Result;
use dashmap::DashMap;
use tokio::sync::mpsc::{channel, Receiver};

use crate::{driver::WebRtcDriver, signal::start_signaling_server, transport::WebRtcTransport};

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
    let mut transport = WebRtcTransport::bind(
        ([127, 0, 0, 1], HTTP_PORT).into(),
        ([127, 0, 0, 1], UDP_PORT).into(),
    )
    .await?;

    // Accept an incoming connection
    while let Some((req, sender, receiver)) = transport.accept().await {
        // Spawn a task for handling the connection
        tokio::spawn(async move { handle_conn(req, sender, receiver).await });
    }

    Ok(())
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
                let offset = u32::from_be_bytes(*array_ref!(bytes, 0, 4)) as usize * BLOCKSIZE;

                // send 8 chunks for this block
                for i in 0..8 {
                    let chunk = &VIDEO_EXAMPLE
                        [offset + (i * WEBRTC_MAX_LEN)..offset + ((i + 1) * WEBRTC_MAX_LEN)];
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
