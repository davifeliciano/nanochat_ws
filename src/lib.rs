use chrono::NaiveDateTime;
use dotenvy_macro::dotenv;
use futures_channel::mpsc::UnboundedSender;
use futures_util::future;
use jsonwebtoken::{DecodingKey, Validation};
use prost::Message as _;
use serde::Deserialize;
use std::{
    collections::HashMap,
    io::Cursor,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

pub mod chat;

pub type Tx = UnboundedSender<Message>;
pub type PeerMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;
pub type UserMap = Arc<Mutex<HashMap<Uuid, SocketAddr>>>;

#[derive(Deserialize)]
#[allow(dead_code)]
struct AuthenticatedUser {
    id: Uuid,
    username: String,
    pbkdf2_salt: String,
    created_at: NaiveDateTime,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct Claims {
    user: AuthenticatedUser,
    exp: usize,
}

pub fn verify_token(token: &str) -> Result<Uuid, jsonwebtoken::errors::Error> {
    jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_secret(dotenv!("ACCESS_TOKEN_SECRET").as_bytes()),
        &Validation::new(jsonwebtoken::Algorithm::HS256),
    )
    .map(|payload| payload.claims.user.id)
}

pub fn handle_text_message(
    msg: Message,
    peer_map: &PeerMap,
    user_map: &UserMap,
    addr: &SocketAddr,
) -> future::Ready<Result<(), tokio_tungstenite::tungstenite::Error>> {
    let text = msg.to_text();

    if text.is_err() {
        return future::ok(());
    }

    let sender_id = verify_token(text.unwrap());

    if sender_id.is_err() {
        return future::ok(());
    }

    let sender_id = sender_id.unwrap();
    let mut peers = peer_map.lock().unwrap();
    let mut users = user_map.lock().unwrap();

    if let Some(addr) = users.get(&sender_id) {
        peers.remove(addr).and_then(|tx| Some(tx.close_channel()));
    }

    users.insert(sender_id, *addr);

    future::ok(())
}

pub fn handle_binary_message(
    msg: Message,
    peer_map: &PeerMap,
    user_map: &UserMap,
) -> future::Ready<Result<(), tokio_tungstenite::tungstenite::Error>> {
    let bytes = msg.clone().into_data();
    let chat_msg = chat::Message::decode(&mut Cursor::new(bytes)).ok();

    if chat_msg.is_none() {
        return future::ok(());
    }

    let chat_msg = chat_msg.unwrap();
    let sender_id = chat_msg.verify_token().ok();
    let recipient_id = Uuid::parse_str(&chat_msg.recipient_id).ok();

    if sender_id.is_none() || recipient_id.is_none() {
        return future::ok(());
    }

    let peers = peer_map.lock().unwrap();
    let users = user_map.lock().unwrap();

    let recipient_tx = recipient_id
        .map(|recipient_id| users.get(&recipient_id))
        .flatten()
        .map(|recipient_addr| peers.get(recipient_addr))
        .flatten();

    if let Some(tx) = recipient_tx {
        tx.unbounded_send(msg).unwrap();
    }

    future::ok(())
}
