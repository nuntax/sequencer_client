# SequencerClient
 This library is used to read messages from the arbitrum sequencer feed and parses them into alloy transactions.
## Features
### It supports the following transaction types:
 - Legacy transactions
 - EIP-2930 transactions
 - EIP-1559 transactions
 - EIP-7702 transactions
### It does not support:
 - EIP-4844 transactions
 - Compressed transactions (Not implemented in arbitrum reference either)
 - Non-mutating calls (Not implemented in arbitrum reference either)
 - Heartbeat messages (Deprecated, not used in arbitrum anymore)
## Motivation
Listening to the sequencer feed is generally faster than listening through
subscribing to logs. Since existing libraries ignore batch transactions, which 
seemingly make up for 80% of transactions, I created this library.
## Example 
```rust
async fn main() {
    let url = "wss://arb1-feed.arbitrum.io/feed";
    let (mut reader, mut receiver) = SequencerReader::new(url).await;
    tokio::spawn(async move {
        while let Some(msg) = receiver.recv().await {
            println!("Received message: {:?}", msg);
        }
    });
    reader.start_reading().await;
}
```
