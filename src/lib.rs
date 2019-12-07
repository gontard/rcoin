use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use std::time::SystemTime;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Block {
    index: u32,
    hash: String,
    previous_hash: Option<String>,
    timestamp: u64,
    data: String,
}

impl Block {
    fn genesis() -> Self {
        Block {
            index: 0,
            hash: "816534932c2b7154836da6afc367695e6337db8a921823784c14378abed4f7d7".to_string(),
            previous_hash: None,
            timestamp: 1_465_154_705,
            data: "'my genesis block!!'".to_string(),
        }
    }

    fn new(index: u32, hash: String, previous_hash: String, timestamp: u64, data: String) -> Self {
        Block {
            index,
            hash,
            previous_hash: Some(previous_hash),
            timestamp,
            data,
        }
    }

    fn is_previous_hash(&self, hash: &str) -> bool {
        let previous_hash = self.previous_hash.as_ref();
        match previous_hash {
            Some(previous_hash) => previous_hash == hash,
            None => false,
        }
    }

    fn is_hash_valid(&self) -> bool {
        let empty = String::new();
        let previous_has = self.previous_hash.as_ref().unwrap_or(&empty);
        let data = &self.data;
        let hash = calculate_hash(self.index, previous_has, self.timestamp, data);
        hash == self.hash
    }

    fn is_valid_next_block(&self, next_block: &Block) -> bool {
        match next_block {
            Block { index, .. } if *index != self.index + 1 => false,
            Block { previous_hash, .. } if previous_hash.is_none() => false,
            block if !block.is_previous_hash(&self.hash) => false,
            block if !block.is_hash_valid() => false,
            _ => true,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct BlockChain {
    vec: Vec<Block>,
}

impl BlockChain {
    pub fn new() -> Self {
        BlockChain {
            vec: vec![Block::genesis()],
        }
    }

    pub fn blocks(&self) -> &Vec<Block> {
        &self.vec
    }

    fn latest_block(&self) -> &Block {
        // safe because there is the genesis block at least
        self.vec.last().unwrap()
    }

    pub fn generate_next_block(&mut self, data: String) -> Block {
        let previous = self.latest_block();
        let next_index = (*previous).index + 1;
        let next_timestamp = now_as_secs();
        let next_hash = calculate_hash(next_index, &previous.hash, next_timestamp, &data);
        let next_block = Block::new(
            next_index,
            next_hash,
            previous.hash.clone(),
            next_timestamp,
            data,
        );
        let result = next_block.clone();
        self.vec.push(next_block);
        result
    }

    pub fn replace_chain(&mut self, new_blocks: Vec<Block>) -> bool {
        println!("{}", new_blocks.len() > self.vec.len());
        let is_valid = new_blocks.len() > self.vec.len()
            && new_blocks.first() == self.vec.first()
            && new_blocks
                .iter()
                .zip(new_blocks.iter().skip(1))
                .all(|(prev, current)| prev.is_valid_next_block(current));
        if is_valid {
            self.vec = new_blocks;
        }
        is_valid
    }

    pub fn add_block(&mut self, new_block: Block) -> bool {
        if !self.latest_block().is_valid_next_block(&new_block) {
            return false;
        }
        self.vec.push(new_block);
        true
    }
}

pub fn calculate_hash(index: u32, previous_hash: &str, timestamp: u64, data: &str) -> String {
    let hasher = Sha256::new();
    let result = hasher
        .chain(index.to_ne_bytes())
        .chain(previous_hash)
        .chain(timestamp.to_ne_bytes())
        .chain(data)
        .result();
    format!("{:x}", result)
}

fn now_as_secs() -> u64 {
    let now = SystemTime::now();
    let since_the_epoch = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("Time went backwards");
    since_the_epoch.as_secs()
}
