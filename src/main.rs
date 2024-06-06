use futures_channel::mpsc::unbounded;
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};
use nanochat_ws::{handle_binary_message, handle_text_message, PeerMap, UserMap};
use std::{collections::HashMap, env, io::Error as IoError, net::SocketAddr, sync::Mutex};
use tokio::net::{TcpListener, TcpStream};

async fn handle_connection(
    peer_map: PeerMap,
    user_map: UserMap,
    raw_stream: TcpStream,
    addr: SocketAddr,
) {
    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .expect("Error during the websocket handshake occurred");

    let (tx, rx) = unbounded();

    peer_map.lock().unwrap().insert(addr, tx);

    let (outgoing, incoming) = ws_stream.split();

    let redirect_incoming = incoming.try_for_each(|msg| match msg {
        msg if msg.is_text() => handle_text_message(msg, &peer_map, &user_map, &addr),
        msg if msg.is_binary() => handle_binary_message(msg, &peer_map, &user_map),
        _ => future::ok(()),
    });

    let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(redirect_incoming, receive_from_others);
    future::select(redirect_incoming, receive_from_others).await;

    peer_map.lock().unwrap().remove(&addr);
}

#[tokio::main]
async fn main() -> Result<(), IoError> {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());

    let peers = PeerMap::new(Mutex::new(HashMap::new()));
    let users = UserMap::new(Mutex::new(HashMap::new()));
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");

    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(handle_connection(
            peers.clone(),
            users.clone(),
            stream,
            addr,
        ));
    }

    Ok(())
}
