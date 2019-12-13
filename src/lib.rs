use block_chain::{Block, BlockChain};
use futures::sync::mpsc::UnboundedSender;
use log::{debug, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod block_chain;
pub mod error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncMessage {
    QueryLastBlock(),
    QueryBlockChain(),
    ResponseLastBlock { block: Block },
    ResponseBlockChain { blocks: Vec<Block> },
}

#[derive(Debug, Deserialize)]
pub struct BlockData {
    data: String,
}

#[derive(Default)]
pub struct RCoin {
    block_chain: BlockChain,
    peers: HashMap<usize, UnboundedSender<SyncMessage>>,
}

impl RCoin {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn blocks(&self) -> &Vec<Block> {
        self.block_chain.blocks()
    }

    pub fn generate_next_block(&mut self, block_data: BlockData) {
        let block = self.block_chain.generate_next_block(block_data.data);
        debug!("block mined: {:?}", block);
        self.notify_peers(SyncMessage::ResponseLastBlock { block }, None);
    }

    pub fn add_peer(&mut self, peer_id: usize, tx: UnboundedSender<SyncMessage>) {
        self.peers.insert(peer_id, tx);
        self.notify_peer(peer_id, SyncMessage::QueryLastBlock {});
    }

    pub fn remove_peer(&mut self, peer_id: usize) {
        debug!("good bye peer: {}", peer_id);

        self.peers.remove(&peer_id);
    }

    pub fn received_block(&mut self, peer_id: usize, block: Block) {
        if self.block_chain.add_block(block.clone()) {
            self.notify_peers(SyncMessage::ResponseLastBlock { block }, Some(&peer_id));
        } else if block.has_higher_index(self.block_chain.latest_block()) {
            self.notify_peer(peer_id, SyncMessage::QueryBlockChain {});
        }
    }

    pub fn received_block_chain(&mut self, peer_id: usize, blocks: Vec<Block>) {
        if self.block_chain.replace_chain(blocks) {
            self.notify_peers(
                SyncMessage::ResponseLastBlock {
                    block: self.block_chain.latest_block().clone(),
                },
                Some(&peer_id),
            );
        } else {
            warn!("failed to replace block chain from {}", peer_id);
        }
    }

    fn send_last_block(&self, peer_id: usize) {
        self.peers.get(&peer_id).iter().for_each(|tx| {
            if let Err(err) = tx.unbounded_send(SyncMessage::ResponseLastBlock {
                block: self.block_chain.latest_block().clone(),
            }) {
                warn!("error sending msg to {}  {}", peer_id, err);
            }
        });
    }

    fn send_block_chain(&self, peer_id: usize) {
        self.peers.get(&peer_id).iter().for_each(|tx| {
            debug!("send block chain to {} peer", self.peers.len());
            if let Err(err) = tx.unbounded_send(SyncMessage::ResponseBlockChain {
                blocks: self.block_chain.blocks().clone(),
            }) {
                warn!("error sending msg to {}  {}", peer_id, err);
            }
        });
    }

    fn notify_peers(&self, msg: SyncMessage, exclude_peer_id: Option<&usize>) {
        debug!("notify {} peers", self.peers.len());
        self.peers
            .iter()
            .filter(|(peer_id, _)| match exclude_peer_id {
                Some(excluded_id) => excluded_id != (*peer_id),
                None => true,
            })
            .for_each(|(peer_id, tx)| {
                if let Err(err) = tx.unbounded_send(msg.clone()) {
                    warn!("error sending msg to {}  {}", peer_id, err);
                }
            });
    }

    fn notify_peer(&self, peer_id: usize, msg: SyncMessage) {
        self.peers.get(&peer_id).iter().for_each(|tx| {
            if let Err(err) = tx.unbounded_send(msg.clone()) {
                warn!("error sending msg to {}  {}", peer_id, err);
            }
        });
    }

    pub fn peer_message_received(&mut self, peer_id: usize, msg: SyncMessage) {
        debug!("peer {} msg {:?}", peer_id, msg);
        match msg {
            SyncMessage::QueryLastBlock() => {
                self.send_last_block(peer_id);
            }
            SyncMessage::QueryBlockChain() => {
                self.send_block_chain(peer_id);
            }
            SyncMessage::ResponseLastBlock { block } => self.received_block(peer_id, block),
            SyncMessage::ResponseBlockChain { blocks } => {
                self.received_block_chain(peer_id, blocks)
            }
        }
    }
}
