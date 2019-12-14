use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use std::time::SystemTime;

// in seconds
const BLOCK_GENERATION_INTERVAL: u32 = 10;

// in blocks
const DIFFICULTY_ADJUSTMENT_INTERVAL: u32 = 10;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Block {
    index: u32,
    hash: String,
    previous_hash: Option<String>,
    timestamp: u64,
    data: String,
    difficulty: u32,
    nonce: u32,
}

impl Block {
    fn genesis() -> Self {
        Block {
            index: 0,
            hash: "816534932c2b7154836da6afc367695e6337db8a921823784c14378abed4f7d7".to_string(),
            previous_hash: None,
            timestamp: 1_465_154_705,
            data: "'my genesis block!!'".to_string(),
            difficulty: 0,
            nonce: 0,
        }
    }

    fn new(
        index: u32,
        hash: String,
        previous_hash: String,
        timestamp: u64,
        data: String,
        difficulty: u32,
        nonce: u32,
    ) -> Self {
        Block {
            index,
            hash,
            previous_hash: Some(previous_hash),
            timestamp,
            data,
            difficulty,
            nonce,
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
        let previous_hash = self.previous_hash.as_ref().unwrap_or(&empty);
        let hash = calculate_hash(
            self.index,
            previous_hash,
            self.timestamp,
            &self.data,
            self.difficulty,
            self.nonce,
        );
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

    pub fn has_higher_index(&self, other_block: &Block) -> bool {
        self.index > other_block.index
    }

    fn is_difficulty_adjustment_block(&self) -> bool {
        self.index != 0 && self.index % DIFFICULTY_ADJUSTMENT_INTERVAL == 0
    }
}

#[derive(Debug, Serialize)]
pub struct BlockChain {
    vec: Vec<Block>,
}

impl Default for BlockChain {
    fn default() -> Self {
        BlockChain {
            vec: vec![Block::genesis()],
        }
    }
}

impl BlockChain {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn blocks(&self) -> &Vec<Block> {
        &self.vec
    }

    pub fn latest_block(&self) -> &Block {
        // safe because there is the genesis block at least
        self.vec.last().unwrap()
    }

    pub fn generate_next_block(&mut self, data: String) -> Block {
        let previous = self.latest_block();
        let next_index = (*previous).index + 1;
        let next_timestamp = now_as_secs();
        let next_block = self.find_block(next_index, &previous.hash, next_timestamp, data);
        let result = next_block.clone();
        self.vec.push(next_block);
        result
    }

    fn find_block(&self, index: u32, previous_hash: &str, timestamp: u64, data: String) -> Block {
        let mut nonce = 0;
        let difficulty = self.get_difficulty();
        let required_prefix = str::repeat("0", difficulty as usize);
        loop {
            let hash = calculate_hash(index, &previous_hash, timestamp, &data, difficulty, nonce);
            if hash.starts_with(&required_prefix) {
                return Block::new(
                    index,
                    hash,
                    previous_hash.to_string(),
                    timestamp,
                    data,
                    difficulty,
                    nonce,
                );
            }
            nonce += 1;
        }
    }

    pub fn replace_chain(&mut self, new_blocks: Vec<Block>) -> bool {
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

    fn get_difficulty(&self) -> u32 {
        let latest_block = self.latest_block();
        if latest_block.is_difficulty_adjustment_block() {
            self.get_adjusted_difficulty()
        } else {
            latest_block.difficulty
        }
    }

    fn get_adjusted_difficulty(&self) -> u32 {
        let latest_block = self.latest_block();
        let prev_adjustment_block: &Block = self
            .blocks()
            .get(self.blocks().len() - DIFFICULTY_ADJUSTMENT_INTERVAL as usize)
            .unwrap();
        let time_expected = (BLOCK_GENERATION_INTERVAL * DIFFICULTY_ADJUSTMENT_INTERVAL) as u64;
        let time_taken = latest_block.timestamp - prev_adjustment_block.timestamp;
        if time_taken < time_expected / 2 {
            prev_adjustment_block.difficulty + 1
        } else if time_taken > time_expected * 2 {
            prev_adjustment_block.difficulty - 1
        } else {
            prev_adjustment_block.difficulty
        }
    }

    pub fn accumulated_difficulty(&self) -> u64 {
        self.blocks()
            .iter()
            .fold(0, |acc, block| acc + 2_u64.pow(block.difficulty.into()))
    }
}

fn calculate_hash(
    index: u32,
    previous_hash: &str,
    timestamp: u64,
    data: &str,
    difficulty: u32,
    nonce: u32,
) -> String {
    let hasher = Sha256::new();
    let result = hasher
        .chain(index.to_ne_bytes())
        .chain(previous_hash)
        .chain(timestamp.to_ne_bytes())
        .chain(data)
        .chain(difficulty.to_ne_bytes())
        .chain(nonce.to_ne_bytes())
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
#[cfg(test)]
mod tests {
    use super::{Block, BlockChain, DIFFICULTY_ADJUSTMENT_INTERVAL};

    #[test]
    fn block_is_hash_valid() {
        let mut block_chain = BlockChain::new();
        let b = block_chain.generate_next_block("data".to_string());
        assert_eq!(true, b.is_hash_valid());
        let block = Block { index: 10, ..b };
        assert_eq!(false, block.is_hash_valid());
    }

    #[test]
    fn block_is_difficulty_adjustment_block() {
        let mut block_chain = BlockChain::new();
        for i in 0..35 {
            let is_adj_block = i == 10 || i == 20 || i == 30;
            assert_eq!(
                is_adj_block,
                block_chain.latest_block().is_difficulty_adjustment_block()
            );
            block_chain.generate_next_block(format!("data-{}", i));
        }
    }

    #[test]
    fn block_chain_genesis() {
        let block_chain = BlockChain::new();
        let genesis = block_chain.latest_block();
        assert_eq!(0, genesis.index);
        assert_eq!(0, genesis.difficulty);
        assert_eq!(0, genesis.nonce);
    }

    #[test]
    fn difficulty_increase_at_the_first_adjustment() {
        let mut block_chain = BlockChain::new();

        generate_n_blocks(&mut block_chain, DIFFICULTY_ADJUSTMENT_INTERVAL);

        let latest_block = block_chain.latest_block();
        assert_eq!(0, latest_block.difficulty);

        generate_n_blocks(&mut block_chain, 1);

        let latest_block = block_chain.latest_block();
        assert_eq!(1, latest_block.difficulty);
    }

    #[test]
    fn difficulty_increase_at_the_second_and_third_adjustment() {
        let mut block_chain = BlockChain::new();
        generate_n_blocks(&mut block_chain, 2 * DIFFICULTY_ADJUSTMENT_INTERVAL + 1);

        let latest_block = block_chain.latest_block();
        assert_eq!(2, latest_block.difficulty);

        generate_n_blocks(&mut block_chain, DIFFICULTY_ADJUSTMENT_INTERVAL);

        let latest_block = block_chain.latest_block();
        assert_eq!(3, latest_block.difficulty);
    }

    #[test]
    fn block_chain_accumulated_difficulty() {
        let mut block_chain = BlockChain::new();

        generate_n_blocks(&mut block_chain, DIFFICULTY_ADJUSTMENT_INTERVAL * 3 + 4);

        let accumulated_difficulty = block_chain.accumulated_difficulty();

        assert_eq!(
            2_u64.pow(0) * 11 // first interval and genesis block: difficulty = 0
                + 2_u64.pow(1) * 10  // second interval: difficulty = 1
                + 2_u64.pow(2) * 10  // third interval: difficulty = 2
                + 2_u64.pow(3) * 4, // last four blocks: difficulty = 3
            accumulated_difficulty
        );
    }

    #[test]
    fn block_chain_add_block() {
        let mut block_chain1 = BlockChain::new();
        let mut block_chain2 = BlockChain::new();
        let mut block_chain3 = BlockChain::new();

        let block1 = block_chain1.generate_next_block("data".to_string());
        assert_eq!(true, block_chain2.add_block(block1.clone()));
        assert_eq!(&block1, block_chain2.latest_block());

        // the block 2 is missing
        let block2 = block_chain1.generate_next_block("another_data".to_string());
        assert_eq!(false, block_chain3.add_block(block2.clone()));

        // invalid hash
        let block3 = Block {
            timestamp: 678,
            ..block2
        };
        assert_eq!(false, block_chain2.add_block(block3.clone()));
    }

    fn generate_n_blocks(block_chain: &mut BlockChain, n: u32) {
        for i in 0..n {
            block_chain.generate_next_block(format!("data-{}", i));
        }
    }
}
