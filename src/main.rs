use futures_util::stream::StreamExt;
use sequencer_client::reader::{SequencerMessage, SequencerReader};

#[tokio::main()]
async fn main() {
    let url = "wss://arb1-feed.arbitrum.io/feed";
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::TRACE)
        .finish(); // use that subscriber to process traces emitted after this point
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set global default subscriber");
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");
    let (reader, _) = SequencerReader::new(url).await;

    let mut stream = reader.into_stream();
    while let Some(msg) = stream.next().await {
        if let Err(e) = &msg {
            tracing::error!("Received error message: {:?}", e);
            continue;
        }
        let msg = msg.unwrap();
        if let SequencerMessage::L2Message {
            ref sequence_number,
            ref tx_hash,
            ref tx,
        } = msg
        {
            tracing::info!(
                "Received L2Message with sequence number: {}, transaction: {tx_hash}",
                msg.block_number(),
            );
        }
    }
}
