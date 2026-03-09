use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

use axum::{
    extract::{
        Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use futures_util::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio::sync::{
    Mutex, mpsc,
    mpsc::{UnboundedReceiver, UnboundedSender},
};
use tracing::{debug, warn};

use crate::{
    error::{AppError, AppResult},
    route::AppState,
};

const MAX_ROOM_ID_LEN: usize = 64;
const MAX_MESSAGE_LEN: usize = 500;
const MAX_RECENT_MESSAGES: usize = 20;

#[derive(Debug, Clone, Deserialize)]
pub struct ChatConnectQuery {
    pub user_id: String,
    pub user_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ChatEmptyPayload {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatUserSummary {
    pub user_id: String,
    pub user_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatSessionUser {
    pub connection_id: String,
    pub user_id: String,
    pub user_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatRoomMessage {
    pub room_id: String,
    pub message_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub content: String,
    pub sent_at: i64,
    pub version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatRoomSnapshot {
    pub room_id: String,
    pub members: Vec<ChatUserSummary>,
    pub recent_messages: Vec<ChatRoomMessage>,
    pub version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatPresenceChange {
    pub room_id: String,
    pub joined_members: Vec<ChatUserSummary>,
    pub left_members: Vec<ChatUserSummary>,
    pub version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatLeftRoomNotice {
    pub room_id: String,
    pub version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatErrorPayload {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ChatCommand {
    JoinRoom { room_id: String },
    LeaveRoom { room_id: String },
    SendRoomMessage { room_id: String, content: String },
    SyncRoomState { room_id: String },
    Ping(ChatEmptyPayload),
}

impl ChatCommand {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::JoinRoom { .. } => "join_room",
            Self::LeaveRoom { .. } => "leave_room",
            Self::SendRoomMessage { .. } => "send_room_message",
            Self::SyncRoomState { .. } => "sync_room_state",
            Self::Ping(_) => "ping",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ChatEvent {
    Connected { user_id: String, user_name: String },
    JoinedRoom(ChatRoomSnapshot),
    LeftRoom(ChatLeftRoomNotice),
    RoomMessage(ChatRoomMessage),
    RoomState(ChatRoomSnapshot),
    PresenceChanged(ChatPresenceChange),
    Pong(ChatEmptyPayload),
    Error(ChatErrorPayload),
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ChatError {
    #[error("room id is required")]
    InvalidRoomId,
    #[error("room message content is required")]
    EmptyContent,
    #[error("room message content is too long")]
    ContentTooLong { max_len: usize },
    #[error("connection is not in room {room_id}")]
    NotInRoom { room_id: String },
}

impl ChatError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidRoomId => "invalid_room_id",
            Self::EmptyContent => "empty_content",
            Self::ContentTooLong { .. } => "content_too_long",
            Self::NotInRoom { .. } => "not_in_room",
        }
    }

    pub fn to_event(&self) -> ChatEvent {
        ChatEvent::Error(ChatErrorPayload {
            code: self.code().to_string(),
            message: self.to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatJoinRoomResult {
    pub room_id: String,
    pub members: Vec<ChatUserSummary>,
    pub recent_messages: Vec<ChatRoomMessage>,
    pub version: u64,
    pub joined_newly: bool,
    pub peer_connection_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatSendRoomMessageResult {
    pub room_id: String,
    pub message: ChatRoomMessage,
    pub recipient_connection_ids: Vec<String>,
}

#[derive(Debug, Clone)]
struct ChatConnection {
    user: ChatSessionUser,
    joined_rooms: BTreeSet<String>,
}

#[derive(Debug, Clone)]
struct ChatRoom {
    version: u64,
    members: BTreeMap<String, ChatUserSummary>,
    recent_messages: Vec<ChatRoomMessage>,
}

#[derive(Debug, Default)]
pub struct ChatHub {
    rooms: HashMap<String, ChatRoom>,
    connections: HashMap<String, ChatConnection>,
    next_message_id: u64,
}

impl ChatHub {
    pub fn join_room(
        &mut self,
        user: ChatSessionUser,
        room_id: &str,
    ) -> Result<ChatJoinRoomResult, ChatError> {
        let room_id = normalized_room_id(room_id)?;
        let room = self
            .rooms
            .entry(room_id.clone())
            .or_insert_with(|| ChatRoom {
                version: 0,
                members: BTreeMap::new(),
                recent_messages: Vec::new(),
            });
        let connection = self
            .connections
            .entry(user.connection_id.clone())
            .or_insert_with(|| ChatConnection {
                user: user.clone(),
                joined_rooms: BTreeSet::new(),
            });
        connection.user = user.clone();

        let joined_newly = if room.members.contains_key(&user.connection_id) {
            false
        } else {
            room.version += 1;
            room.members.insert(
                user.connection_id.clone(),
                ChatUserSummary {
                    user_id: user.user_id.clone(),
                    user_name: user.user_name.clone(),
                },
            );
            connection.joined_rooms.insert(room_id.clone());
            true
        };

        Ok(ChatJoinRoomResult {
            room_id: room_id.clone(),
            members: sorted_members(room),
            recent_messages: room.recent_messages.clone(),
            version: room.version,
            joined_newly,
            peer_connection_ids: room
                .members
                .keys()
                .filter(|connection_id| *connection_id != &user.connection_id)
                .cloned()
                .collect(),
        })
    }

    pub fn leave_room(
        &mut self,
        connection_id: &str,
        room_id: &str,
    ) -> Result<(ChatLeftRoomNotice, ChatPresenceChange, Vec<String>), ChatError> {
        let room_id = normalized_room_id(room_id)?;
        let connection = self
            .connections
            .get_mut(connection_id)
            .ok_or_else(|| ChatError::NotInRoom { room_id: room_id.clone() })?;
        let room = self
            .rooms
            .get_mut(&room_id)
            .ok_or_else(|| ChatError::NotInRoom { room_id: room_id.clone() })?;
        let Some(left_member) = room.members.remove(connection_id) else {
            return Err(ChatError::NotInRoom { room_id });
        };

        connection.joined_rooms.remove(&room_id);
        room.version += 1;
        let version = room.version;
        let peer_connection_ids = room.members.keys().cloned().collect::<Vec<_>>();
        let left_notice = ChatLeftRoomNotice { room_id: room_id.clone(), version };
        let presence = ChatPresenceChange {
            room_id: room_id.clone(),
            joined_members: Vec::new(),
            left_members: vec![left_member],
            version,
        };

        if room.members.is_empty() {
            self.rooms.remove(&room_id);
        }
        if connection.joined_rooms.is_empty() {
            self.connections.remove(connection_id);
        }

        Ok((left_notice, presence, peer_connection_ids))
    }

    pub fn send_room_message(
        &mut self,
        connection_id: &str,
        room_id: &str,
        content: &str,
    ) -> Result<ChatSendRoomMessageResult, ChatError> {
        let room_id = normalized_room_id(room_id)?;
        let content = normalized_content(content)?;
        let connection = self
            .connections
            .get(connection_id)
            .ok_or_else(|| ChatError::NotInRoom { room_id: room_id.clone() })?;
        let room = self
            .rooms
            .get_mut(&room_id)
            .ok_or_else(|| ChatError::NotInRoom { room_id: room_id.clone() })?;

        if !room.members.contains_key(connection_id) {
            return Err(ChatError::NotInRoom { room_id });
        }

        room.version += 1;
        self.next_message_id += 1;
        let message = ChatRoomMessage {
            room_id: room_id.clone(),
            message_id: format!("msg-{}", self.next_message_id),
            sender_id: connection.user.user_id.clone(),
            sender_name: connection.user.user_name.clone(),
            content,
            sent_at: OffsetDateTime::now_utc().unix_timestamp(),
            version: room.version,
        };
        room.recent_messages.push(message.clone());
        if room.recent_messages.len() > MAX_RECENT_MESSAGES {
            let drop_len = room.recent_messages.len() - MAX_RECENT_MESSAGES;
            room.recent_messages.drain(0..drop_len);
        }

        Ok(ChatSendRoomMessageResult {
            room_id,
            message,
            recipient_connection_ids: room.members.keys().cloned().collect(),
        })
    }

    pub fn sync_room_state(
        &self,
        connection_id: &str,
        room_id: &str,
    ) -> Result<ChatRoomSnapshot, ChatError> {
        let room_id = normalized_room_id(room_id)?;
        let room = self
            .rooms
            .get(&room_id)
            .ok_or_else(|| ChatError::NotInRoom { room_id: room_id.clone() })?;
        if !room.members.contains_key(connection_id) {
            return Err(ChatError::NotInRoom { room_id });
        }

        Ok(ChatRoomSnapshot {
            room_id,
            members: sorted_members(room),
            recent_messages: room.recent_messages.clone(),
            version: room.version,
        })
    }

    pub fn disconnect(&mut self, connection_id: &str) -> Vec<ChatPresenceChange> {
        let Some(connection) = self.connections.remove(connection_id) else {
            return Vec::new();
        };

        let left_member = ChatUserSummary {
            user_id: connection.user.user_id.clone(),
            user_name: connection.user.user_name.clone(),
        };
        let mut changes = Vec::new();
        for room_id in connection.joined_rooms {
            if let Some(room) = self.rooms.get_mut(&room_id) {
                room.members.remove(connection_id);
                room.version += 1;
                changes.push(ChatPresenceChange {
                    room_id: room_id.clone(),
                    joined_members: Vec::new(),
                    left_members: vec![left_member.clone()],
                    version: room.version,
                });
                if room.members.is_empty() {
                    self.rooms.remove(&room_id);
                }
            }
        }

        changes
    }

    pub fn room_exists(&self, room_id: &str) -> bool {
        self.rooms.contains_key(room_id)
    }

    fn chat_member_connection_ids(&self, room_id: &str) -> Vec<String> {
        self.rooms
            .get(room_id)
            .map(|room| room.members.keys().cloned().collect())
            .unwrap_or_default()
    }
}

#[derive(Debug, Default)]
struct ChatRuntime {
    hub: ChatHub,
    connections: HashMap<String, UnboundedSender<ChatEvent>>,
    next_connection_id: u64,
}

#[derive(Debug, Default)]
pub struct ChatState {
    runtime: Mutex<ChatRuntime>,
}

#[derive(Debug)]
struct ChatDispatch {
    sender: UnboundedSender<ChatEvent>,
    event: ChatEvent,
}

impl ChatState {
    pub async fn register_connection(
        &self,
        user_id: &str,
        user_name: &str,
    ) -> (ChatSessionUser, UnboundedReceiver<ChatEvent>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        let mut runtime = self.runtime.lock().await;
        runtime.next_connection_id += 1;
        let connection_id = format!("chat-{}", runtime.next_connection_id);
        runtime.connections.insert(connection_id.clone(), sender);

        (
            ChatSessionUser {
                connection_id,
                user_id: user_id.to_string(),
                user_name: user_name.to_string(),
            },
            receiver,
        )
    }

    pub async fn send_to_connection(&self, connection_id: &str, event: ChatEvent) {
        let sender = {
            let runtime = self.runtime.lock().await;
            runtime.connections.get(connection_id).cloned()
        };

        if let Some(sender) = sender {
            let _ = sender.send(event);
        }
    }

    pub async fn process_message(
        &self,
        session_user: &ChatSessionUser,
        message: ChatCommand,
    ) -> Result<(), ChatError> {
        let dispatches = {
            let mut runtime = self.runtime.lock().await;
            match message {
                ChatCommand::JoinRoom { room_id } => {
                    let joined = runtime.hub.join_room(session_user.clone(), &room_id)?;
                    let mut dispatches = dispatch_to_connection(
                        runtime
                            .connections
                            .get(&session_user.connection_id)
                            .cloned(),
                        ChatEvent::JoinedRoom(ChatRoomSnapshot {
                            room_id: joined.room_id.clone(),
                            members: joined.members.clone(),
                            recent_messages: joined.recent_messages.clone(),
                            version: joined.version,
                        }),
                    )
                    .into_iter()
                    .collect::<Vec<_>>();

                    if joined.joined_newly {
                        let presence = ChatEvent::PresenceChanged(ChatPresenceChange {
                            room_id: joined.room_id,
                            joined_members: vec![ChatUserSummary {
                                user_id: session_user.user_id.clone(),
                                user_name: session_user.user_name.clone(),
                            }],
                            left_members: Vec::new(),
                            version: joined.version,
                        });
                        dispatches.extend(joined.peer_connection_ids.into_iter().filter_map(
                            |connection_id| {
                                runtime
                                    .connections
                                    .get(&connection_id)
                                    .cloned()
                                    .map(|sender| ChatDispatch { sender, event: presence.clone() })
                            },
                        ));
                    }

                    dispatches
                }
                ChatCommand::LeaveRoom { room_id } => {
                    let (left_notice, presence, peer_connection_ids) = runtime
                        .hub
                        .leave_room(&session_user.connection_id, &room_id)?;
                    let mut dispatches = dispatch_to_connection(
                        runtime
                            .connections
                            .get(&session_user.connection_id)
                            .cloned(),
                        ChatEvent::LeftRoom(left_notice),
                    )
                    .into_iter()
                    .collect::<Vec<_>>();
                    let presence_event = ChatEvent::PresenceChanged(presence);
                    dispatches.extend(peer_connection_ids.into_iter().filter_map(
                        |connection_id| {
                            runtime
                                .connections
                                .get(&connection_id)
                                .cloned()
                                .map(|sender| ChatDispatch {
                                    sender,
                                    event: presence_event.clone(),
                                })
                        },
                    ));
                    dispatches
                }
                ChatCommand::SendRoomMessage { room_id, content } => {
                    let sent = runtime.hub.send_room_message(
                        &session_user.connection_id,
                        &room_id,
                        &content,
                    )?;
                    let event = ChatEvent::RoomMessage(sent.message);
                    sent.recipient_connection_ids
                        .into_iter()
                        .filter_map(|connection_id| {
                            runtime
                                .connections
                                .get(&connection_id)
                                .cloned()
                                .map(|sender| ChatDispatch { sender, event: event.clone() })
                        })
                        .collect()
                }
                ChatCommand::SyncRoomState { room_id } => {
                    let snapshot = runtime
                        .hub
                        .sync_room_state(&session_user.connection_id, &room_id)?;
                    dispatch_to_connection(
                        runtime
                            .connections
                            .get(&session_user.connection_id)
                            .cloned(),
                        ChatEvent::RoomState(snapshot),
                    )
                    .into_iter()
                    .collect()
                }
                ChatCommand::Ping(_) => dispatch_to_connection(
                    runtime
                        .connections
                        .get(&session_user.connection_id)
                        .cloned(),
                    ChatEvent::Pong(ChatEmptyPayload::default()),
                )
                .into_iter()
                .collect(),
            }
        };

        for dispatch in dispatches {
            let _ = dispatch.sender.send(dispatch.event);
        }

        Ok(())
    }

    pub async fn unregister_connection(&self, connection_id: &str) {
        let dispatches = {
            let mut runtime = self.runtime.lock().await;
            runtime.connections.remove(connection_id);
            runtime
                .hub
                .disconnect(connection_id)
                .into_iter()
                .flat_map(|presence| {
                    let event = ChatEvent::PresenceChanged(presence.clone());
                    runtime
                        .hub
                        .chat_member_connection_ids(&presence.room_id)
                        .into_iter()
                        .filter_map(|peer_connection_id| {
                            runtime
                                .connections
                                .get(&peer_connection_id)
                                .cloned()
                                .map(|sender| ChatDispatch { sender, event: event.clone() })
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };

        for dispatch in dispatches {
            let _ = dispatch.sender.send(dispatch.event);
        }
    }
}

pub async fn connect(
    chat_socket: WebSocketUpgrade,
    Query(query): Query<ChatConnectQuery>,
    State(state): State<Arc<AppState>>,
) -> AppResult<impl IntoResponse> {
    let user_id = query.user_id.trim().to_string();
    if user_id.is_empty() {
        return Err(AppError::new("user_id is required"));
    }
    let user_name = query
        .user_name
        .as_deref()
        .unwrap_or(&user_id)
        .trim()
        .to_string();
    if user_name.is_empty() {
        return Err(AppError::new("user_name is invalid"));
    }

    Ok(chat_socket.on_upgrade(move |socket| async move {
        let (session_user, receiver) = state
            .chat_service
            .register_connection(&user_id, &user_name)
            .await;
        run_socket(state.chat_service.clone(), socket, session_user, receiver).await;
    }))
}

async fn run_socket(
    chat_state: Arc<ChatState>,
    socket: WebSocket,
    session_user: ChatSessionUser,
    mut outgoing_rx: UnboundedReceiver<ChatEvent>,
) {
    let connection_id = session_user.connection_id.clone();
    let (mut socket_sender, mut socket_receiver) = socket.split();

    let writer = tokio::spawn(async move {
        while let Some(event) = outgoing_rx.recv().await {
            let payload = match serde_json::to_string(&event) {
                Ok(payload) => payload,
                Err(error) => {
                    warn!(error = %error, "failed to serialize websocket event");
                    continue;
                }
            };

            if socket_sender
                .send(Message::Text(payload.into()))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    chat_state
        .send_to_connection(
            &connection_id,
            ChatEvent::Connected {
                user_id: session_user.user_id.clone(),
                user_name: session_user.user_name.clone(),
            },
        )
        .await;

    while let Some(result) = socket_receiver.next().await {
        match result {
            Ok(Message::Text(text)) => match serde_json::from_str::<ChatCommand>(&text) {
                Ok(message) => {
                    if let Err(error) = chat_state.process_message(&session_user, message).await {
                        chat_state
                            .send_to_connection(&connection_id, error.to_event())
                            .await;
                    }
                }
                Err(error) => {
                    debug!(error = %error, "invalid websocket message");
                    chat_state
                        .send_to_connection(
                            &connection_id,
                            ChatEvent::Error(ChatErrorPayload {
                                code: "invalid_message".to_string(),
                                message: "invalid websocket message".to_string(),
                            }),
                        )
                        .await;
                }
            },
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(error) => {
                debug!(error = %error, "websocket receive failed");
                break;
            }
        }
    }

    chat_state.unregister_connection(&connection_id).await;
    writer.abort();
}

fn dispatch_to_connection(
    sender: Option<UnboundedSender<ChatEvent>>,
    event: ChatEvent,
) -> Option<ChatDispatch> {
    sender.map(|sender| ChatDispatch { sender, event })
}

fn sorted_members(room: &ChatRoom) -> Vec<ChatUserSummary> {
    let mut members = room.members.values().cloned().collect::<Vec<_>>();
    members.sort_by(|left, right| {
        left.user_id
            .cmp(&right.user_id)
            .then(left.user_name.cmp(&right.user_name))
    });
    members
}

fn normalized_room_id(room_id: &str) -> Result<String, ChatError> {
    let room_id = room_id.trim();
    if room_id.is_empty() || room_id.len() > MAX_ROOM_ID_LEN {
        return Err(ChatError::InvalidRoomId);
    }

    Ok(room_id.to_string())
}

fn normalized_content(content: &str) -> Result<String, ChatError> {
    let content = content.trim();
    if content.is_empty() {
        return Err(ChatError::EmptyContent);
    }
    if content.chars().count() > MAX_MESSAGE_LEN {
        return Err(ChatError::ContentTooLong { max_len: MAX_MESSAGE_LEN });
    }

    Ok(content.to_string())
}
