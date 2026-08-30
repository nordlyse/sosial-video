use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Arc,
    time::Duration,
};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};
use webrtc::{
    api::{
        interceptor_registry::register_default_interceptors,
        media_engine::MediaEngine,
        setting_engine::SettingEngine,
        APIBuilder,
    },
    ice::udp_network::{EphemeralUDP, UDPNetwork},
    ice_transport::{
        ice_candidate_type::RTCIceCandidateType,
        ice_server::RTCIceServer,
    },
    interceptor::registry::Registry,
    peer_connection::{
        configuration::RTCConfiguration,
        peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription,
        RTCPeerConnection,
    },
    rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication,
    rtp_transceiver::rtp_codec::RTPCodecType,
    track::track_local::{
        track_local_static_rtp::TrackLocalStaticRTP, TrackLocal, TrackLocalWriter,
    },
    Error,
};

#[derive(Clone, Default)]
struct Publisher {
    video: Option<Arc<TrackLocalStaticRTP>>,
    audio: Option<Arc<TrackLocalStaticRTP>>,
}

#[derive(Clone, Default)]
struct Room {
    publishers: HashMap<String, Publisher>,
}

#[derive(Clone)]
struct AppState {
    rooms: Arc<Mutex<HashMap<String, Room>>>,
    udp_port_min: u16,
    udp_port_max: u16,
    announced_ip: Option<String>,
}

#[derive(Deserialize)]
struct SdpRequest {
    #[serde(rename = "type")]
    sdp_type: String,
    sdp: String,
    peer_id: Option<String>,
}

#[derive(Serialize)]
struct SdpResponse {
    #[serde(rename = "type")]
    sdp_type: String,
    sdp: String,
}

#[derive(Serialize)]
struct PublisherStatus {
    peer_id: String,
    has_video: bool,
    has_audio: bool,
}

#[derive(Serialize)]
struct RoomStatus {
    room: String,
    publishers: Vec<PublisherStatus>,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let state = AppState {
        rooms: Arc::new(Mutex::new(HashMap::new())),
        udp_port_min: env_u16("WEBRTC_UDP_PORT_MIN", 40000),
        udp_port_max: env_u16("WEBRTC_UDP_PORT_MAX", 40031),
        announced_ip: std::env::var("WEBRTC_ANNOUNCED_IP")
            .ok()
            .filter(|s| !s.is_empty()),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/rooms/{room}", get(room_status))
        .route("/v1/rooms/{room}/publish", post(publish))
        .route(
            "/v1/rooms/{room}/peers/{peer_id}/subscribe",
            post(subscribe),
        )
        .with_state(state)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    let listen: SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8082".into())
        .parse()?;
    tracing::info!("webrtc-service SFU listening on {listen}");
    let listener = tokio::net::TcpListener::bind(listen).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn env_u16(name: &str, default: u16) -> u16 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

async fn health() -> &'static str {
    "ok"
}

async fn room_status(State(state): State<AppState>, Path(room): Path<String>) -> Json<RoomStatus> {
    let rooms = state.rooms.lock().await;
    let publishers = rooms
        .get(&room)
        .map(|current| {
            current
                .publishers
                .iter()
                .map(|(peer_id, publisher)| PublisherStatus {
                    peer_id: peer_id.clone(),
                    has_video: publisher.video.is_some(),
                    has_audio: publisher.audio.is_some(),
                })
                .collect()
        })
        .unwrap_or_default();
    Json(RoomStatus { room, publishers })
}

async fn publish(
    State(state): State<AppState>,
    Path(room): Path<String>,
    Json(body): Json<SdpRequest>,
) -> Result<Json<SdpResponse>, (StatusCode, Json<ErrorBody>)> {
    let peer_id = body
        .peer_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| error(StatusCode::BAD_REQUEST, "peer_id is required"))?
        .to_string();
    let offer = offer_from_body(&body)?;
    let peer = new_peer_connection(&state).await.map_err(internal)?;

    peer.add_transceiver_from_kind(RTPCodecType::Video, None)
        .await
        .map_err(internal)?;
    peer.add_transceiver_from_kind(RTPCodecType::Audio, None)
        .await
        .map_err(internal)?;

    {
        let mut rooms = state.rooms.lock().await;
        rooms
            .entry(room.clone())
            .or_default()
            .publishers
            .entry(peer_id.clone())
            .or_default();
    }

    let rooms = state.rooms.clone();
    let room_name = room.clone();
    let publisher_id = peer_id.clone();
    let pc = Arc::downgrade(&peer);
    peer.on_track(Box::new(move |track, _, _| {
        let media_ssrc = track.ssrc();
        let pc2 = pc.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(3)).await;
                let Some(pc) = pc2.upgrade() else {
                    break;
                };
                if pc
                    .write_rtcp(&[Box::new(PictureLossIndication {
                        sender_ssrc: 0,
                        media_ssrc,
                    })])
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        let rooms = rooms.clone();
        let room_name = room_name.clone();
        let publisher_id = publisher_id.clone();
        tokio::spawn(async move {
            let kind = track.kind();
            let track_id = if kind == RTPCodecType::Video {
                "video"
            } else {
                "audio"
            };
            let local_track = Arc::new(TrackLocalStaticRTP::new(
                track.codec().capability,
                format!("{publisher_id}-{track_id}"),
                room_name.clone(),
            ));
            {
                let mut rooms = rooms.lock().await;
                let publisher = rooms
                    .entry(room_name.clone())
                    .or_default()
                    .publishers
                    .entry(publisher_id.clone())
                    .or_default();
                if kind == RTPCodecType::Video {
                    publisher.video = Some(Arc::clone(&local_track));
                } else if kind == RTPCodecType::Audio {
                    publisher.audio = Some(Arc::clone(&local_track));
                }
                tracing::info!("publisher {publisher_id} {track_id} ready in room {room_name}");
            }

            while let Ok((rtp, _)) = track.read_rtp().await {
                if let Err(err) = local_track.write_rtp(&rtp).await {
                    if !matches!(err, Error::ErrClosedPipe) {
                        tracing::warn!("rtp write error: {err}");
                    }
                    break;
                }
            }

            let mut rooms = rooms.lock().await;
            if let Some(room) = rooms.get_mut(&room_name) {
                if let Some(publisher) = room.publishers.get_mut(&publisher_id) {
                    if kind == RTPCodecType::Video {
                        publisher.video = None;
                    } else if kind == RTPCodecType::Audio {
                        publisher.audio = None;
                    }
                }
            }
        });

        Box::pin(async {})
    }));

    let rooms = state.rooms.clone();
    let room_name = room.clone();
    let publisher_id = peer_id.clone();
    peer.on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
        tracing::info!("publisher {publisher_id} state: {s}");
        if matches!(
            s,
            RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed
        ) {
            let rooms = rooms.clone();
            let room_name = room_name.clone();
            let publisher_id = publisher_id.clone();
            tokio::spawn(async move {
                let mut rooms = rooms.lock().await;
                if let Some(room) = rooms.get_mut(&room_name) {
                    room.publishers.remove(&publisher_id);
                }
            });
        }
        Box::pin(async {})
    }));

    answer_offer(peer, offer).await
}

async fn subscribe(
    State(state): State<AppState>,
    Path((room, peer_id)): Path<(String, String)>,
    Json(body): Json<SdpRequest>,
) -> Result<Json<SdpResponse>, (StatusCode, Json<ErrorBody>)> {
    let offer = offer_from_body(&body)?;
    let publisher = {
        let rooms = state.rooms.lock().await;
        rooms
            .get(&room)
            .and_then(|current| current.publishers.get(&peer_id).cloned())
            .unwrap_or_default()
    };

    let peer = new_peer_connection(&state).await.map_err(internal)?;
    add_local_track(&peer, publisher.video).await?;
    add_local_track(&peer, publisher.audio).await?;

    peer.on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
        tracing::info!("subscriber for {peer_id} state: {s}");
        Box::pin(async {})
    }));

    answer_offer(peer, offer).await
}

async fn add_local_track(
    peer: &Arc<RTCPeerConnection>,
    track: Option<Arc<TrackLocalStaticRTP>>,
) -> Result<(), (StatusCode, Json<ErrorBody>)> {
    let Some(track) = track else {
        return Ok(());
    };
    let sender = peer
        .add_track(track as Arc<dyn TrackLocal + Send + Sync>)
        .await
        .map_err(internal)?;
    tokio::spawn(async move {
        let mut buf = vec![0u8; 1500];
        while sender.read(&mut buf).await.is_ok() {}
    });
    Ok(())
}

async fn new_peer_connection(state: &AppState) -> anyhow::Result<Arc<RTCPeerConnection>> {
    let mut media = MediaEngine::default();
    media.register_default_codecs()?;

    let mut registry = Registry::new();
    registry = register_default_interceptors(registry, &mut media)?;

    let mut settings = SettingEngine::default();
    settings.set_udp_network(UDPNetwork::Ephemeral(EphemeralUDP::new(
        state.udp_port_min,
        state.udp_port_max,
    )?));
    settings.set_include_loopback_candidate(true);
    if let Some(ip) = &state.announced_ip {
        settings.set_nat_1to1_ips(vec![ip.clone()], RTCIceCandidateType::Host);
    }

    let api = APIBuilder::new()
        .with_media_engine(media)
        .with_interceptor_registry(registry)
        .with_setting_engine(settings)
        .build();

    let config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_owned()],
            ..Default::default()
        }],
        ..Default::default()
    };
    Ok(Arc::new(api.new_peer_connection(config).await?))
}

async fn answer_offer(
    peer: Arc<RTCPeerConnection>,
    offer: RTCSessionDescription,
) -> Result<Json<SdpResponse>, (StatusCode, Json<ErrorBody>)> {
    peer.set_remote_description(offer).await.map_err(internal)?;
    let answer = peer.create_answer(None).await.map_err(internal)?;
    let mut gather_complete = peer.gathering_complete_promise().await;
    peer.set_local_description(answer).await.map_err(internal)?;
    let _ = gather_complete.recv().await;

    let local = peer
        .local_description()
        .await
        .ok_or_else(|| error(StatusCode::INTERNAL_SERVER_ERROR, "missing local description"))?;
    Ok(Json(SdpResponse {
        sdp_type: "answer".to_string(),
        sdp: local.sdp,
    }))
}

fn offer_from_body(body: &SdpRequest) -> Result<RTCSessionDescription, (StatusCode, Json<ErrorBody>)> {
    if body.sdp_type != "offer" {
        return Err(error(StatusCode::BAD_REQUEST, "expected SDP offer"));
    }
    RTCSessionDescription::offer(body.sdp.clone()).map_err(internal)
}

fn error(status: StatusCode, message: &str) -> (StatusCode, Json<ErrorBody>) {
    (
        status,
        Json(ErrorBody {
            error: message.to_string(),
        }),
    )
}

fn internal<E: std::fmt::Display>(err: E) -> (StatusCode, Json<ErrorBody>) {
    tracing::error!("{err}");
    error(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
}
