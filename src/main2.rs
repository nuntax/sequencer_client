use futures_util::stream::StreamExt;
use sequencer_client::reader::SequencerReader;
use sequencer_client::types::consensus::transactions::ArbTxEnvelope;
use sequencer_client::types::messages::Message;
#[tokio::main()]
async fn main() {
    let url = "wss://arb1-feed.arbitrum.io/feed";
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::TRACE) // Changed to DEBUG to see more logs
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
            Ok(Some(msg_result)) => match msg_result {
                Ok(msg) => {
                    tracing::info!("Received message: {:?}", msg.sequence_number);
                }
                Err(e) => {
                    tracing::error!("Error in received message: {:?}", e);
                }
            },
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
