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

    fn assert_blocks_count(&self, count: usize) -> Result<(), Error> {
        let blocks = self.get_blocks()?.text()?;
        assert_eq!(true, contains_n_blocks(count).eval(&blocks));
        assert_eq!(false, contains_n_blocks(count + 1).eval(&blocks));
        Ok(())
    }

    fn assert_blocks_contains(&self, pattern: &str) -> Result<(), Error> {
        let blocks = self.get_blocks()?.text()?;
        assert_eq!(true, predicate::str::contains(pattern).eval(&blocks));
        Ok(())
    }

    fn assert_sync_with_node(&self, other_node: &RCoinNode) -> Result<(), Error> {
        let blocks = self.get_blocks()?.text()?;
        let other_blocks = other_node.get_blocks()?.text()?;
        assert_eq!(blocks, other_blocks);
        Ok(())
    }

    fn stop(&mut self) -> std::io::Result<()> {
        self.child_process.kill()
    }
}

impl Drop for RCoinNode {
    fn drop(&mut self) {
        if let Err(err) = self.stop() {
            println!("error {:?}", err)
        }
    }
}

fn wait_a_little_bit() {
    std::thread::sleep(std::time::Duration::from_millis(50));
}

fn contains_n_blocks(n: usize) -> ContainsPredicate {
    predicate::str::contains(format!(r#""index":{}"#, n - 1))
}

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn control_one_node() -> TestResult {
    let node = RCoinNode::start(17000);

    node.assert_blocks_count(1)?;

    node.mine_block("The first block data")?;

    node.assert_blocks_count(2)?;
    node.assert_blocks_contains("The first block data")?;

    node.mine_block("The second block data")?;

    node.assert_blocks_count(3)?;
    node.assert_blocks_contains("The second block data")?;

    Ok(())
}

#[test]
fn connect_two_nodes_then_mine_some_blocks() -> TestResult {
    let node1 = RCoinNode::start(17001);
    let node2 = RCoinNode::start(17002);

    node2.listen_other_node(&node1)?;
    node1.mine_block("The first block data")?;
    node1.mine_block("The second block data")?;
    node1.mine_block("The third block data")?;

    node1.assert_blocks_count(4)?;
    node2.assert_sync_with_node(&node1)?;

    Ok(())
}

#[test]
fn connect_three_nodes_in_a_chain_then_mine_some_blocks() -> TestResult {
    let node1 = RCoinNode::start(17003);
    let node2 = RCoinNode::start(17004);
    let node3 = RCoinNode::start(17005);

    node2.listen_other_node(&node1)?;
    node3.listen_other_node(&node2)?;
    node1.mine_block("The first block data")?;
    node1.mine_block("The second block data")?;

    let node1_blocks = node1.get_blocks()?.text()?;
    let node2_blocks = node2.get_blocks()?.text()?;
    let node3_blocks = node3.get_blocks()?.text()?;

    node1.assert_blocks_count(3)?;
    node2.assert_sync_with_node(&node1)?;
    node3.assert_sync_with_node(&node1)?;

    Ok(())
}

#[test]
fn mine_some_blocks_then_connect_a_node() -> TestResult {
    let node1 = RCoinNode::start(17006);
    let node2 = RCoinNode::start(17007);

    node1.mine_block("The first block data")?;
    node2.listen_other_node(&node1)?;
    node1.mine_block("The second block data")?;

    node1.assert_blocks_count(3)?;
    node2.assert_sync_with_node(&node1)?;

    Ok(())
}

#[test]
fn error_port_not_specified() -> TestResult {
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
fn error_port_is_not_an_int() -> TestResult {
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
fn error_port_too_large() -> TestResult {
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
