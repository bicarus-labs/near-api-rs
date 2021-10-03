use near_primitives::types::{BlockReference, Finality};

use crate::new_client;

#[tokio::test]
async fn get_pools() {
    let near_client = new_client("https://rpc.mainnet.near.org");
    let block = near_client
        .block(BlockReference::Finality(Finality::Final))
        .await
        .unwrap();
    println!("block {}", block.header.height);
}
