use alloy::consensus::Transaction;
use alloy::consensus::transaction::Recovered;
use alloy::consensus::transaction::RlpEcdsaDecodableTx;

use alloy_primitives::SignatureError;
use alloy_primitives::TxHash;
use base64::prelude::*;
use futures_util::Stream;
use futures_util::StreamExt;
use std::collections::HashSet;
use std::io::{Cursor, Read};
use std::pin::Pin;

use serde_derive::Deserialize;
use serde_derive::Serialize;
use serde_json::Value;
use tokio::{
    net::TcpStream,
    sync::mpsc::{Receiver, Sender, channel},
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

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
    l1_incoming_message: L1IncomingMessageHeader,
    delayed_messages_read: u64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct L1IncomingMessageHeader {
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

pub enum SequencerMessage {
    /// Represents a message containing a single transaction.
    L2Message {
        sequence_number: u64,
        tx_hash: TxHash,
        tx: Recovered<Box<dyn Transaction + Send + Sync>>,
    },
    EndOfBlock {
        sequence_number: u64,
    },
    /// Represents all other messages that are not transactions.
    Other {
        sequence_number: u64,
    },
}
#[derive(Debug)]
pub enum SequencerMessageError {
    /// Error while decoding a message from the Arbitrum sequencer.
    MessageDecodingError(MessageDecodingError),
    /// Error while decoding a transaction from the Arbitrum sequencer.
    TransactionDecodingError(TransactionDecodingError),
    /// Error indicating an issue with the WebSocket connection.
    WebSocketError(tokio_tungstenite::tungstenite::Error),
}

impl SequencerMessage {
    /// Returns the block number corresponding to the sequence number.
    ///
    /// This is calculated by adding the sequence number to the Arbitrum genesis block number (22207817).
    ///
    /// <div class="warning">On Nova chains the sequence_number variable is the block number.</div>
    #[inline]
    pub fn block_number(&self) -> u64 {
        match self {
            SequencerMessage::L2Message {
                sequence_number, ..
            } => sequence_number + 22207817,
            SequencerMessage::EndOfBlock { sequence_number } => *sequence_number + 22207817,
            SequencerMessage::Other { sequence_number } => *sequence_number + 22207817,
        }
    }
    /// Returns the sequence number of the message.
    #[inline]
    pub fn sequence_number(&self) -> u64 {
        match self {
            SequencerMessage::L2Message {
                sequence_number, ..
            } => *sequence_number,
            SequencerMessage::EndOfBlock { sequence_number } => *sequence_number,
            SequencerMessage::Other { sequence_number } => *sequence_number,
        }
    }
}

/// SequencerReader is the main struct of this library.
/// It is used to connect to the Arbitrum sequencer feed and read messages from it.
/// It then forwards the messages to a tokio mpsc, which can be used to receive the messages.
#[derive(Debug)]
pub struct SequencerReader {
    /// The WebSocket stream used to connect to the Arbitrum sequencer feed.
    pub ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    /// The sender part of the mpsc channel used to forward messages to the receiver.
    pub tx: Sender<Result<SequencerMessage, SequencerMessageError>>,
    /// The url
    url: String,
}

impl SequencerReader {
    /// Creates a new SequencerReader and connects to the given URL.
    /// Returns a tuple containing the SequencerReader and the receiver part of the mpsc channel.
    pub async fn new(
        url: &str,
    ) -> (
        Self,
        Receiver<Result<SequencerMessage, SequencerMessageError>>,
    ) {
        let (ws_stream, _) = tokio_tungstenite::connect_async(url)
            .await
            .expect("Failed to connect");
        let (tx, rx): (
            Sender<Result<SequencerMessage, SequencerMessageError>>,
            Receiver<Result<SequencerMessage, SequencerMessageError>>,
        ) = channel(1000);
        (
            SequencerReader {
                ws_stream,
                tx,
                url: url.to_string(),
            },
            rx,
        )
    }
    /// Starts reading messages from the Arbitrum sequencer feed.
    /// This method runs in a loop and listens for incoming messages.
    /// It then parses the messages and forwards single transactions to the mpsc channel.
    pub async fn start_reading(&mut self) {
        // deduplicate messages based on sequence number
        let mut dedup_map: HashSet<u64> = std::collections::HashSet::new();

        while let Some(message) = self.ws_stream.next().await {
            match message {
                Ok(raw_msg) => {
                    if raw_msg.is_empty() {
                        continue;
                    }
                    let as_txt = raw_msg.into_text().unwrap();
                    let structured_data: Root = match serde_json::from_slice(as_txt.as_bytes()) {
                        Ok(ret) => ret,
                        Err(_) => {
                            continue;
                        }
                    };
                    structured_data.messages.iter().for_each(|msg| {
                        if msg.message_with_meta_data.l1_incoming_message.l2msg.len() > 256 * 1024 {
                            return;
                        }

                        if dedup_map.contains(&msg.sequence_number) {
                            return;
                        }

                        dedup_map.insert(msg.sequence_number);

                        match msg.message_with_meta_data.l1_incoming_message.header.kind {
                            3 => {
                                let txs = match BASE64_STANDARD.decode(
                                    msg.message_with_meta_data.l1_incoming_message.l2msg.clone(),
                                ) {
                                    Ok(mut bytes) => match parse_l2_msg(&mut bytes, 0) {
                                        Ok(txs) => {
                                            if txs.is_empty() {
                                                if let Err(e) = self.tx.try_send(Err(SequencerMessageError::MessageDecodingError(MessageDecodingError::UnsupportedMessageKind(3)))) {
                                                    tracing::error!("Failed to send error to receiver: {}", e);
                                                }
                                                return;
                                            }
                                            txs
                                        }
                                        Err(e) => {
                                            if let Err(send_err) = self.tx.try_send(Err(SequencerMessageError::MessageDecodingError(e))) {
                                                tracing::error!("Failed to send error to receiver: {}", send_err);
                                            }
                                            return;
                                        }
                                    },
                                    Err(_) => {
                                        if let Err(e) = self.tx.try_send(Err(SequencerMessageError::MessageDecodingError(MessageDecodingError::UnsupportedMessageKind(3)))) {
                                            tracing::error!("Failed to send error to receiver: {}", e);
                                        }
                                        return;
                                    }
                                };
                                for (tx_hash, tx) in txs {
                                    let msg = SequencerMessage::L2Message {
                                        sequence_number: msg.sequence_number,
                                        tx_hash,
                                        tx,
                                    };
                                    if let Err(e) = self.tx.try_send(Ok(msg)) {
                                        tracing::error!(
                                            "Failed to send message to receiver: {}",
                                            e
                                        );
                                        return;
                                    }
                                }
                            }
                            6 => {
                                // end of block message, this one is important for producing blocks
                                let msg = SequencerMessage::EndOfBlock {
                                    sequence_number: msg.sequence_number,
                                };
                                if let Err(e) = self.tx.try_send(Ok(msg)) {
                                    tracing::error!(
                                        "Failed to send end of block message to receiver: {}",
                                        e
                                    );
                                }
                            }
                            _ => {
                                // this is not a transaction, just forward it
                                let sequencer_msg = SequencerMessage::Other {
                                    sequence_number: msg.sequence_number,
                                };
                                if let Err(e) = self.tx.try_send(Ok(sequencer_msg)) {
                                    tracing::error!(
                                        "Failed to send non-transaction message to receiver: {}",
                                        e
                                    );
                                }
                            }
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("Error receiving message: {}", e);

                    if let Err(send_err) = self
                        .tx
                        .try_send(Err(SequencerMessageError::WebSocketError(e)))
                    {
                        tracing::error!("Failed to send websocket error to receiver: {}", send_err);
                    }
                    tracing::error!("WebSocket Connection interrupted, attempting reconnect.");
                    (self.ws_stream, _) = tokio_tungstenite::connect_async(self.url.clone())
                        .await
                        .expect("Failed to connect");
                }
            }
        }
    }

    /// Returns a stream of SequencerMessage without blocking the current thread.
    /// This provides the same functionality as start_reading but as a stream.
    pub fn into_stream(
        mut self,
    ) -> Pin<Box<dyn Stream<Item = Result<SequencerMessage, SequencerMessageError>> + Send>> {
        Box::pin(async_stream::stream! {
            let mut dedup_map: HashSet<u64> = std::collections::HashSet::new();

            while let Some(message) = self.ws_stream.next().await {
                match message {
                    Ok(raw_msg) => {
                        if raw_msg.is_empty() {
                            continue;
                        }
                        let as_txt = raw_msg.into_text().unwrap();
                        let structured_data: Root = match serde_json::from_slice(as_txt.as_bytes()) {
                            Ok(ret) => ret,
                            Err(_) => {
                                continue;
                            }
                        };

                        for msg in structured_data.messages.iter() {
                            if msg.message_with_meta_data.l1_incoming_message.l2msg.len() > 256 * 1024 {
                                continue;
                            }

                            if dedup_map.contains(&msg.sequence_number) {
                                continue;
                            }

                            dedup_map.insert(msg.sequence_number);

                            match msg.message_with_meta_data.l1_incoming_message.header.kind {
                                3 => {
                                    let txs = match BASE64_STANDARD.decode(
                                        msg.message_with_meta_data.l1_incoming_message.l2msg.clone(),
                                    ) {
                                        Ok(mut bytes) => match parse_l2_msg(&mut bytes, 0) {
                                            Ok(txs) => {
                                                if txs.is_empty() {
                                                    yield Err(SequencerMessageError::MessageDecodingError(MessageDecodingError::UnsupportedMessageKind(3)));
                                                    continue;
                                                }
                                                txs
                                            }
                                            Err(e) => {
                                                yield Err(SequencerMessageError::MessageDecodingError(e));
                                                continue;
                                            }
                                        },
                                        Err(_) => {
                                            yield Err(SequencerMessageError::MessageDecodingError(MessageDecodingError::UnsupportedMessageKind(3)));
                                            continue;
                                        }
                                    };
                                    for (tx_hash, tx) in txs {
                                        let sequencer_msg = SequencerMessage::L2Message {
                                            sequence_number: msg.sequence_number,
                                            tx_hash,
                                            tx,
                                        };
                                        yield Ok(sequencer_msg);
                                    }
                                }
                                6 => {
                                    let sequencer_msg = SequencerMessage::EndOfBlock {
                                        sequence_number: msg.sequence_number,
                                    };
                                    yield Ok(sequencer_msg);
                                }
                                _ => {
                                    let sequencer_msg = SequencerMessage::Other {
                                        sequence_number: msg.sequence_number,
                                    };
                                    yield Ok(sequencer_msg);
                                }
                            }
                        }
                    }
                    Err(e) => {

                        tracing::error!("Error receiving message: {}", e);
                        yield Err(SequencerMessageError::WebSocketError(e));
                        tracing::error!("WebSocket Connection interrupted, attempting reconnect.");
                        (self.ws_stream, _) = tokio_tungstenite::connect_async(self.url.clone())
                            .await
                            .expect("Failed to connect");
                    }
                }
            }
        })
    }
}

#[derive(Debug)]
/// MessageDecodingError represents an error that can occur while decoding a message from the Arbitrum sequencer.
pub enum MessageDecodingError {
    /// Unsupported message kind, the message kind is not recognized.
    UnsupportedMessageKind(u8),
    /// Error while decoding a transaction, this can happen if the transaction is malformed or not supported.
    TxDecodingError(TransactionDecodingError),
    /// The batch depth is too deep, this can happen if the message contains too many nested batches.
    BatchTooDeep,
    /// The size of the batch is too large, this can happen if the message contains too many transactions.
    /// The maximum size of a batch is 256KB.
    BatchSizeTooLarge,
}

const MAX_BATCH_DEPTH: u32 = 16;
const MAX_L2_MESSAGE_SIZE: u64 = 256 * 1024; // 256KB

#[derive(Debug)]
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
    type Error = MessageDecodingError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(L2MessageKind::UnsignedUserTx),
            1 => Ok(L2MessageKind::ContractTx),
            2 => Ok(L2MessageKind::NonmutatingCall),
            3 => Ok(L2MessageKind::Batch),
            4 => Ok(L2MessageKind::SignedTx),
            5 => Ok(L2MessageKind::Heartbeat),
            6 => Ok(L2MessageKind::SignedCompressedTx),
            _ => Err(MessageDecodingError::UnsupportedMessageKind(value)),
        }
    }
}

fn parse_l2_msg(
    bytes: &mut [u8],
    depth: u32,
) -> Result<Vec<(TxHash, Recovered<Box<dyn Transaction + Send + Sync>>)>, MessageDecodingError> {
    if depth >= MAX_BATCH_DEPTH {
        return Err(MessageDecodingError::BatchTooDeep);
    }

    // get kind byte and cast to enum
    let kind = L2MessageKind::try_from(bytes[0])?;
    let mut transactions: Vec<(TxHash, Recovered<Box<dyn Transaction + Send + Sync>>)> = Vec::new();

    match kind {
        L2MessageKind::SignedTx => {
            let (tx_hash, tx) =
                parse_raw_tx(&bytes[1..]).map_err(MessageDecodingError::TxDecodingError)?;
            transactions.push((tx_hash, tx));
        }
        L2MessageKind::Heartbeat => {
            // deprecated heartbeat message, we can ignore it
        }
        L2MessageKind::NonmutatingCall | L2MessageKind::SignedCompressedTx => {
            return Err(MessageDecodingError::UnsupportedMessageKind(kind as u8));
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
                    return Err(MessageDecodingError::BatchSizeTooLarge);
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
            return Err(MessageDecodingError::UnsupportedMessageKind(kind as u8));
        }
    }

    Ok(transactions)
}

#[derive(Debug)]
/// TransactionDecodingError represents an error that can occur while decoding a transaction from the Arbitrum sequencer.
pub enum TransactionDecodingError {
    RecoverError(SignatureError),
    /// Error while decoding a legacy transaction.
    LegacyDecodingError(alloy_rlp::Error),
    /// Error while decoding an EIP-2930 transaction.
    Eip2930DecodingError(alloy_rlp::Error),
    /// Error while decoding an EIP-1559 transaction.
    Eip1559DecodingError(alloy_rlp::Error),
    /// Error while decoding an EIP-7702 transaction.
    Eip7702DecodingError(alloy_rlp::Error),
    /// Kind byte is neither 0x00 nor 0x01, 0x02, or bigger than 0x7f;
    InvalidTransactionType(u8),
    /// The transaction type byte is missing, indicating an empty transaction. Should not happen.
    MissingTransactionType,
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
    pub fn from_u8(value: u8) -> Result<Self, TransactionDecodingError> {
        match value {
            x if x > 0x7f => Ok(TxType::Legacy),
            1 => Ok(TxType::Eip2930),
            2 => Ok(TxType::Eip1559),
            4 => Ok(TxType::Eip7702),
            _ => Err(TransactionDecodingError::InvalidTransactionType(value)),
        }
    }
}

fn parse_raw_tx(
    bytes: &[u8],
) -> Result<(TxHash, Recovered<Box<dyn Transaction + Send + Sync>>), TransactionDecodingError> {
    let tx_type = bytes
        .first()
        .ok_or(TransactionDecodingError::MissingTransactionType)?;
    let tx_type = TxType::from_u8(*tx_type)?;
    let (tx_hash, tx): (TxHash, Recovered<Box<dyn Transaction + Send + Sync>>) = match tx_type {
        TxType::Legacy => {
            let tx = alloy::consensus::transaction::TxLegacy::rlp_decode_signed(&mut &bytes[0..])
                .map(Box::new)
                .map_err(TransactionDecodingError::Eip1559DecodingError)?;
            let tx_hash = tx.hash();
            let signer = tx
                .recover_signer()
                .map_err(TransactionDecodingError::RecoverError)?;
            (*tx_hash, Recovered::new_unchecked(tx, signer))
        }
        TxType::Eip2930 => {
            let tx = alloy::consensus::transaction::TxEip2930::rlp_decode_signed(&mut &bytes[1..])
                .map(Box::new)
                .map_err(TransactionDecodingError::Eip2930DecodingError)?;
            let tx_hash = tx.hash();
            let signer = tx
                .recover_signer()
                .map_err(TransactionDecodingError::RecoverError)?;
            (*tx_hash, Recovered::new_unchecked(tx, signer))
        }
        TxType::Eip1559 => {
            //log hex string of the transaction
            // decode the EIP-1559 transaction
            let tx = alloy::consensus::transaction::TxEip1559::rlp_decode_signed(&mut &bytes[1..])
                .map(Box::new)
                .map_err(TransactionDecodingError::Eip1559DecodingError)?;
            let tx_hash = tx.hash();
            let signer = tx
                .recover_signer()
                .map_err(TransactionDecodingError::RecoverError)?;
            (*tx_hash, Recovered::new_unchecked(tx, signer))
        }
        TxType::Eip7702 => {
            let tx = alloy::consensus::transaction::TxEip7702::rlp_decode_signed(&mut &bytes[1..])
                .map(Box::new)
                .map_err(TransactionDecodingError::Eip7702DecodingError)?;
            let tx_hash = tx.hash();
            let signer = tx
                .recover_signer()
                .map_err(TransactionDecodingError::RecoverError)?;
            (*tx_hash, Recovered::new_unchecked(tx, signer))
        }
    };
    Ok((tx_hash, tx))
}
