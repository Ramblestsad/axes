use serde_json::json;

use crate::handlers::chat::{
    ChatCommand, ChatError, ChatEvent, ChatHub, ChatSessionUser, ChatUserSummary,
};

fn session_user(id: &str, name: &str) -> ChatSessionUser {
    ChatSessionUser {
        connection_id: format!("conn-{id}"),
        user_id: id.to_string(),
        user_name: name.to_string(),
    }
}

#[test]
fn chat_wire_protocol_uses_tagged_json_messages() {
    let join_message: ChatCommand = serde_json::from_value(json!({
        "type": "join_room",
        "payload": {
            "room_id": "lobby"
        }
    }))
    .expect("join_room message should deserialize");

    assert_eq!(join_message.event_type(), "join_room");

    let connected = ChatEvent::Connected { user_id: "u1".to_string(), user_name: "rc".to_string() };

    assert_eq!(
        serde_json::to_value(&connected).expect("connected event should serialize"),
        json!({
            "type": "connected",
            "payload": {
                "user_id": "u1",
                "user_name": "rc"
            }
        })
    );
}

#[test]
fn join_room_is_idempotent_and_returns_current_snapshot() {
    let mut hub = ChatHub::default();
    let joined = hub
        .join_room(session_user("u1", "rc"), "lobby")
        .expect("join should succeed");

    assert_eq!(joined.version, 1);
    assert_eq!(
        joined.members,
        vec![ChatUserSummary { user_id: "u1".to_string(), user_name: "rc".to_string() }]
    );
    assert!(joined.recent_messages.is_empty());

    let joined_again = hub
        .join_room(session_user("u1", "rc"), "lobby")
        .expect("repeated join should be idempotent");

    assert_eq!(joined_again.version, 1);
    assert_eq!(joined_again.members.len(), 1);
}

#[test]
fn send_message_requires_membership_and_history_is_capped() {
    let mut hub = ChatHub::default();
    let user = session_user("u1", "rc");

    let error = hub
        .send_room_message(&user.connection_id, "lobby", "hello")
        .expect_err("sending without joining should fail");
    assert_eq!(error, ChatError::NotInRoom { room_id: "lobby".to_string() });

    hub.join_room(user.clone(), "lobby")
        .expect("join should succeed");

    for idx in 0..25 {
        hub.send_room_message(&user.connection_id, "lobby", &format!("msg-{idx}"))
            .expect("message should be accepted");
    }

    let snapshot = hub
        .sync_room_state(&user.connection_id, "lobby")
        .expect("joined user should sync state");

    assert_eq!(snapshot.version, 26);
    assert_eq!(snapshot.recent_messages.len(), 20);
    assert_eq!(
        snapshot
            .recent_messages
            .first()
            .expect("history should have first item")
            .content,
        "msg-5"
    );
    assert_eq!(
        snapshot
            .recent_messages
            .last()
            .expect("history should have last item")
            .content,
        "msg-24"
    );
}

#[test]
fn disconnect_removes_members_and_drops_empty_rooms() {
    let mut hub = ChatHub::default();
    let alice = session_user("u1", "alice");
    let bob = session_user("u2", "bob");

    hub.join_room(alice.clone(), "lobby").expect("alice joins");
    hub.join_room(bob.clone(), "lobby").expect("bob joins");

    let disconnected = hub.disconnect(&alice.connection_id);
    assert_eq!(disconnected.len(), 1);
    assert_eq!(disconnected[0].room_id, "lobby");
    assert_eq!(
        disconnected[0].left_members,
        vec![ChatUserSummary { user_id: "u1".to_string(), user_name: "alice".to_string() }]
    );

    let remaining = hub
        .sync_room_state(&bob.connection_id, "lobby")
        .expect("bob should still see the room");
    assert_eq!(remaining.members.len(), 1);
    assert_eq!(remaining.members[0].user_id, "u2");

    let disconnected_last = hub.disconnect(&bob.connection_id);
    assert_eq!(disconnected_last.len(), 1);
    assert!(!hub.room_exists("lobby"));
}

#[test]
fn leave_room_removes_membership_and_reports_updated_version() {
    let mut hub = ChatHub::default();
    let alice = session_user("u1", "alice");
    let bob = session_user("u2", "bob");

    hub.join_room(alice.clone(), "lobby").expect("alice joins");
    hub.join_room(bob.clone(), "lobby").expect("bob joins");

    let (left_notice, presence, peers) = hub
        .leave_room(&alice.connection_id, "lobby")
        .expect("leave should succeed");

    assert_eq!(left_notice.room_id, "lobby");
    assert_eq!(left_notice.version, 3);
    assert_eq!(presence.version, 3);
    assert_eq!(presence.left_members[0].user_id, "u1");
    assert_eq!(peers, vec![bob.connection_id.clone()]);
}
