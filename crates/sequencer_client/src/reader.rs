use alloy_consensus::TxEip1559;
use alloy_consensus::TxEip2930;
use alloy_consensus::TxEip7702;
use alloy_consensus::TxLegacy;
use alloy_consensus::transaction::Recovered;
use alloy_consensus::transaction::RlpEcdsaDecodableTx;
use alloy_primitives::Address;
use alloy_primitives::B256;
use alloy_primitives::ChainId;
use alloy_primitives::FixedBytes;
use alloy_primitives::U256;
use alloy_primitives::hex::FromHex;
use arb_sequencer_consensus::transactions::ArbTxEnvelope;
use arb_sequencer_consensus::transactions::TxDeposit;
use arb_sequencer_consensus::transactions::batchpostingreport::BatchPostingReport;
use arb_sequencer_consensus::transactions::internal::ArbitrumInternalTx;
use arb_sequencer_consensus::transactions::submit_retryable::SubmitRetryableTx;
use arb_sequencer_network::sequencer::feed::BroadcastFeedMessage;
use arb_sequencer_network::sequencer::feed::L1Header;
use arb_sequencer_network::sequencer::feed::L1IncomingMessage;
use arb_sequencer_network::sequencer::feed::MessageType;
use arb_sequencer_network::sequencer::feed::Root;
use async_stream::stream;
use base64::prelude::*;
use eyre::Result;
use eyre::eyre;
use futures_util::StreamExt;
use futures_util::stream::Stream;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::io::{Cursor, Read};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_stream::StreamMap;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use rustls::pki_types::ServerName;

#[derive(Debug)]
pub struct SequencerMessage {
    /// The sequence number of the message.
    pub sequence_number: u64,
    /// The messages
    pub txs: Vec<ArbTxEnvelope>,
    /// timestamp of the message
    pub timestamp: u64,
    /// local instant when the message was received from websocket
    pub received_at: Instant,
    /// l1header
    pub l1_header: L1Header,
}

#[derive(Debug, Clone)]
pub struct BufferedMessage {
    pub message: BroadcastFeedMessage,
    pub received_at: Instant,
    pub version: u8,
}

/// Creates a WebSocket connection with optimized TCP settings.
/// Enables TCP_NODELAY to disable Nagle's algorithm for lower latency.
async fn connect_websocket_optimized(url: &str) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    // Parse URL to extract host and port
    let parsed_url = url::Url::parse(url)?;
    let host = parsed_url.host_str().ok_or_else(|| eyre!("Invalid URL: no host"))?;
    let port = parsed_url.port_or_known_default().ok_or_else(|| eyre!("Invalid URL: no port"))?;
    let is_wss = parsed_url.scheme() == "wss";

    // Create TCP connection
    let addr = format!("{}:{}", host, port);
    let tcp_stream = TcpStream::connect(&addr).await?;

    // Enable TCP_NODELAY for low-latency (disables Nagle's algorithm)
    tcp_stream.set_nodelay(true)?;

    // Optional: Set larger TCP buffers for better throughput
    // Commented out by default, uncomment if needed
    // let _ = tcp_stream.set_recv_buffer_size(2_097_152); // 2MB
    // let _ = tcp_stream.set_send_buffer_size(2_097_152); // 2MB

    // Wrap in TLS if needed
    let stream: MaybeTlsStream<TcpStream> = if is_wss {
        // Load native root certificates
        let mut root_store = rustls::RootCertStore::empty();
        let certs = rustls_native_certs::load_native_certs();

        for cert in certs.certs {
            root_store.add(cert)
                .map_err(|e| eyre!("Failed to add cert: {}", e))?;
        }

        let config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
        let server_name = ServerName::try_from(host)
            .map_err(|_| eyre!("Invalid DNS name: {}", host))?
            .to_owned();

        let tls_stream = connector.connect(server_name, tcp_stream).await?;
        MaybeTlsStream::Rustls(tls_stream)
    } else {
        MaybeTlsStream::Plain(tcp_stream)
    };

    // Perform WebSocket handshake
    let request = url.into_client_request()?;
    let (ws_stream, _) = tokio_tungstenite::client_async(request, stream).await?;

    Ok(ws_stream)
}

/// SequencerReader is the main struct of this library.
/// It is used to connect to the Arbitrum sequencer feed and read messages from it.
#[derive(Debug)]
pub struct SequencerReader {
    /// The WebSocket stream used to connect to the Arbitrum sequencer feed.
    stream_map: StreamMap<u8, WebSocketStream<MaybeTlsStream<TcpStream>>>,
    /// The url
    url: String,
    /// The chain ID of the Arbitrum network.
    pub chain_id: ChainId,
    /// The buffer for out-of-order messages.
    buffer: Arc<Mutex<BTreeMap<u64, BufferedMessage>>>,
}

impl SequencerReader {
    /// Creates a new SequencerReader and connects to the given URL.
    /// Returns a SequencerReader instance.
    /// To use the reader, call `into_stream` to get a stream of SequencerMessage results.
    /// # Arguments
    /// * `url` - The URL of the Arbitrum sequencer feed WebSocket
    /// * `chain_id` - The chain ID of the Arbitrum network.
    /// * `connections` - The number of parallel WebSocket connections to establish. (Recommended: less than 10)
    pub async fn new(url: &str, chain_id: ChainId, connections: u8) -> Self {
        let connections = connections.max(1); // Ensure at least 1 connection
        let mut stream_map = StreamMap::new();

        for i in 0..connections {
            let ws_stream = connect_websocket_optimized(url)
                .await
                .expect("Failed to connect");
            stream_map.insert(i, ws_stream);
        }
        SequencerReader {
            stream_map,
            url: url.to_string(),
            chain_id,
            buffer: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    /// Converts the SequencerReader into a stream of SequencerMessage results.
    pub fn into_stream(mut self) -> impl Stream<Item = Result<SequencerMessage>> + Unpin {
        tracing::debug!(
            "Creating sequencer message stream with {} connections",
            self.stream_map.len()
        );
        let buffer_for_reader = self.buffer.clone();
        tokio::spawn(async move {
            let mut dedup_map: HashSet<u64> = std::collections::HashSet::new();
            tracing::debug!("Stream started, waiting for messages");

            while let Some(message) = self.stream_map.next().await {
                if message.1.is_err() {
                    tracing::error!("Error receiving message: {:?}", message.1.err().unwrap());
                    // reconnect the websocket
                    tracing::debug!("Reconnecting websocket for connection {}", message.0);
                    let ws_stream = connect_websocket_optimized(&self.url)
                        .await
                        .expect("Failed to reconnect");
                    self.stream_map.insert(message.0, ws_stream);
                    continue;
                }
                let message = message.1.unwrap();
                let msg_text = match message.into_text() {
                    Ok(text) => text,
                    Err(e) => {
                        tracing::error!("Failed to convert message to text: {}", e);
                        continue;
                    }
                };
                if msg_text.is_empty() {
                    tracing::debug!("Received empty message");
                    continue;
                }
                let (messages, version) = match serde_json::from_str::<Root>(&msg_text) {
                    Ok(r) => {
                        if let Some(msgs) = r.messages {
                            (msgs, r.version)
                        } else {
                            continue;
                        }
                    }
                    Err(e) => {
                        //debug trace here since this indicates an empty message in almost all cases
                        tracing::debug!("Failed to parse JSON: {}, text: {}", e, msg_text);
                        continue;
                    }
                };
                let message_receive = Instant::now();
                for msg in messages {
                    let seq_num = msg.sequence_number;
                    if dedup_map.contains(&seq_num) {
                        continue;
                    }
                    dedup_map.insert(seq_num);
                    let mut buf = buffer_for_reader.lock().await;
                    // store the message together with the receive timestamp
                    buf.insert(
                        seq_num,
                        BufferedMessage {
                            message: msg,
                            received_at: message_receive,
                            version,
                        },
                    );
                }
            }
            tracing::debug!("WebSocket stream ended");
        });
        Box::pin(stream! {
                loop {
                    let mut buf = self.buffer.lock().await;
                        if buf.is_empty() {
                            drop(buf); // release lock before sleeping
                            continue;
                        }
                        let first_key = *buf.keys().next().unwrap();
                        let BufferedMessage { message, received_at, version } = buf.remove(&first_key).unwrap();
                        drop(buf); // release lock before processing

                        let header = message.message_with_meta_data.l1_incoming_message.header.clone();
                        let l1_msg = message.message_with_meta_data.l1_incoming_message;
                        match parse_message(l1_msg, self.chain_id, version,
                        ) {
                        Ok(messages) => {
                                yield Ok(SequencerMessage {
                                    sequence_number: message.sequence_number,
                                    txs: messages,
                                    timestamp: header.timestamp,
                                    received_at,
                                    l1_header: L1Header::from_header(&header, message.message_with_meta_data.delayed_messages_read).unwrap(),
                                });
                        }
                        Err(e) => {
                            tracing::error!("Failed to parse message: {}", e);
                            continue;
                        }
                    }
                }
        })
    }
}

pub fn parse_message(
    msg: L1IncomingMessage,
    chain_id: ChainId,
    version: u8,
) -> Result<Vec<ArbTxEnvelope>> {
    let msg_type = MessageType::from_u8(msg.header.kind);
    tracing::debug!("Parsing message type: {:?}", msg_type);

    match msg_type {
        MessageType::L2Message => {
            tracing::debug!("Decoding L2Message base64 content");
            let mut buffer = match BASE64_STANDARD.decode(msg.l2msg) {
                Ok(buf) => {
                    tracing::debug!("Successfully decoded base64 of length: {}", buf.len());
                    buf
                }
                Err(e) => {
                    tracing::error!("Failed to decode base64: {}", e);
                    return Err(e.into());
                }
            };

            match parse_l2_msg(buffer.as_mut_slice(), 0) {
                Ok(txs) => {
                    tracing::debug!("Successfully parsed {} L2 transactions", txs.len());
                    Ok(txs)
                }
                Err(e) => {
                    tracing::error!("Failed to parse L2 message: {}", e);
                    Err(e)
                }
            }
        }
        MessageType::EndOfBlock => {
            todo!()
        }
        MessageType::EthDeposit => {
            let mut buffer_vec = BASE64_STANDARD.decode(msg.l2msg)?;
            let buffer = buffer_vec.as_mut_slice();
            tracing::debug!("Buffer: {}", hex::encode(&buffer));
            let tx = TxDeposit::decode_fields_sequencer(
                &mut &*buffer,
                U256::from(chain_id),
                FixedBytes::from_hex(
                    msg.header
                        .request_id
                        .as_str()
                        .ok_or(eyre!("failed to deserialize request_id"))?,
                )?,
                msg.header.sender.parse()?,
            )?;
            tracing::debug!("Parsed TxDeposit: {:?}", tx);
            tracing::debug!("TxDeposit hash: {}", tx.tx_hash());
            Ok(vec![ArbTxEnvelope::DepositTx(tx)])
        }
        MessageType::SubmitRetryable => {
            tracing::debug!("Retyrable message: {:?}", msg);
            let mut buffer_vec = BASE64_STANDARD.decode(msg.l2msg)?;
            let buffer = buffer_vec.as_mut_slice();
            //log the whole message and the buffer
            let tx = parse_submit_retryable(
                &mut &*buffer,
                chain_id,
                Address::from_str(&msg.header.sender).unwrap(),
                B256::from_hex(
                    msg.header
                        .request_id
                        .as_str()
                        .ok_or(eyre!("failed to deserialize request_id"))?,
                )?,
                U256::from(
                    msg.header
                        .base_fee_l1
                        .as_u64()
                        .ok_or(eyre!("failed to deserialize base fee l1"))?,
                ),
            )?;
            Ok(vec![ArbTxEnvelope::SubmitRetryableTx(tx.inner().clone())])
        }
        MessageType::BatchPostingReport => {
            let mut buffer_vec = BASE64_STANDARD.decode(msg.l2msg)?;
            let buffer = buffer_vec.as_mut_slice();
            tracing::debug!("BatchPostingReport Buffer: {}", hex::encode(&buffer));
            tracing::debug!(
                "Additional args: chain_id: {}, version: {}, batch_data_stats: {:?}, legacy_batch_gas_cost: {:?}",
                chain_id,
                version,
                msg.batch_data_stats,
                msg.legacy_batch_gas_cost
            );
            let report = BatchPostingReport::decode_fields_sequencer(
                &mut &*buffer,
                chain_id,
                version.into(),
                msg.batch_data_stats,
                msg.legacy_batch_gas_cost,
            )?;
            tracing::debug!("Parsed BatchPostingReport: {:?}", report);
            let internal_tx = ArbitrumInternalTx::BatchPostingReport(report);
            Ok(vec![internal_tx.into()])
        }
        _ => {
            tracing::warn!("not yet supported message type: {:?}", msg_type);
            Ok(Vec::new())
        }
    }
}

pub fn parse_submit_retryable(
    msg: &mut &[u8],
    chain_id: ChainId,
    sender: Address,
    request_id: B256,
    l1_base_fee: U256,
) -> Result<Recovered<SubmitRetryableTx>> {
    let tx = SubmitRetryableTx::decode_fields_sequencer(
        msg,
        U256::from(chain_id),
        U256::from_be_bytes(request_id.0),
        sender,
        l1_base_fee,
    )?;
    let recovered = Recovered::new_unchecked(tx, sender);

    tracing::debug!(
        "Parsed TxSubmitRetryable: chain_id: {}, request_id: {}, sender: {}",
        chain_id,
        request_id,
        sender
    );
    Ok(recovered)
}

const MAX_BATCH_DEPTH: u32 = 16;
const MAX_L2_MESSAGE_SIZE: u64 = 256 * 1024; // 256KB

#[derive(Debug, PartialEq)]

/// L2MessageKind represents the kind of message that can be received from the Arbitrum sequencer.
pub enum L2MessageKind {
    /// Unsigned user transaction. Only here for completeness.
    UnsignedUserTx = 0,
    /// Contract transaction. Only here for completeness.
    ContractTx = 1,
    /// Non-mutating call. Not implemented in Arbitrum reference implementation, only here for completeness.
    NonmutatingCall = 2,
    /// Batch transaction, this is a message that contains multiple transactions.
    Batch = 3,
    /// Signed transaction, this is a message that contains a signed transaction.
    SignedTx = 4,
    /// Heartbeat message. Deprecated, not used in Arbitrum anymore. Only here for completeness.
    Heartbeat = 5,
    /// Signed compressed transaction. Not implemented in Arbitrum reference implementation. Only here for completeness.
    SignedCompressedTx = 6,
}

impl TryFrom<u8> for L2MessageKind {
    type Error = eyre::Report;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(L2MessageKind::UnsignedUserTx),
            1 => Ok(L2MessageKind::ContractTx),
            2 => Ok(L2MessageKind::NonmutatingCall),
            3 => Ok(L2MessageKind::Batch),
            4 => Ok(L2MessageKind::SignedTx),
            5 => Ok(L2MessageKind::Heartbeat),
            6 => Ok(L2MessageKind::SignedCompressedTx),
            _ => Err(eyre::eyre!("Unsupported L2 message kind: {}", value)),
        }
    }
}

fn parse_l2_msg(bytes: &mut [u8], depth: u32) -> Result<Vec<ArbTxEnvelope>> {
    if depth >= MAX_BATCH_DEPTH {
        return Err(eyre::eyre!("Maximum batch depth exceeded: {}", depth));
    }

    // get kind byte and cast to enum
    let kind = L2MessageKind::try_from(bytes[0])?;
    let mut transactions: Vec<ArbTxEnvelope> = Vec::new();

    match kind {
        L2MessageKind::SignedTx => {
            let tx = parse_raw_tx(&bytes[1..])?;
            transactions.push(tx);
        }
        L2MessageKind::Heartbeat => {
            // deprecated heartbeat message, we can ignore it
        }
        L2MessageKind::NonmutatingCall | L2MessageKind::SignedCompressedTx => {
            return Err(eyre::eyre!("Unsupported L2 message kind: {:?}", kind));
        }

        L2MessageKind::Batch => {
            let mut cursor = Cursor::new(&bytes[1..]); // skip the kind byte
            loop {
                // first 8 bytes after the kind are
                let mut length_buf = [0u8; 8];
                match cursor.read_exact(&mut length_buf) {
                    Ok(_) => {}
                    Err(_) => break, //end of batch
                }

                let msg_len = u64::from_be_bytes(length_buf);
                if msg_len > MAX_L2_MESSAGE_SIZE {
                    return Err(eyre::eyre!(
                        "L2 message size exceeds maximum allowed size: {} > {}",
                        msg_len,
                        MAX_L2_MESSAGE_SIZE
                    ));
                }

                let mut msg_buf = vec![0u8; msg_len as usize];
                if cursor.read_exact(&mut msg_buf).is_err() {
                    break;
                }

                // recursive here for nested batch tx
                let nested_txs = parse_l2_msg(&mut msg_buf, depth + 1)?;
                transactions.extend(nested_txs);
            }
        }

        _ => {
            return Err(eyre!("Unsupported L2 message kind: {:?}", kind));
        }
    }

    Ok(transactions)
}

/// TxType represents the type of transaction that can be received from the Arbitrum sequencer.
pub enum TxType {
    /// Legacy transaction. Indicated by kind byte bigger than 0x7f.
    Legacy,
    /// EIP-2930 transaction. Indicated by kind byte 0x01.
    Eip2930 = 1,
    /// EIP-1559 transaction. Indicated by kind byte 0x02.
    Eip1559 = 2,
    /// EIP-7702 transaction. Indicated by kind byte 0x04.
    Eip7702 = 4,
}

impl TxType {
    /// Converts a u8 value to a TxType.
    pub fn from_u8(value: u8) -> Result<Self> {
        match value {
            x if x > 0x7f => Ok(TxType::Legacy),
            1 => Ok(TxType::Eip2930),
            2 => Ok(TxType::Eip1559),
            4 => Ok(TxType::Eip7702),
            _ => Err(eyre::eyre!(
                "Invalid transaction type: {}. Expected 0x01, 0x02, 0x04 or bigger than 0x7f.",
                value
            )),
        }
    }
}

fn parse_raw_tx(bytes: &[u8]) -> Result<ArbTxEnvelope> {
    let tx_type = bytes.first().ok_or(eyre!("Missing transaction type"))?;
    let tx_type = TxType::from_u8(*tx_type)?;
    let tx: ArbTxEnvelope = match tx_type {
        TxType::Legacy => ArbTxEnvelope::Legacy(TxLegacy::rlp_decode_signed(&mut &bytes[0..])?),
        TxType::Eip2930 => ArbTxEnvelope::Eip2930(TxEip2930::rlp_decode_signed(&mut &bytes[1..])?),
        TxType::Eip1559 => ArbTxEnvelope::Eip1559(TxEip1559::rlp_decode_signed(&mut &bytes[1..])?),
        TxType::Eip7702 => ArbTxEnvelope::Eip7702(TxEip7702::rlp_decode_signed(&mut &bytes[1..])?),
    };

    Ok(tx)
}
