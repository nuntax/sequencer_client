use sequencer_reader::reader::SequencerReader;
#[tokio::main()]
async fn main() {
    let url = "wss://arb1-feed.arbitrum.io/feed"; // Replace with your WebSocket
    let (mut reader, mut receiver) = SequencerReader::new(url).await;
    tokio::spawn(async move {
        while let Some(msg) = receiver.recv().await {
            println!("Received message: {:?}", msg);
        }
    });
    reader.start_reading().await;
}
