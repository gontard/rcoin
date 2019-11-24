use rcoin::{calculate_hash, BlockChain};

fn main() {
    let mut block_chain = BlockChain::new();
    block_chain.generate_next_block("ma data 2".to_string());
    block_chain.generate_next_block("ma data 3".to_string());
    println!("{:#?}", block_chain);

    let mut block_chain_2 = BlockChain::new();
    block_chain_2.generate_next_block("ma data 5".to_string());
    block_chain_2.generate_next_block("ma data 6".to_string());
    block_chain_2.generate_next_block("ma data 7".to_string());

    println!("{:#?}", block_chain.replace_chain(block_chain_2.vec));
    println!("{:#?}", block_chain);
}
