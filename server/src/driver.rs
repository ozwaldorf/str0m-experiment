use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use dashmap::DashMap;
use log::{error, info, trace, warn};
use str0m::channel::ChannelId;
use str0m::net::{Receive, Transmit};
use str0m::{Event, IceConnectionState, Input, Output, Rtc};
use tokio::net::UdpSocket;
use tokio::select;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::Notify;

use triomphe::Arc;

use crate::schema::{HandshakeRequestFrame, RequestFrame};

/// Map of connections. Currently, an ip can only hold a single connection to avoid having to iterate
/// on every incoming packet. A dashmap is used to avoid a lot of locks, and should be performant enough
/// for now.
pub(crate) type ConnectionMap = Arc<DashMap<IpAddr, Connection>>;

/// Driver for our webrtc server.
pub(crate) struct WebRtcDriver {
    /// Underlying UDP socket used for all connections.
    pub(crate) socket: UdpSocket,
    /// Sender for new connections, containing the handshake request, the client address, and a
    /// receiver for incoming payloads.
    conn_tx: Sender<(HandshakeRequestFrame, IpAddr, Receiver<RequestFrame>)>,
    /// Map of connections, holding the raw IO state.
    conns: ConnectionMap,
}

impl WebRtcDriver {
    pub async fn bind(
        udp_addr: SocketAddr,
        conn_tx: Sender<(HandshakeRequestFrame, IpAddr, Receiver<RequestFrame>)>,
        conns: ConnectionMap,
    ) -> Result<Self> {
        // Bind to the udp socket
        let socket = UdpSocket::bind(udp_addr).await?;

        Ok(Self {
            socket,
            conn_tx,
            conns,
        })
    }

    pub async fn run(self, waker: Arc<Notify>) {
        // UDP datagrams should be ~2KB max
        let buf = &mut [0; 2 << 10];

        loop {
            // Remove any disconnected clients.
            self.conns.retain(|_, c| c.rtc.is_alive());

            // Poll client states until they all are at an idle state.
            // TODO: Avoid re-polling idle clients

            let mut timeouts = Vec::with_capacity(self.conns.len());
            for mut client in self.conns.iter_mut() {
                match client
                    .handle_until_timeout(&self.socket, &self.conn_tx)
                    .await
                {
                    Ok(t) => timeouts.push(t),
                    Err(e) => error!("failed to handle rtc connection state: {e}"),
                }
            }

            let timeout = timeouts
                .into_iter()
                .min()
                .map(|t| t - Instant::now())
                .unwrap_or(Duration::from_millis(30000))
                .max(Duration::from_millis(1));

            let now = Instant::now();
            select! {
                // Read incoming data from the socket
                res = self.socket.recv_from(buf) => match res {
                    Ok((n, source)) => {
                        info!("read in {:?}", now.elapsed());
                        if let Err(e) = self.handle_input(source, &buf[..n]) {
                            error!("failed to handle socket input: {e}");
                        }
                    }
                    Err(_) => {}
                },
                // Waker, which is called after rtc.write()
                _ = waker.notified() => {},
                // Read timeout
                _ = tokio::time::sleep(timeout) => {}
            }

            // Drive time forward in all clients.
            let now = Instant::now();
            for mut conn in self.conns.iter_mut() {
                if let Err(e) = conn.rtc.handle_input(Input::Timeout(now)) {
                    warn!("failed to advance time for rtc instance: {e}");
                }
            }
        }
    }

    fn handle_input(&self, source: SocketAddr, buffer: &[u8]) -> Result<()> {
        // Try and get the rtc instance for this address, and handle its input
        if let Some(mut client) = self.conns.get_mut(&source.ip()) {
            info!("handling input");
            if let Err(e) = client.rtc.handle_input(Input::Receive(
                Instant::now(),
                Receive {
                    source,
                    destination: self.socket.local_addr().unwrap(),
                    contents: buffer.try_into().unwrap(),
                },
            )) {
                warn!("failed to handle client input: {e}");
            };
        };

        Ok(())
    }
}

/// Connection object holding the rtc instance and the handshake state
pub struct Connection {
    rtc: Rtc,
    addr: IpAddr,
    state: ConnectionState,
}

enum ConnectionState {
    AwaitingDataChannel,
    AwaitingHandshake(ChannelId),
    AwaitingRequest(ChannelId, Sender<RequestFrame>),
    Disconnected,
}

impl Connection {
    pub fn new(rtc: Rtc, addr: IpAddr) -> Self {
        Self {
            rtc,
            addr,
            state: ConnectionState::AwaitingDataChannel,
        }
    }

    async fn handle_until_timeout(
        &mut self,
        socket: &UdpSocket,
        conn_tx: &Sender<(HandshakeRequestFrame, IpAddr, Receiver<RequestFrame>)>,
    ) -> Result<Instant> {
        loop {
            match self.handle_output(socket, conn_tx).await? {
                Some(t) => return Ok(t),
                None => continue,
            }
        }
    }

    /// Poll and handle the output from the rtc state. On error, the connection should be
    /// considered disconnected. If the state is idle, returns an instant when the connection will
    /// time out.
    /// TODO: Support multiple data channels as individual connections at the transport level
    async fn handle_output(
        &mut self,
        socket: &UdpSocket,
        conn_tx: &Sender<(HandshakeRequestFrame, IpAddr, Receiver<RequestFrame>)>,
    ) -> Result<Option<Instant>> {
        let time = Instant::now();
        match self.rtc.poll_output()? {
            Output::Timeout(t) => {
                // The rtc is at an idle state
                return Ok(Some(t));
            }
            Output::Transmit(Transmit {
                destination,
                contents,
                ..
            }) => {
                let payload = contents.as_ref();
                // We have some data to send on the socket.

                let len = payload.len();
                assert_eq!(len, socket.send_to(payload, destination).await?);
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_micros();
                info!("sent {len} bytes at {now}");
            }
            Output::Event(event) => match event {
                Event::IceConnectionStateChange(IceConnectionState::Disconnected) => {
                    self.state = ConnectionState::Disconnected
                }
                Event::ChannelOpen(id, label) => {
                    if let ConnectionState::AwaitingDataChannel = self.state {
                        trace!("Client opened channel {label}");
                        // TODO: Send challenge key for client proof of possession

                        // Tick the handshake state forward
                        self.state = ConnectionState::AwaitingHandshake(id);
                    }
                }
                Event::ChannelData(msg) => {
                    // Handle incoming payload
                    if msg.binary {
                        match &self.state {
                            ConnectionState::AwaitingDataChannel => {
                                unreachable!(
                                    "should never receive channel data before a channel has opened"
                                )
                            }
                            ConnectionState::AwaitingHandshake(id) => {
                                // Decode the frame
                                let frame = HandshakeRequestFrame::decode(&msg.data)?;

                                // Send the new connection to the transport
                                let (tx, rx) = channel(256);
                                conn_tx.send((frame, self.addr, rx)).await?;

                                // Tick the handshake state forward
                                self.state = ConnectionState::AwaitingRequest(*id, tx);
                            }
                            ConnectionState::AwaitingRequest(_, sender) => {
                                println!("read request frame");

                                let frame = RequestFrame::decode(&msg.data)?;
                                sender.send(frame).await?;
                                let now = SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_micros();
                                info!("sent frame to receiver at {now}");
                            }
                            ConnectionState::Disconnected => {}
                        }
                    }
                }
                Event::ChannelClose(id) => {
                    if let ConnectionState::AwaitingHandshake(current_id)
                    | ConnectionState::AwaitingRequest(current_id, _) = self.state
                    {
                        if id == current_id {
                            self.state = ConnectionState::Disconnected;
                        }
                    }
                }
                // We don't care much about the other events for now
                _ => {}
            },
        }

        info!("handled event in {:?}", time.elapsed());

        Ok(None)
    }

    /// Write a payload to the rtc instance to be processed.
    /// Should only be called with payloads < 64KB
    pub(crate) fn write(&mut self, payload: &[u8]) -> Result<()> {
        match self.state {
            ConnectionState::AwaitingDataChannel => Err(anyhow!(
                "data channel has not been opened by the client yet"
            )),
            ConnectionState::AwaitingHandshake(id) | ConnectionState::AwaitingRequest(id, _) => {
                let mut channel = self
                    .rtc
                    .channel(id)
                    .ok_or(anyhow!("failed to get channel writer"))?;

                // Assert that we've written all of the payload to the rtc state.
                // We *should* only get an error if this method was called with a
                // payload that was too large.
                assert_eq!(payload.len(), channel.write(true, payload)?);

                Ok(())
            }
            ConnectionState::Disconnected => Err(anyhow!("connection state is disconnected")),
        }
    }
}
