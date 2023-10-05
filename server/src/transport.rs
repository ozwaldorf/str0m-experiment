use arrayref::array_ref;
use bytes::{BufMut, Bytes, BytesMut};

use log::{info, warn};

use std::net::{IpAddr, SocketAddr};
use triomphe::Arc;

use anyhow::Result;
use dashmap::DashMap;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use crate::{
    driver::{ConnectionMap, WebRtcDriver},
    schema::{self, HandshakeRequestFrame, RequestFrame},
    signal::start_signaling_server,
    WEBRTC_MAX_LEN,
};

/// A WebRTC Transport. Spawns a HTTP signaling server, and binds to a udp port.
pub struct WebRtcTransport {
    /// Receiver for new incoming DataChannels from peer connections.
    conn_rx: Receiver<(HandshakeRequestFrame, IpAddr, Receiver<RequestFrame>)>,
    /// Shared map of connections
    conns: ConnectionMap,
}

impl WebRtcTransport {
    pub async fn bind(http_addr: SocketAddr, udp_addr: SocketAddr) -> anyhow::Result<Self> {
        log::info!("Binding WebRTC transport on {http_addr} (HTTP) and {udp_addr} (UDP)");
        let conns = Arc::new(DashMap::new());

        // A bounded channel is used to provide some back pressure for incoming client handshakes.
        let (conn_tx, conn_rx) = channel(256);

        // Spawn the driver IO loop
        let driver = WebRtcDriver::bind(udp_addr, conn_tx, conns.clone()).await?;

        // Spawn a HTTP server for accepting incoming SDP requests.
        let conns2 = conns.clone();
        tokio::spawn(async move {
            start_signaling_server(http_addr, udp_addr, conns2)
                .await
                .expect("Failed to setup server");
        });

        tokio::spawn(driver.run());

        Ok(Self { conn_rx, conns })
    }

    pub async fn accept(
        &mut self,
    ) -> Option<(schema::HandshakeRequestFrame, WebRtcSender, WebRtcReceiver)> {
        let (req, addr, receiver) = self.conn_rx.recv().await?;

        let sender = WebRtcSender {
            addr,
            conns: self.conns.clone(),
        };
        let receiver = WebRtcReceiver(receiver);

        Some((req, sender, receiver))
    }
}

/// Sender for a webrtc connection.
pub struct WebRtcSender {
    addr: IpAddr,
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
    pub fn send_handshake_response(&mut self, frame: schema::HandshakeResponse) {
        webrtc_send!(self, &frame.encode());
    }

    pub fn send(&self, frame: schema::ResponseFrame) {
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
    pub async fn recv(&mut self) -> Option<schema::RequestFrame> {
        self.0.recv().await
    }
}
