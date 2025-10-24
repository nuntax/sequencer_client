# SequencerClient
 This library is used to read messages from the arbitrum sequencer feed and parses them into alloy transactions.
## Features
### It supports the following transaction types
- L2 Messages
  - Legacy transactions
  - EIP-2930 transactions
  - EIP-1559 transactions
  - EIP-7702 transactions
- Arbitrum Specific Transactions
  - SubmitRetryable
  - More to come (it's a wip)
## Motivation
Listening to the sequencer feed is generally faster than listening through
subscribing to logs. Since existing libraries ignore batch transactions, which 
seemingly make up for 80% of transactions, I created this library.
In the long run, types from this library could be extracted into a seperate crate at some point to support arbitrum tooling in rust.
## Example 
```rust
use futures_util::stream::StreamExt;
use sequencer_client::reader::SequencerReader;
#[tokio::main()]
async fn main() {
    let url = "wss://arb1-feed.arbitrum.io/feed";
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::DEBUG) // Changed to DEBUG to see more logs
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set global default subscriber");

    tracing::info!("Starting application");

    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    tracing::info!("Connecting to sequencer at {}", url);

    let reader = SequencerReader::new(url, 42161, 10).await;

    let mut stream = reader.into_stream();
    tracing::info!("Created stream, starting to read messages...");

    let timeout = std::time::Duration::from_secs(30);

    loop {
        match tokio::time::timeout(timeout, stream.next()).await {
            Ok(Some(msg_result)) => {
                match msg_result {
                    Ok(msg) => {
                        tracing::info!(
                            "Received message: {:?}, elapsed: {:?}",
                            msg.sequence_number,
                            msg.received_at.elapsed(),
                        );
                        for tx in msg.messages {
                            match tx {
                            arb_alloy_consensus::transactions::ArbTxEnvelope::DepositTx(tx_deposit) => {
                                tracing::info!("DepositTx from: {:?}, hash: {:?}", tx_deposit.from(), tx_deposit.tx_hash());
                            },
                            arb_alloy_consensus::transactions::ArbTxEnvelope::SubmitRetryableTx(submit_retryable_tx) => {
                                tracing::info!("RetryableTx from: {:?}, hash: {:?}", submit_retryable_tx.from(), submit_retryable_tx.tx_hash());

                            },
                            arb_alloy_consensus::transactions::ArbTxEnvelope::ArbitrumInternal(arbitrum_internal_tx) => {
                                tracing::info!("BatchPostingReport from: {:?}, hash: {:?}", arbitrum_internal_tx.from(), arbitrum_internal_tx.tx_hash());

                            },
                            _ => {
                            }
                        }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Error in received message: {:?}", e);
                    }
                }
            }
            Ok(None) => {
                tracing::warn!("Stream ended, exiting");
                break;
            }
            Err(_) => {
                tracing::warn!(
                    "No messages received in the last {} seconds",
                    timeout.as_secs()
                );
                tracing::info!("Waiting for messages...");
            }
        }
    }
}

```
