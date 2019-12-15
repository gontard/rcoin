use block_chain::{Block, BlockChain};
use futures::sync::mpsc::UnboundedSender;
use log::{debug, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod block_chain;
pub mod error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SyncMessage {
    QueryLastBlock(),
    QueryBlockChain(),
    ResponseLastBlock { block: Block },
    ResponseBlockChain { block_chain: BlockChain },
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

    pub fn generate_next_block(&mut self, block_data: String) {
        let block = self.block_chain.generate_next_block(block_data);
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

    pub fn received_block_chain(&mut self, peer_id: usize, block_chain: BlockChain) {
        if self.block_chain.replace_chain(block_chain) {
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
                block_chain: self.block_chain.clone(),
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
            SyncMessage::ResponseBlockChain { block_chain } => {
                self.received_block_chain(peer_id, block_chain)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RCoin, SyncMessage};
    use futures::prelude::*;
    use futures::{sync::mpsc, Stream};
    use std::thread;

    #[test]
    fn should_query_last_block_when_a_peer_is_added() {
        let (tx, rx) = mpsc::unbounded();

        thread::spawn(|| {
            let mut rcoin = RCoin::new();
            rcoin.add_peer(1, tx);
        });

        let messages = rx.collect().wait();
        assert_eq!(messages, Ok(vec![SyncMessage::QueryLastBlock {}]));
    }

    #[test]
    fn should_notify_peer_when_a_block_is_generated() {
        let (tx, rx) = mpsc::unbounded();
        let computation = thread::spawn(move || {
            let mut rcoin = RCoin::new();
            rcoin.add_peer(1, tx);
            rcoin.generate_next_block("stuff".to_string());
            rcoin.block_chain.latest_block().clone()
        });

        let messages = rx.collect().wait();
        let block = computation.join().unwrap();
        assert_eq!(
            messages,
            Ok(vec![
                SyncMessage::QueryLastBlock {},
                SyncMessage::ResponseLastBlock { block }
            ])
        );
    }

    #[test]
    fn should_send_last_block_when_a_peer_queries_it() {
        let (tx1, rx1) = mpsc::unbounded();
        let (tx2, rx2) = mpsc::unbounded();
        let computation = thread::spawn(move || {
            let mut rcoin = RCoin::new();
            rcoin.add_peer(1, tx1);
            rcoin.add_peer(2, tx2);
            rcoin.generate_next_block("stuff".to_string());
            rcoin.peer_message_received(1, SyncMessage::QueryLastBlock {});
            rcoin.block_chain.latest_block().clone()
        });
        let block = computation.join().unwrap();

        let messages1 = rx1.collect().wait();
        assert_eq!(
            messages1,
            Ok(vec![
                SyncMessage::QueryLastBlock {},
                SyncMessage::ResponseLastBlock {
                    block: block.clone()
                },
                SyncMessage::ResponseLastBlock {
                    block: block.clone()
                }
            ])
        );
        let messages2 = rx2.collect().wait();
        assert_eq!(
            messages2,
            Ok(vec![
                SyncMessage::QueryLastBlock {},
                SyncMessage::ResponseLastBlock {
                    block: block.clone()
                }
            ])
        );
    }
}
