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

Reference implementation:
<https://github.com/OffchainLabs/nitro/blob/9b1e622102fa2bebfd7dffd327be19f8881f1467/arbos/incomingmessage.go#L328>

## Warning
Using the library without a defaut CryptoProvider will cause a panic.
Make sure to install a default CryptoProvider.



## Example 
```rust
use sequencer_client::reader::SequencerReader;
#[tokio::main()]
async fn main() {
    let url = "wss://arb1-feed.arbitrum.io/feed";
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");
    let (mut reader, mut receiver) = SequencerReader::new(url).await;
    tokio::spawn(async move {
        while let Some(msg) = receiver.recv().await {
            println!("Received message: {:?}", msg.sequence_number());
        }
    });
    reader.start_reading().await;
}

```
