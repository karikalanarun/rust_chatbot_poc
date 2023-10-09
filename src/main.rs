use axum::extract::ws::{Message as WsMessage, WebSocket};
use axum::extract::{Json, WebSocketUpgrade};
use axum::response::Response;
use axum::{
    extract::State,
    routing::{get, post},
    Router,
};
use futures_util::{
    sink::SinkExt,
    stream::{SplitSink, SplitStream, StreamExt},
};
use serde::Deserialize;
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::broadcast;
use tokio::sync::RwLock;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug)]
struct Counter {
    pub count: RwLock<i32>,
}

#[derive(Clone, Debug, Deserialize)]
struct Message {
    info: String,
}

struct ChannelContainer {
    tx: broadcast::Sender<Message>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "example_chat=trace".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let counter_state = Arc::new(Counter {
        count: RwLock::new(1),
    });

    let (tx, _rx) = broadcast::channel(100);
    let container = Arc::new(ChannelContainer { tx });

    let app = Router::new()
        .route("/", get(handler))
        .route("/message", post(message))
        .route("/ws", get(ws_handler))
        .with_state(container);

    // Address that server will bind to.
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    // Use `hyper::server::Server` which is re-exported through `axum::Server` to serve the app.
    axum::Server::bind(&addr)
        // Hyper server takes a make service.
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn handler() -> &'static str {
    "Hello, world!"
}

async fn message(
    State(container): State<Arc<ChannelContainer>>,
    Json(payload): Json<Message>,
) -> String {
    container.tx.send(payload).unwrap();
    "Ok".into()
}

async fn ws_handler(
    State(container): State<Arc<ChannelContainer>>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, container))
}

async fn handle_socket(socket: WebSocket, container: Arc<ChannelContainer>) {
    let (sender, receiver) = socket.split();

    tokio::spawn(write(sender, container.tx.subscribe()));
    tokio::spawn(read(receiver));
}

async fn read(mut receiver: SplitStream<WebSocket>) {
    while let Some(Ok(WsMessage::Text(text))) = receiver.next().await {
        // Add username before message.
        println!("User says ::: {}", text);
    }
}

async fn write(mut sender: SplitSink<WebSocket, WsMessage>, mut rx: broadcast::Receiver<Message>) {
    while let Ok(msg) = rx.recv().await {
        // In any websocket error, break loop.
        if sender.send(WsMessage::Text(msg.info)).await.is_err() {
            break;
        }
    }
}
