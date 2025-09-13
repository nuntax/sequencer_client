use alloy::consensus::transaction::Recovered;
use alloy::consensus::transaction::RlpEcdsaDecodableTx;
use alloy::consensus::transaction::TxLegacy;
use alloy::hex::FromHex;
use alloy_consensus::TxEip1559;
use alloy_consensus::TxEip2930;
use alloy_consensus::TxEip7702;
use alloy_primitives::Address;
use alloy_primitives::B256;
use alloy_primitives::ChainId;
use alloy_primitives::FixedBytes;
use alloy_primitives::U256;
use async_stream::stream;
use base64::prelude::*;
use eyre::Result;
use eyre::eyre;
use futures_util::StreamExt;
use futures_util::stream::Stream;
use serde_derive::Deserialize;
use serde_derive::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::io::{Cursor, Read};
use std::str::FromStr;
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::types::transactions::ArbTxEnvelope;
use crate::types::transactions::TxDeposit;
use crate::types::transactions::TxSubmitRetryable;
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Root {
    version: u8,
    messages: Vec<BroadcastFeedMessage>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BroadcastFeedMessage {
    sequence_number: u64,
    #[serde(rename = "message")]
    message_with_meta_data: MessageWithMetadata,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessageWithMetadata {
    #[serde(rename = "message")]
    l1_incoming_message: L1IncomingMessage,
    delayed_messages_read: u64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct L1IncomingMessage {
    header: Header,
    #[serde(rename = "l2Msg")]
    l2msg: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Header {
    kind: u8,
    sender: String,
    block_number: u64,
    timestamp: u64,
    request_id: Value,
    base_fee_l1: Value,
}
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    L2Message = 3,
    EndOfBlock = 6,
    L2FundedByL1 = 7,
    RollupEvent = 8,
    SubmitRetryable = 9,
    BatchForGasEstimation = 10,
    Initialize = 11,
    EthDeposit = 12,
    BatchPostingReport = 13,
    Invalid = 0xFF,
}
impl MessageType {
    pub fn from_u8(value: u8) -> Self {
        match value {
            3 => MessageType::L2Message,
            6 => MessageType::EndOfBlock,
            7 => MessageType::L2FundedByL1,
            8 => MessageType::RollupEvent,
            9 => MessageType::SubmitRetryable,
            10 => MessageType::BatchForGasEstimation,
            11 => MessageType::Initialize,
            12 => MessageType::EthDeposit,
            13 => MessageType::BatchPostingReport,
            _ => MessageType::Invalid,
        }
    }
    #[allow(dead_code)]
    pub fn to_u8(&self) -> u8 {
        match self {
            MessageType::L2Message => 3,
            MessageType::EndOfBlock => 6,
            MessageType::L2FundedByL1 => 7,
            MessageType::RollupEvent => 8,
            MessageType::SubmitRetryable => 9,
            MessageType::BatchForGasEstimation => 10,
            MessageType::Initialize => 11,
            MessageType::EthDeposit => 12,
            MessageType::BatchPostingReport => 13,
            MessageType::Invalid => 0xFF,
        }
    }
}

#[derive(Debug)]
pub struct SequencerMessage {
    /// The sequence number of the message.
    pub sequence_number: u64,
    /// The transaction
    pub tx: ArbTxEnvelope,
}

/// SequencerReader is the main struct of this library.
/// It is used to connect to the Arbitrum sequencer feed and read messages from it.
/// It then forwards the messages to a tokio mpsc, which can be used to receive the messages.
#[derive(Debug)]
pub struct SequencerReader {
    /// The WebSocket stream used to connect to the Arbitrum sequencer feed.
    pub ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    /// The url
    url: String,
    /// The chain ID of the Arbitrum network.
    pub chain_id: ChainId,
    /// Number of parallel connections to maintain for lower latency
    pub connections: u32,
}

impl SequencerReader {
    /// Creates a new SequencerReader and connects to the given URL.
    /// Returns a tuple containing the SequencerReader and the receiver part of the mpsc channel.
    pub async fn new(url: &str, chain_id: ChainId, connections: u32) -> Self {
        let connections = connections.max(1); // Ensure at least 1 connection
        let (ws_stream, _) = tokio_tungstenite::connect_async(url)
            .await
            .expect("Failed to connect");

        SequencerReader {
            ws_stream,
            url: url.to_string(),
            chain_id,
            connections,
        }
    }

    /// Converts the SequencerReader into a stream of SequencerMessage results.
    /// This provides an alternative to using the mpsc channel approach.
    pub fn into_stream(mut self) -> impl Stream<Item = Result<SequencerMessage>> + Unpin {
        tracing::debug!(
            "Creating sequencer message stream with {} connections",
            self.connections
        );

        Box::pin(stream! {
            let mut dedup_map: HashSet<u64> = std::collections::HashSet::new();
            tracing::debug!("Stream started, waiting for messages");

            if self.connections == 1 {
                // Single connection mode - use existing logic
                while let Some(message) = self.ws_stream.next().await {
                    tracing::debug!("Received websocket message");

                    match message {
                        Ok(raw_msg) => {
                            if raw_msg.is_empty() {
                                tracing::debug!("Received empty message, skipping");
                                continue;
                            }

                            let as_txt = match raw_msg.into_text() {
                                Ok(txt) => {
                                    tracing::debug!("Parsed websocket message text of length: {}", txt.len());
                                    txt
                                }
                                Err(e) => {
                                    tracing::error!("Failed to parse websocket message text: {}", e);
                                    yield Err(e.into());
                                    continue;
                                }
                            };

                            let structured_data: Root = match serde_json::from_slice::<Root>(as_txt.as_bytes()) {
                                Ok(ret) => {
                                    tracing::debug!("Parsed JSON with {} messages", ret.messages.len());
                                    ret
                                }
                                Err(e) => {
                                    tracing::error!("Failed to parse JSON: {}", e);
                                    tracing::error!("message: {}", as_txt);
                                    continue;
                                }
                            };

                            for msg in structured_data.messages {
                                tracing::debug!("Processing message with sequence number: {}", msg.sequence_number);

                                if msg.message_with_meta_data.l1_incoming_message.l2msg.len() > 256 * 1024 {
                                    tracing::debug!("Message too large, skipping");
                                    continue;
                                }

                                if dedup_map.contains(&msg.sequence_number) {
                                    tracing::debug!("Duplicate message, skipping");
                                    continue;
                                }

                                dedup_map.insert(msg.sequence_number);
                                tracing::debug!("Message type: {}", msg.message_with_meta_data.l1_incoming_message.header.kind);

                                match parse_message(
                                    msg.message_with_meta_data.l1_incoming_message.clone(),
                                    self.chain_id,
                                ) {
                                    Ok(txs) => {
                                        tracing::debug!("Successfully parsed {} transactions", txs.len());
                                        for tx in txs {
                                            tracing::debug!("Yielding transaction");
                                            yield Ok(SequencerMessage {
                                                sequence_number: msg.sequence_number,
                                                tx,
                                            });
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            "Failed to parse message with sequence number {}: {:?}",
                                            msg.sequence_number,
                                            e
                                        );
                                        yield Err(e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("Error receiving message: {}", e);
                            yield Err(e.into());

                            tracing::error!("WebSocket Connection interrupted, attempting reconnect.");
                            match tokio_tungstenite::connect_async(self.url.clone()).await {
                                Ok((ws_stream, _)) => {
                                    tracing::debug!("Successfully reconnected to WebSocket");
                                    self.ws_stream = ws_stream;
                                }
                                Err(reconnect_err) => {
                                    tracing::error!("Failed to reconnect: {}", reconnect_err);
                                    yield Err(reconnect_err.into());
                                    break;
                                }
                            }
                        }
                    }
                }
            } else {
                // Multiple connections mode
                use futures_util::stream::select_all;

                // Create additional connections
                let mut streams = Vec::new();
                streams.push(self.ws_stream);

                for i in 1..self.connections {
                    match tokio_tungstenite::connect_async(self.url.clone()).await {
                        Ok((ws_stream, _)) => {
                            tracing::debug!("Created additional connection {}", i);
                            streams.push(ws_stream);
                        }
                        Err(e) => {
                            tracing::error!("Failed to create connection {}: {}", i, e);
                            yield Err(e.into());
                            return;
                        }
                    }
                }

                let mut combined_stream = select_all(streams);

                while let Some(message) = combined_stream.next().await {
                    match message {
                        Ok(raw_msg) => {
                            if raw_msg.is_empty() {
                                tracing::debug!("Received empty message, skipping");
                                continue;
                            }

                            let as_txt = match raw_msg.into_text() {
                                Ok(txt) => {
                                    tracing::debug!("Parsed websocket message text of length: {}", txt.len());
                                    txt
                                }
                                Err(e) => {
                                    tracing::error!("Failed to parse websocket message text: {}", e);
                                    yield Err(e.into());
                                    continue;
                                }
                            };

                            let structured_data: Root = match serde_json::from_slice::<Root>(as_txt.as_bytes()) {
                                Ok(ret) => {
                                    tracing::debug!("Parsed JSON with {} messages", ret.messages.len());
                                    ret
                                }
                                Err(e) => {
                                    tracing::error!("Failed to parse JSON: {}", e);
                                    tracing::error!("message: {}", as_txt);
                                    continue;
                                }
                            };

                            for msg in structured_data.messages {
                                tracing::debug!("Processing message with sequence number: {}", msg.sequence_number);

                                if msg.message_with_meta_data.l1_incoming_message.l2msg.len() > 256 * 1024 {
                                    tracing::debug!("Message too large, skipping");
                                    continue;
                                }

                                if dedup_map.contains(&msg.sequence_number) {
                                    tracing::debug!("Duplicate message sequence {}, skipping", msg.sequence_number);
                                    continue;
                                }

                                dedup_map.insert(msg.sequence_number);
                                tracing::debug!("Message type: {}", msg.message_with_meta_data.l1_incoming_message.header.kind);

                                match parse_message(
                                    msg.message_with_meta_data.l1_incoming_message.clone(),
                                    self.chain_id,
                                ) {
                                    Ok(txs) => {
                                        tracing::debug!("Successfully parsed {} transactions", txs.len());
                                        for tx in txs {
                                            tracing::debug!("Yielding transaction");
                                            yield Ok(SequencerMessage {
                                                sequence_number: msg.sequence_number,
                                                tx,
                                            });
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            "Failed to parse message with sequence number {}: {:?}",
                                            msg.sequence_number,
                                            e
                                        );
                                        yield Err(e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("Error receiving message: {}", e);
                            yield Err(e.into());

                            tracing::error!("WebSocket Connection interrupted, attempting reconnect.");
                            match tokio_tungstenite::connect_async(self.url.clone()).await {
                                Ok((ws_stream, _)) => {
                                    tracing::debug!("Successfully reconnected to WebSocket");
                                    combined_stream.push(ws_stream);
                                }
                                Err(reconnect_err) => {
                                    tracing::error!("Failed to reconnect: {}", reconnect_err);
                                    yield Err(reconnect_err.into());
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            tracing::debug!("WebSocket stream ended");
        })
    }
}

pub fn parse_message(msg: L1IncomingMessage, chain_id: ChainId) -> Result<Vec<ArbTxEnvelope>> {
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
            let mut buffer_vec = BASE64_STANDARD.decode(msg.l2msg.clone())?;
            let buffer = buffer_vec.as_mut_slice();
            //log the whole message and the buffer
            tracing::debug!("Retyrable message: {:?}", msg);
            tracing::debug!("Retryable message buffer: {}", hex::encode(&buffer));

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
) -> Result<Recovered<TxSubmitRetryable>> {
    let tx = TxSubmitRetryable::decode_fields_sequencer(
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
