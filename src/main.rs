use std::collections::HashMap;
use std::io::{BufRead, Cursor};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use futures::{future::poll_fn, sync::mpsc};
use log::{debug, warn};
use reqwest::r#async::{Client, Response};
use serde::{Deserialize, Serialize};
use structopt::StructOpt;
use warp::{body, get2, http::StatusCode, path, sse::ServerSentEvent, Buf, Filter, Future, Stream};

use rcoin::{Block, BlockChain};

#[derive(Debug, Deserialize)]
struct BlockData {
    data: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Peer {
    hostname: String,
    port: u16,
}

impl Peer {
    fn sync_url(&self) -> String {
        format!("http://{}:{}/sync", self.hostname, self.port)
    }
    fn get_blocks_url(&self) -> String {
        format!("http://{}:{}/blocks", self.hostname, self.port)
    }
}

#[derive(Clone)]
enum SyncMessage {
    BlockMined(Block),
}

struct RCoin {
    block_chain: BlockChain,
    peers: HashMap<usize, mpsc::UnboundedSender<SyncMessage>>,
}

impl RCoin {
    fn new() -> RCoin {
        RCoin {
            block_chain: BlockChain::new(),
            peers: HashMap::new(),
        }
    }

    fn add_block(&mut self, block: Block) -> bool {
        if self.block_chain.add_block(block.clone()) {
            self.notify_peers(SyncMessage::BlockMined(block));
            return true;
        }
        false
    }

    fn replace_chain(&mut self, blocks: Vec<Block>) -> bool {
        if self.block_chain.replace_chain(blocks) {
            self.notify_peers(SyncMessage::BlockMined(
                self.block_chain.latest_block().clone(),
            ));
            return true;
        }
        false
    }

    fn notify_peers(&self, msg: SyncMessage) {
        debug!("notify {} peers", self.peers.len());
        self.peers.iter().for_each(|(peer_id, tx)| {
            if let Err(err) = tx.unbounded_send(msg.clone()) {
                warn!("error sending msg to {}  {}", peer_id, err);
            }
        });
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
    let blocks_index = path("blocks").and(path::end());
    let peers_index = path("peers").and(path::end());

    let get_blocks = get2().and(blocks_index).and(state.clone()).map(list_blocks);
    let post_block = warp::post2()
        .and(blocks_index)
        .and(body::content_length_limit(1024 * 16))
        .and(body::json())
        .and(state.clone())
        .and_then(mine_block);

    //    let get_peers = get2().and(peers_index).and(state.clone()).map(list_peers);
    let post_peer = warp::post2()
        .and(peers_index)
        .and(body::content_length_limit(1024 * 16))
        .and(body::json())
        .and(state.clone())
        .and_then(connect_to_peer);

    //    let sync_ws = warp::path("sync")
    //        .and(warp::ws2())
    //        .and(state.clone())
    //        .map(|ws: warp::ws::Ws2, state| {
    //            ws.on_upgrade(move |socket| peer_connected(socket, state))
    //        });

    let sync_sse =
        warp::path("sync")
            .and(warp::sse())
            .and(state)
            .map(|sse: warp::sse::Sse, state| {
                // reply using server-sent events
                let stream = peer_connected(state);
                sse.reply(warp::sse::keep_alive().stream(stream))
            });

    let api = get_blocks.or(post_block).or(post_peer).or(sync_sse);
    let routes = api.with(warp::log("rcoin"));

    warp::serve(routes).run(([127, 0, 0, 1], opt.port));
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
    let block = rcoin.block_chain.generate_next_block(block_data.data);

    debug!("block mined: {:?}", block);
    rcoin.notify_peers(SyncMessage::BlockMined(block));
    Ok(StatusCode::CREATED)
}
//
//fn list_peers(state: RCoinState) -> impl warp::Reply {
//    let rcoin = state.lock().unwrap();
//    warp::reply::json(&rcoin.peers)
//}

fn connect_to_peer(peer: Peer, state: RCoinState) -> Result<impl warp::Reply, warp::Rejection> {
    debug!("connect to peer {:?}", peer);
    let request = Client::new()
        .get(&peer.sync_url())
        .header("accept", "text/event-stream")
        .send()
        .and_then(move |response| {
            response.into_body().for_each(move |chunk| {
                let cursor: Cursor<&[u8]> = Cursor::new(chunk.bytes());
                let mut block: Option<Block> = None;
                for line in cursor.lines().map(|l| l.unwrap()) {
                    if line.starts_with("data:") {
                        block = serde_json::from_str(&line[5..]).ok();
                    }
                }
                if let Some(block) = block {
                    debug!("received block {:?} from {:?}", peer, block);
                    let mut rcoin = state.lock().unwrap();
                    if !rcoin.add_block(block) {
                        warn!("block can't be added, let's fetch all the blocks");
                        fetch_peer_blocks(&peer, state.clone())
                    }
                }
                return futures::future::ok(());
            })
        })
        .map_err(|err| warn!("error on the event stream {:?}", err));
    warp::spawn(request);

    Ok(StatusCode::CREATED)
}

fn fetch_peer_blocks(peer: &Peer, state: RCoinState) {
    let json = |mut res: Response| res.json::<Vec<Block>>();
    let request = Client::new()
        .get(&peer.get_blocks_url())
        .send()
        .and_then(json)
        .map(move |blocks: Vec<Block>| {
            let mut rcoin = state.lock().unwrap();
            if rcoin.replace_chain(blocks) {
                debug!("replace all blocks by new ones");
            } else {
                debug!("new blocks can't be added");
            }
            ()
        })
        .map_err(|err| warn!("error on the event stream {:?}", err));
    warp::spawn(request);
}

fn peer_connected(
    state: RCoinState,
) -> impl Stream<Item = impl ServerSentEvent + Send + 'static, Error = warp::Error> + Send + 'static
{
    let peer_id = NEXT_PEER_ID.fetch_add(1, Ordering::Relaxed);
    debug!("peer connected {}", peer_id);

    // Use an unbounded channel to handle buffering and flushing of messages
    // to the event source...
    let (tx, rx) = mpsc::unbounded();

    // Make an extra clone of users list to give to our disconnection handler...
    let rcoin2 = state.clone();

    // Save the sender in our list of connected users.
    state.lock().unwrap().peers.insert(peer_id, tx);

    // Create channel to track disconnecting the receiver side of events.
    // This is little bit tricky.
    let (mut dtx, mut drx) = futures::sync::oneshot::channel::<()>();

    // When `drx` will dropped then `dtx` will be canceled.
    // We can track it to make sure when the peer leaves.
    warp::spawn(poll_fn(move || dtx.poll_cancel()).map(move |_| {
        peer_disconnected(peer_id, &rcoin2);
    }));

    rx.map(|msg| match msg {
        SyncMessage::BlockMined(block) => (warp::sse::event("block-mined"), warp::sse::json(block)),
    })
    .map_err(move |_| {
        // Keep `drx` alive until `rx` will be closed
        drx.close();
        unreachable!("unbounded rx never errors");
    })
}

fn peer_disconnected(my_id: usize, state: &RCoinState) {
    debug!("good bye peer: {}", my_id);

    // Stream closed up, so remove from the user list
    state.lock().unwrap().peers.remove(&my_id);
}
