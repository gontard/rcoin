use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use futures::{sync::mpsc, Future, Sink, Stream};
use log::{debug, error, warn};
use serde::{Deserialize, Serialize};
use structopt::StructOpt;
use warp::Filter;

use rcoin::{
    error::{Error, Kind},
    BlockData, RCoin, SyncMessage,
};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Peer {
    hostname: String,
    port: u16,
}

impl Peer {
    fn sync_url(&self) -> String {
        format!("ws://{}:{}/sync-ws", self.hostname, self.port)
    }
}

#[derive(Debug, StructOpt)]
struct Opt {
    port: u16,
}

type RCoinState = Arc<Mutex<RCoin>>;

/// Our global unique peer id counter.
static NEXT_PEER_ID: AtomicUsize = AtomicUsize::new(1);

fn main() {
    pretty_env_logger::init();
    let opt: Opt = Opt::from_args();

    let state: RCoinState = Arc::new(Mutex::new(RCoin::new()));
    let state = warp::any().map(move || state.clone());
    let blocks_index = warp::path("blocks").and(warp::path::end());
    let peers_index = warp::path("peers").and(warp::path::end());
    let body_conten_length_limit = warp::body::content_length_limit(1024 * 16);

    let get_blocks = warp::get2()
        .and(blocks_index)
        .and(state.clone())
        .map(list_blocks);
    let post_block = warp::post2()
        .and(blocks_index)
        .and(body_conten_length_limit)
        .and(warp::body::json())
        .and(state.clone())
        .and_then(mine_block);

    //    let get_peers = get2().and(peers_index).and(state.clone()).map(list_peers);
    let post_peer = warp::post2()
        .and(peers_index)
        .and(body_conten_length_limit)
        .and(warp::body::json())
        .and(state.clone())
        .and_then(connect_to_peer);

    let sync_ws = warp::path("sync-ws")
        .and(warp::ws2())
        .and(state.clone())
        .map(|ws: warp::ws::Ws2, state| ws.on_upgrade(move |socket| peer_connected(socket, state)));

    let api = get_blocks.or(post_block).or(post_peer).or(sync_ws);
    let routes = api.with(warp::log("rcoin"));

    warp::serve(routes).run(([127, 0, 0, 1], opt.port));
}

fn list_blocks(state: RCoinState) -> impl warp::Reply {
    let rcoin = state.lock().unwrap();
    warp::reply::json(rcoin.blocks())
}

fn mine_block(
    block_data: BlockData,
    state: RCoinState,
) -> Result<impl warp::Reply, warp::Rejection> {
    let mut rcoin = state.lock().unwrap();
    rcoin.generate_next_block(block_data);
    Ok(warp::http::StatusCode::CREATED)
}

fn connect_to_peer(peer: Peer, state: RCoinState) -> Result<impl warp::Reply, warp::Rejection> {
    let url = url::Url::parse(peer.sync_url().as_str()).unwrap();
    let request = tokio_tungstenite::connect_async(url)
        .map_err(|err| warn!("Error during the websocket handshake occurred: {}", err))
        .and_then(move |(ws_stream, _)| {
            debug!("WebSocket handshake has been successfully completed");
            let adapt_error = |err| Error::from(Kind::Tungstenite(err));
            let msg_to_string = move |msg: tungstenite::Message| msg.into_text().unwrap();
            let string_to_msg = |content: String| {
                futures::future::ok::<tungstenite::Message, Error>(tungstenite::Message::text(
                    content,
                ))
            };
            let peer_socket = ws_stream
                .map(msg_to_string)
                .map_err(adapt_error)
                .sink_map_err(adapt_error)
                .with(string_to_msg);
            add_peer(peer_socket, state)
        });

    warp::spawn(request);

    Ok(warp::http::StatusCode::CREATED)
}

fn peer_connected(
    ws: warp::ws::WebSocket,
    state: RCoinState,
) -> impl Future<Item = (), Error = ()> {
    let adapt_error = |err| Error::from(Kind::Warp(err));
    let msg_to_string = move |msg: warp::ws::Message| msg.to_str().unwrap().to_string();
    let string_to_msg = |content: String| {
        futures::future::ok::<warp::ws::Message, Error>(warp::ws::Message::text(content))
    };
    let peer_socket = ws
        .map(msg_to_string)
        .map_err(adapt_error)
        .sink_map_err(adapt_error)
        .with(string_to_msg);
    add_peer(peer_socket, state)
}

fn add_peer<PeerSocket>(
    peer_socket: PeerSocket,
    state: RCoinState,
) -> impl Future<Item = (), Error = ()>
where
    PeerSocket: Stream<Item = String, Error = Error>
        + Sink<SinkItem = String, SinkError = Error>
        + Sized
        + Send
        + 'static,
{
    let peer_id = NEXT_PEER_ID.fetch_add(1, Ordering::Relaxed);
    let (peer_ws_tx, peer_ws_rx) = peer_socket.split();
    // Use an unbounded channel to handle buffering and flushing of messages
    // to the websocket...
    let (tx, rx) = mpsc::unbounded();

    warp::spawn({
        rx.map(|msg| serde_json::to_string(&msg).unwrap())
            .map_err(|()| -> Error { unreachable!("unbounded rx never errors") })
            .forward(peer_ws_tx)
            .map(|_tx_rx| ())
            .map_err(|ws_err| error!("websocket send error: {}", ws_err))
    });

    state.lock().unwrap().add_peer(peer_id, tx);

    // Make an extra clone to give to our disconnection handler...
    let state2 = state.clone();

    peer_ws_rx
        .for_each(move |msg| {
            peer_message_received(peer_id, msg, &state);
            Ok(())
        })
        // for_each will keep processing as long as the user stays
        // connected. Once they disconnect, then...
        .then(move |result| {
            peer_disconnected(peer_id, &state2);
            result
        })
        // If at any time, there was a websocket error, log here...
        .map_err(move |e| {
            error!("websocket error(peer_id={}): {}", peer_id, e);
        })
}

fn peer_message_received(peer_id: usize, msg: String, state: &RCoinState) {
    let message: SyncMessage = serde_json::from_str(msg.as_str()).unwrap();
    let mut rcoin = state.lock().unwrap();
    rcoin.peer_message_received(peer_id, message);
}

fn peer_disconnected(peer_id: usize, state: &RCoinState) {
    // Stream closed up, so remove the peer
    state.lock().unwrap().remove_peer(peer_id);
}
