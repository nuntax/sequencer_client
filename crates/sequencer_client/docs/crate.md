# SequencerClient

A Rust library for reading and parsing messages from the Arbitrum sequencer feed into typed transactions.

## Features

### Supported Transaction Types

**Standard L2 Transactions:**
- Legacy transactions
- EIP-2930 transactions (access lists)
- EIP-1559 transactions (dynamic fees)
- EIP-7702 transactions (set code)

**Arbitrum-Specific Transactions:**
- ETH deposits (TxDeposit)
- Retryable tickets (SubmitRetryableTx)
- Batch posting reports (internal transaction type)

### Key Capabilities

- Multiple parallel WebSocket connections for improved throughput
- Automatic reconnection on connection failures
- Out-of-order message buffering and reordering
- Recursive parsing of nested batch transactions
- Stream-based API using Rust async streams

### Not Supported

- EIP-4844 transactions (blob transactions)
- Compressed transactions (not implemented in Arbitrum reference implementation)
- Non-mutating calls (not implemented in Arbitrum reference implementation)
- Heartbeat messages (deprecated, not used in Arbitrum anymore)

## Important Notes

**Crypto Provider Required:**
You must install a default rustls crypto provider before using this library, otherwise it will panic.

```rust
rustls::crypto::aws_lc_rs::default_provider()
    .install_default()
    .expect("Failed to install rustls crypto provider");
```

**Reference Implementation:**
This library follows the Arbitrum Nitro implementation:
<https://github.com/OffchainLabs/nitro/blob/9b1e622102fa2bebfd7dffd327be19f8881f1467/arbos/incomingmessage.go#L328>



## Usage

### Basic Example

```rust
use futures_util::stream::StreamExt;
use sequencer_client::reader::SequencerReader;
use arb_alloy_consensus::transactions::ArbTxEnvelope;

#[tokio::main]
async fn main() {
    // Install a default crypto provider (required)
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    // Connect to the Arbitrum One sequencer feed
    let url = "wss://arb1-feed.arbitrum.io/feed";
    let chain_id = 42161; // Arbitrum One
    let connections = 10; // Number of parallel WebSocket connections

    let reader = SequencerReader::new(url, chain_id, connections).await;
    let mut stream = reader.into_stream();

    // Process messages from the stream
    while let Some(msg_result) = stream.next().await {
        match msg_result {
            Ok(msg) => {
                println!("Sequence: {}, Transactions: {}",
                    msg.sequence_number,
                    msg.txs.len()
                );

                // Access individual transactions
                for tx in msg.txs {
                    match tx {
                        ArbTxEnvelope::Legacy(tx) => {
                            println!("Legacy tx: {}", tx.tx_hash());
                        }
                        ArbTxEnvelope::Eip1559(tx) => {
                            println!("EIP-1559 tx: {}", tx.tx_hash());
                        }
                        ArbTxEnvelope::DepositTx(tx) => {
                            println!("Deposit from: {}", tx.from());
                        }
                        ArbTxEnvelope::SubmitRetryableTx(tx) => {
                            println!("Retryable ticket from: {}", tx.from());
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                eprintln!("Error: {}", e);
            }
        }
    }
}
```

### SequencerMessage Structure

Each message from the stream contains:

- `sequence_number`: The sequential message number from the feed
- `txs`: Vector of parsed transactions (`Vec<ArbTxEnvelope>`)
- `is_last_in_block`: Whether this is the last message in a block
- `timestamp`: Message timestamp from the L1 header
- `received_at`: Local instant when the message was received
- `l1_header`: L1 header information

### API Notes

**Connection Parameters:**
- `url`: WebSocket endpoint (e.g., `wss://arb1-feed.arbitrum.io/feed`)
- `chain_id`: Network chain ID (42161 for Arbitrum One, 42170 for Arbitrum Nova)
- `connections`: Number of parallel WebSocket connections (recommended: less than 10)

**Stream Behavior:**
- Messages are automatically reordered if received out of sequence
- Connections automatically reconnect on failure
- Duplicate messages are filtered out
- The stream yields `Result<SequencerMessage>` items

## About the Type System

This library uses two internal crates for Arbitrum-specific types:

**`arb_alloy_consensus`**: Contains consensus-layer types for Arbitrum transactions, including deposit transactions, retryable tickets, and batch posting reports. These types extend the standard Alloy transaction types to support Arbitrum's unique transaction formats.

**`arb_alloy_network`**: Contains network-layer types for the sequencer feed protocol, including message parsing and WebSocket communication structures.

**Note on Naming**: The current naming convention doesn't follow typical Alloy crate naming patterns. These crates are named with the `arb_alloy_` prefix in anticipation of potential future extraction as standalone crates that could serve the broader Arbitrum Rust ecosystem. If extracted, the naming may be revised to better align with Alloy conventions or community feedback.
