use axum::extract::ws::{Message as WsMessage, WebSocket};
use axum::extract::{Json, WebSocketUpgrade};
use axum::response::Response;
use axum::{
    extract::State,
    routing::{get, post},
    Router,
};
use futures_util::future::join_all;
use futures_util::{
    sink::SinkExt,
    stream::{SplitSink, SplitStream, StreamExt},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::{broadcast, Mutex};
use tokio::sync::{mpsc, RwLock};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug)]
struct Counter {
    pub count: RwLock<i32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Message {
    info: String,
}

// enum SocketEvent<'a, T: Deserialize<'a> + Serialize> {
//     Connected,
//     Disconnected,
//     Message(T),
// }

#[derive(Clone)]
struct Channel {
    id: String,
    tx: mpsc::Sender<Message>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
enum RoomJoinErr {
    ChannelNotFound,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
enum RoomSendErr {
    RoomNotFound,
}

struct ChannelManager {
    rooms: RwLock<HashMap<String, HashMap<String, Channel>>>,
    channels: Mutex<HashMap<String, Channel>>,
}

impl ChannelManager {
    fn new() -> Self {
        Self {
            rooms: RwLock::new(HashMap::new()),
            channels: Mutex::new(HashMap::new()),
        }
    }

    async fn create_channel(&self, id: String, socket: WebSocket) -> mpsc::Receiver<Message> {
        let (ws_tx, ws_rx) = mpsc::channel(100);
        let (tx, mut rx) = mpsc::channel(100);
        let (mut sender, mut receiver) = socket.split();
        let read_handle = tokio::spawn(async move {
            while let Some(Ok(WsMessage::Text(text))) = receiver.next().await {
                if ws_tx
                    .send(serde_json::from_str(&text).unwrap())
                    .await
                    .is_err()
                {
                    // client disconnected
                    return;
                }
            }
        });
        let mut channels = self.channels.lock().await;
        channels.insert(id.clone(), Channel { id: id.clone(), tx });
        let write_handle = tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if sender
                    .send(WsMessage::Text(serde_json::to_string(&msg).unwrap()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });
        // TODO: clean up with tokio_select!
        // either reading from channel ends or writing ends, have to close all channel
        // need to remove the channel from all rooms
        // need to destroy channel with given id
        ws_rx
    }

    fn drop(channel_id: String) {
        unimplemented!()
    }

    fn message(self, channel_id: String, msg: Message) {
        unimplemented!()
    }

    async fn message_to_room(&self, room_id: &String, msg: Message) -> Result<(), RoomSendErr> {
        let room = self.rooms.read().await;
        let room = room.get(room_id).ok_or(RoomSendErr::RoomNotFound)?;
        let task_results: Result<Vec<()>, _> =
            join_all(room.iter().map(|(_, channel)| channel.tx.send(msg.clone())))
                .await
                .into_iter()
                .collect();
        task_results.unwrap(); // TODO: remove this unwrap, write proper error message
        Ok(())
    }

    async fn join_room(&self, channel_id: &String, room_id: &String) -> Result<(), RoomJoinErr> {
        let channel = self.channels.lock().await;
        let channel = channel
            .get(channel_id)
            .ok_or(RoomJoinErr::ChannelNotFound)?;
        let mut rooms = self.rooms.write().await;
        let room = rooms.get_mut(room_id);
        // let room = room.as_mut();
        match room {
            Some(room) => {
                room.insert(channel_id.into(), channel.clone());
            }
            None => {
                let mut room = HashMap::new();
                room.insert(channel_id.into(), channel.clone());
                rooms.insert(room_id.into(), room);
            }
        };
        Ok(())
    }

    fn remove_from_room(channel_id: String, room_id: String) {
        unimplemented!()
    }
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

    // let (tx, _rx) = broadcast::channel(100);
    let container = Arc::new(ChannelManager::new());

    let app = Router::new()
        .route("/", get(handler))
        .route("/message", post(message))
        .route("/join_room", post(join_to_room))
        .route("/message_to_room", post(message_to_room))
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
    State(container): State<Arc<ChannelManager>>,
    Json(payload): Json<Message>,
) -> String {
    // container.tx.send(payload).unwrap();
    // "Ok".into()
    unimplemented!()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct MessageToRoom {
    room_id: String,
    message: Message,
}

async fn message_to_room(
    State(container): State<Arc<ChannelManager>>,
    Json(MessageToRoom { room_id, message }): Json<MessageToRoom>,
) -> String {
    container.message_to_room(&room_id, message).await.unwrap();
    "Ok".into()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct JoinRoomDetails {
    room_id: String,
    channel_id: String,
}

async fn join_to_room(
    State(container): State<Arc<ChannelManager>>,
    Json(JoinRoomDetails {
        channel_id,
        room_id,
    }): Json<JoinRoomDetails>,
) -> String {
    container.join_room(&channel_id, &room_id).await.unwrap();
    // Ok("Ok".into())
    "Ok".into()
}

async fn ws_handler(
    State(container): State<Arc<ChannelManager>>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, container))
}

async fn handle_socket(socket: WebSocket, container: Arc<ChannelManager>) {
    let id = "arun".into();
    let mut rx = container.create_channel(id, socket).await;
    while let Some(msg) = rx.recv().await {
        println!("User says ::: {:#?}", msg.info);
    }
}
