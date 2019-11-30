use rcoin::BlockChain;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use warp::{body, get2, http::StatusCode, path, Filter, Future, Stream};
use warp::ws::{WebSocket, Message};
use std::collections::HashMap;
use futures::sync::mpsc;

#[derive(Debug, Deserialize)]
struct BlockData {
    data: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Peer {
    hostname: String,
    port: u16,
}

struct RCoin {
    block_chain: BlockChain,
    peers: HashMap<usize, mpsc::UnboundedSender<Message>>,
}

impl RCoin {
    fn new() -> RCoin {
        RCoin {
            block_chain: BlockChain::new(),
            peers: HashMap::new(),
        }
    }
}

type RCoinState = Arc<Mutex<RCoin>>;

fn main() {
    pretty_env_logger::init();

    let state: RCoinState = Arc::new(Mutex::new(RCoin::new()));
    let state = warp::any().map(move || state.clone());
    let blocks_index = path("blocks").and(path::end());
    let peers_index = path("peers").and(path::end());

    let get_blocks = get2().and(blocks_index).and(state.clone()).map(list_blocks);
    let post_block = warp::post2()
        .and(blocks_index)
        .and(body::content_length_limit(1024 * 16).and(body::json()))
        .and(state.clone())
        .and_then(mine_block);

//    let get_peers = get2().and(peers_index).and(state.clone()).map(list_peers);
    let post_peer = warp::post2()
        .and(peers_index)
        .and(body::content_length_limit(1024 * 16).and(body::json()))
        .and(state.clone())
        .and_then(connect_to_peer);

    let sync_ws = warp::path("sync")
        .and(warp::ws2())
        .and(state.clone())
        .map(|ws: warp::ws::Ws2, state| {
            ws.on_upgrade(move |socket| peer_connected(socket, state))
        });

    let api = get_blocks.or(post_block).or(post_peer).or(sync_ws);
    let routes = api.with(warp::log("rcoin"));

    warp::serve(routes).run(([127, 0, 0, 1], 3030));
}

fn list_blocks(state: RCoinState) -> impl warp::Reply {
    let rcoin = state.lock().unwrap();
    warp::reply::json(rcoin.block_chain.blocks())
}

fn mine_block(
    block_data: BlockData,
    state: RCoinState,
) -> Result<impl warp::Reply, warp::Rejection> {
    let mut rcoin = state.lock().unwrap();
    rcoin.block_chain.generate_next_block(block_data.data);
    Ok(StatusCode::CREATED)
}
//
//fn list_peers(state: RCoinState) -> impl warp::Reply {
//    let rcoin = state.lock().unwrap();
//    warp::reply::json(&rcoin.peers)
//}

fn connect_to_peer(peer: Peer, state: RCoinState) -> Result<impl warp::Reply, warp::Rejection> {
    let mut rcoin = state.lock().unwrap();

    Ok(StatusCode::CREATED)
}

fn peer_connected(ws: WebSocket, state: RCoinState) -> impl Future<Item=(), Error=()> {
    let peer_id= 1;
    println!("peer_connected {}", peer_id);

    // Split the socket into a sender and receive of messages.
    let (ws_tx, ws_rx) = ws.split();

    // Use an unbounded channel to handle buffering and flushing of messages
    // to the websocket...
    let (tx, rx) = mpsc::unbounded();
    state.lock().unwrap().peers.insert(peer_id, tx);

//    warp::spawn(
//        rx.map_err(|()| -> warp::Error { unreachable!("unbounded rx never errors") })
//            .forward(user_ws_tx)
//            .map(|_tx_rx| ())
//            .map_err(|ws_err| eprintln!("websocket send error: {}", ws_err)),
//    );



    // Return a `Future` that is basically a state machine managing
    // this specific peer's connection.

    // Make an extra clone to give to our disconnection handler...
    let rcoin2 = state.clone();

    ws_rx.for_each(move |msg| {
        peer_message(peer_id, msg, &state);
        Ok(())
    }).then(move |result| {
        peer_disconnected(peer_id, &rcoin2);
        result
    }).map_err(move |e| {
        eprintln!("websocket error: {}", e);
    })
}

fn peer_message(peer_id: usize, msg: Message, state: &RCoinState) {
    // Skip any non-Text messages...
    let msg = if let Ok(s) = msg.to_str() {
        s
    } else {
        return;
    };

    let new_msg = format!("<User#{}>: {}", peer_id, msg);
    println!("{}", new_msg);

//    // New message from this user, send it to everyone else (except same uid)...
//    //
//    // We use `retain` instead of a for loop so that we can reap any user that
//    // appears to have disconnected.
//    for (&uid, tx) in users.lock().unwrap().iter() {
//        if my_id != uid {
//            match tx.unbounded_send(Message::text(new_msg.clone())) {
//                Ok(()) => (),
//                Err(_disconnected) => {
//                    // The tx is disconnected, our `user_disconnected` code
//                    // should be happening in another task, nothing more to
//                    // do here.
//                }
//            }
//        }
//    }
}

fn peer_disconnected(my_id: usize, state: &RCoinState) {
    println!("good bye peer: {}", my_id);

    // Stream closed up, so remove from the user list
    state.lock().unwrap().peers.remove(&my_id);
}
