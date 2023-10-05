use std::net::SocketAddr;

use anyhow::Result;
use axum::extract::{ConnectInfo, State};
use axum::routing::post;
use axum::{Json, Router};
use str0m::change::{SdpAnswer, SdpOffer};
use str0m::{Candidate, Rtc, RtcConfig};
use tower_http::cors::CorsLayer;
use triomphe::Arc;

use super::driver::{Connection, ConnectionMap};
struct SignalState {
    // Map of rtc connection states.
    client_map: ConnectionMap,
    host: Candidate,
}
pub async fn start_signaling_server(
    server_addr: SocketAddr,
    udp_addr: SocketAddr,
    client_map: ConnectionMap,
) -> Result<()> {
    let app = Router::new()
        .route("/sdp", post(handler))
        .layer(CorsLayer::permissive())
        .with_state(Arc::new(SignalState {
            client_map,
            host: Candidate::host(udp_addr)?,
        }));

    axum::Server::bind(&server_addr)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await?;

    Ok(())
}

async fn handler(
    State(state): State<Arc<SignalState>>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    Json(offer): Json<SdpOffer>,
) -> Result<Json<SdpAnswer>, String> {
    let mut rtc = RtcConfig::new().set_ice_lite(true).build();
    rtc.add_local_candidate(state.host.clone());
    let answer = rtc
        .sdp_api()
        .accept_offer(offer)
        .map_err(|e| e.to_string())?;

    let ip = peer_addr.ip();
    state.client_map.insert(ip, Connection::new(rtc, ip));

    Ok(Json(answer))
}
