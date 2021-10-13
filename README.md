# Near API

Rust implementation of Near API library that compatible with `tokio`.

[![Crates.io](https://img.shields.io/crates/v/near-api)](https://crates.io/crates/near-api)

## Usage

First, add this to your `Cargo.toml`:

```toml
[dependencies]
near-api = "0.1.0"
```

## Example

```rust
use near_primitives_v01::types::{BlockReference, Finality};

#[tokio::test]
async fn get_pools() {
    let near_client = near_api::new_client("https://rpc.mainnet.near.org");
    let block = near_client
        .block(BlockReference::Finality(Finality::Final))
        .await
        .unwrap();
    println!("block {}", block.header.height);
}
```
