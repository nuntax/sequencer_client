use sequencer_client::reader::SequencerReader;
#[tokio::main()]
async fn main() {
    let url = "wss://arb1-feed.arbitrum.io/feed";
    let (mut reader, mut receiver) = SequencerReader::new(url).await;
    tokio::spawn(async move {
        while let Some(msg) = receiver.recv().await {
            println!("Received message: {:?}", msg.sequence_number());
        }
    });
    reader.start_reading().await;
}
