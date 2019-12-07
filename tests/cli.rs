use assert_cmd::prelude::*;
use predicates::prelude::*;
use predicates::str::ContainsPredicate;
use reqwest::{Error, Response};
use serde::Serialize;
use std::process::{Child, Command};

#[derive(Debug, Serialize)]
struct BlockData {
    data: String,
}

#[derive(Debug, Serialize)]
struct Peer {
    hostname: String,
    port: u16,
}

struct RCoinNode {
    port: u16,
    child_process: Child,
}

impl RCoinNode {
    fn start(port: u16) -> Self {
        let child_process = Command::cargo_bin("rcoin")
            .unwrap()
            .arg(port.to_string())
            .spawn()
            .unwrap();
        wait_a_little_bit();
        RCoinNode {
            port,
            child_process,
        }
    }

    fn add_peer(&self, peer: &Peer) -> Result<Response, Error> {
        let client = reqwest::Client::builder().build()?;
        let result = client
            .post(format!("http://localhost:{}/peers", self.port).as_str())
            .json(&peer)
            .send();
        wait_a_little_bit();
        result
    }

    fn listen_other_node(&self, other_node: &RCoinNode) -> Result<Response, Error> {
        self.add_peer(&Peer {
            hostname: "localhost".to_string(),
            port: other_node.port,
        })
    }

    fn mine_block(&self, data: &str) -> Result<Response, Error> {
        let block_data = BlockData {
            data: data.to_string(),
        };
        let client = reqwest::Client::builder().build()?;
        let result = client
            .post(format!("http://localhost:{}/blocks", self.port).as_str())
            .json(&block_data)
            .send();
        wait_a_little_bit();
        result
    }

    fn get_blocks(&self) -> Result<Response, Error> {
        let client = reqwest::Client::builder().build()?;
        client
            .get(format!("http://localhost:{}/blocks", self.port).as_str())
            .send()
    }

    fn stop(&mut self) -> std::io::Result<()> {
        self.child_process.kill()
    }
}

fn wait_a_little_bit() {
    std::thread::sleep(std::time::Duration::from_millis(50));
}

fn contains_n_blocks(n: usize) -> ContainsPredicate {
    predicate::str::contains(format!(r#""index":{}"#, n - 1))
}

#[test]
fn control_one_node() -> Result<(), Box<dyn std::error::Error>> {
    let mut node = RCoinNode::start(17000);

    let node_blocks = node.get_blocks()?.text()?;
    assert_eq!(true, contains_n_blocks(1).eval(&node_blocks));

    node.mine_block("The first block data")?;
    let node_blocks = node.get_blocks()?.text()?;
    assert_eq!(true, contains_n_blocks(2).eval(&node_blocks));
    assert_eq!(
        true,
        predicate::str::contains("The first block data").eval(&node_blocks)
    );

    node.mine_block("The second block data")?;
    let node_blocks = node.get_blocks()?.text()?;
    assert_eq!(true, contains_n_blocks(3).eval(&node_blocks));
    assert_eq!(
        true,
        predicate::str::contains("The second block data").eval(&node_blocks)
    );

    node.stop()?;

    Ok(())
}

#[test]
fn sync_two_nodes() -> Result<(), Box<dyn std::error::Error>> {
    let mut node1 = RCoinNode::start(17001);
    let mut node2 = RCoinNode::start(17002);

    node2.listen_other_node(&node1)?;
    node1.mine_block("The first block data")?;
    node1.mine_block("The second block data")?;
    node1.mine_block("The third block data")?;

    let node1_blocks = node1.get_blocks()?.text()?;
    let node2_blocks = node2.get_blocks()?.text()?;

    node1.stop()?;
    node2.stop()?;

    let contains_two_blocks = contains_n_blocks(3);
    assert_eq!(true, contains_two_blocks.eval(&node1_blocks));
    assert_eq!(true, contains_two_blocks.eval(&node2_blocks));
    assert_eq!(&node1_blocks, &node2_blocks);

    Ok(())
}

#[test]
fn sync_three_nodes_in_chain() -> Result<(), Box<dyn std::error::Error>> {
    let mut node1 = RCoinNode::start(17003);
    let mut node2 = RCoinNode::start(17004);
    let mut node3 = RCoinNode::start(17005);

    node2.listen_other_node(&node1)?;
    node3.listen_other_node(&node2)?;
    node1.mine_block("The first block data")?;
    node1.mine_block("The second block data")?;

    let node1_blocks = node1.get_blocks()?.text()?;
    let node2_blocks = node2.get_blocks()?.text()?;
    let node3_blocks = node3.get_blocks()?.text()?;

    node1.stop()?;
    node2.stop()?;
    node3.stop()?;

    let contains_two_blocks = contains_n_blocks(2);
    assert_eq!(true, contains_two_blocks.eval(&node1_blocks));
    assert_eq!(true, contains_two_blocks.eval(&node2_blocks));
    assert_eq!(true, contains_two_blocks.eval(&node3_blocks));
    assert_eq!(&node1_blocks, &node2_blocks);
    assert_eq!(&node2_blocks, &node3_blocks);

    Ok(())
}

#[test]
fn error_port_not_specified() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("rcoin")?;

    cmd.assert()
        .code(1)
        .failure()
        .stderr(predicate::str::contains(
            "error: The following required arguments were not provided:\n    <port>",
        ));

    Ok(())
}

#[test]
fn error_port_is_not_an_int() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("rcoin")?;

    cmd.arg("abc")
        .assert()
        .code(1)
        .failure()
        .stderr(predicate::str::contains(
            "error: Invalid value for '<port>': invalid digit found in string",
        ));

    Ok(())
}

#[test]
fn error_port_too_large() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("rcoin")?;

    cmd.arg("100000000000000000")
        .assert()
        .code(1)
        .failure()
        .stderr(predicate::str::contains(
            "error: Invalid value for '<port>': number too large to fit in target type",
        ));

    Ok(())
}
