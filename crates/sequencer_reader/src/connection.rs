//! This library is used to read messages from the arbitrum sequencer feed and parses them into alloy transactions.
//! It supports the following transaction types:
//! - Legacy transactions
//! - EIP-2930 transactions
//! - EIP-1559 transactions
//! It does not support:
//! - EIP-4844 transactions
//! - EIP-7702 transactions
//! - Compressed transactions (Not implemented in arbitrum reference either)
//! - Non-mutating calls (Not implemented in arbitrum reference either)
//! - Heartbeat messages (Deprecated, not used in arbitrum anymore)
//! Reference implementation:
//! https://github.com/OffchainLabs/nitro/blob/9b1e622102fa2bebfd7dffd327be19f8881f1467/arbos/incomingmessage.go#L328
//! @author: @nuntax (email: dev@nun.tax)
//! @license: MIT License

use crate::types::Root;
use alloy::consensus::Transaction;
use alloy::consensus::transaction::RlpEcdsaDecodableTx;

use alloy_primitives::private::alloy_rlp::Decodable;
use base64::prelude::*;
use futures_util::StreamExt;
use std::io::{Cursor, Read};

use tokio::{
    net::TcpStream,
    sync::mpsc::{Receiver, Sender, channel},
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

/// SequencerMessage represents a message received from the Arbitrum sequencer and is equivalent to one transaction.
/// It contains the sequence number of the message and the transaction itself.
/// Note that the transaction is a boxed trait object. because the transaction can be of different types (legacy, eip2930, eip1559).
/// Also note that the sequence number is not the block number, but can be obtained by calling the `block_number` method.
#[derive(Debug)]
pub struct SequencerMessage {
    /// The sequence number of the message, can be used to determine the block number.
    pub sequence_number: u64,
    /// The transaction contained in the message, can be of different types (legacy, eip2930, eip1559).
    pub tx: Box<dyn Transaction>,
}
impl SequencerMessage {
    /// Returns the block number corresponding to the sequence number.
    ///
    /// This is calculated by adding the sequence number to the Arbitrum genesis block number (22207817).
    ///
    /// <div class="warning">On Nova chains the sequence_number variable is the block number.</div>
    pub fn block_number(&self) -> u64 {
        self.sequence_number + 22207817
    }
}

/// SequencerReader is the main struct of this library.
/// It is used to connect to the Arbitrum sequencer feed and read messages from it.
/// It then forwards the messages to a tokio mpsc, which can be used to receive the messages.
pub struct SequencerReader {
    /// The WebSocket stream used to connect to the Arbitrum sequencer feed.
    pub ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    /// The sender part of the mpsc channel used to forward messages to the receiver.
    pub tx: Sender<SequencerMessage>,
}

impl SequencerReader {
    /// Creates a new SequencerReader and connects to the given URL.
    /// Returns a tuple containing the SequencerReader and the receiver part of the mpsc channel.
    pub async fn new(url: &str) -> (Self, Receiver<SequencerMessage>) {
        let (ws_stream, _) = tokio_tungstenite::connect_async(url)
            .await
            .expect("Failed to connect");
        let (tx, rx): (Sender<SequencerMessage>, Receiver<SequencerMessage>) = channel(100);

        (SequencerReader { ws_stream, tx }, rx)
    }
    /// Starts reading messages from the Arbitrum sequencer feed.
    /// This method runs in a loop and listens for incoming messages.
    /// It then parses the messages and forwards single transactions to the mpsc channel.
    pub async fn start_reading(&mut self) {
        while let Some(message) = self.ws_stream.next().await {
            match message {
                Ok(raw_msg) => {
                    println!("###############################################");
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
                        if msg.message_with_meta_data.l1_incoming_message.header.kind != 3 {
                            return;
                        }

                        let txs = match BASE64_STANDARD
                            .decode(msg.message_with_meta_data.l1_incoming_message.l2msg.clone())
                        {
                            Ok(mut bytes) => match parse_l2_msg(&mut bytes, 0) {
                                Ok(txs) => {
                                    if txs.is_empty() {
                                        eprintln!("Empty transaction batch received");
                                        return;
                                    }
                                    txs
                                }
                                Err(e) => {
                                    eprintln!("Failed to parse L2 message: {:?}", e);
                                    return;
                                }
                            },
                            Err(_) => {
                                eprintln!("Failed to decode base64 L2 message");
                                return;
                            }
                        };
                        for tx in txs {
                            let msg = SequencerMessage {
                                sequence_number: msg.sequence_number,
                                tx,
                            };
                            if let Err(e) = self.tx.try_send(msg) {
                                eprintln!("Failed to send message to receiver: {}", e);
                                return;
                            }
                        }
                    });
                }
                Err(e) => {
                    eprintln!("Error receiving message: {}", e);
                    break;
                }
            }
        }
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
) -> Result<Vec<Box<dyn Transaction>>, MessageDecodingError> {
    if depth >= MAX_BATCH_DEPTH {
        return Err(MessageDecodingError::BatchTooDeep);
    }

    // get kind byte and cast to enum
    let kind = L2MessageKind::try_from(bytes[0])?;
    let mut transactions: Vec<Box<dyn Transaction>> = Vec::new();

    match kind {
        L2MessageKind::SignedTx => {
            let tx = parse_raw_tx(&bytes[1..]).map_err(MessageDecodingError::TxDecodingError)?;
            transactions.push(tx);
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

fn parse_raw_tx(bytes: &[u8]) -> Result<Box<dyn Transaction>, TransactionDecodingError> {
    let tx_type = bytes
        .first()
        .ok_or(TransactionDecodingError::MissingTransactionType)?;
    let tx_type = TxType::from_u8(*tx_type)?;
    let tx: Box<dyn Transaction> = match tx_type {
        TxType::Legacy => alloy::consensus::transaction::TxLegacy::decode(&mut &bytes[1..])
            .map(Box::new)
            .map_err(TransactionDecodingError::LegacyDecodingError)?,
        TxType::Eip2930 => alloy::consensus::transaction::TxEip2930::decode(&mut &bytes[1..])
            .map(Box::new)
            .map_err(TransactionDecodingError::Eip2930DecodingError)?,
        TxType::Eip1559 => {
            alloy::consensus::transaction::TxEip1559::rlp_decode_signed(&mut &bytes[1..])
                .map(Box::new)
                .map_err(TransactionDecodingError::Eip1559DecodingError)?
        }
        TxType::Eip7702 => {
            alloy::consensus::transaction::TxEip7702::rlp_decode_signed(&mut &bytes[1..])
                .map(Box::new)
                .map_err(TransactionDecodingError::Eip7702DecodingError)?
        }
    };
    Ok(tx)
}
