use futures_util::stream::StreamExt;
use sequencer_client::reader::SequencerReader;

#[tokio::main()]
async fn main() {
    let url = "wss://arb1-feed.arbitrum.io/feed";
    let subscriber = tracing_subscriber::FmtSubscriber::new();
    // use that subscriber to process traces emitted after this point
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set global default subscriber");
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");
    let (reader, _) = SequencerReader::new(url).await;
    // tokio::spawn(async move {
    //     while let Some(msg) = receiver.recv().await {
    //         tracing::info!("Received message: {:?}", msg.block_number());
    //     }
    // });
    // reader.start_reading().await;
    let mut stream = reader.into_stream();
    while let Some(msg) = stream.next().await {
        tracing::info!("Received message: {:?}", msg.block_number());
    }
}
