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
#[derive(Debug)]
pub struct SequencerMessage {
    pub sequence_number: u64,
    pub tx: Box<dyn Transaction>,
}
impl SequencerMessage {
    pub fn block_number(&self) -> u64 {
        self.sequence_number + 22207817
    }
}
pub type SequencerReceiver = Receiver<SequencerMessage>;
pub type SequencerSender = Sender<SequencerMessage>;
pub struct SequencerReader {
    pub ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    pub tx: SequencerSender,
}

impl SequencerReader {
    pub async fn new(url: &str) -> (Self, SequencerReceiver) {
        let (ws_stream, _) = tokio_tungstenite::connect_async(url)
            .await
            .expect("Failed to connect");
        let (tx, rx): (SequencerSender, SequencerReceiver) = channel(100);

        (SequencerReader { ws_stream, tx }, rx)
    }
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
pub enum MessageDecodingError {
    UnsupportedMessageKind(u8),
    TxDecodingError(TransactionDecodingError),
    BatchTooDeep,
    BatchSizeTooLarge,
}

const MAX_BATCH_DEPTH: u32 = 16;
const MAX_L2_MESSAGE_SIZE: u64 = 256 * 1024; // 256KB

#[derive(Debug)]
pub enum L2MessageKind {
    UnsignedUserTx = 0,
    ContractTx = 1,
    NonmutatingCall = 2,
    Batch = 3,
    SignedTx = 4,
    Heartbeat = 5,
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
pub enum TransactionDecodingError {
    LegacyDecodingError(alloy_rlp::Error),
    Eip2930DecodingError(alloy_rlp::Error),
    Eip1559DecodingError(alloy_rlp::Error),
    InvalidTransactionType(u8),
    MissingTransactionType,
}

fn parse_raw_tx(bytes: &[u8]) -> Result<Box<dyn Transaction>, TransactionDecodingError> {
    let tx_type = bytes
        .first()
        .ok_or(TransactionDecodingError::MissingTransactionType)?;

    let tx: Box<dyn Transaction> = match tx_type {
        x if x > &0x7f => {
            let mut rawtx = &bytes[0..];
            alloy::consensus::transaction::TxLegacy::decode(&mut rawtx)
                .map(Box::new)
                .map_err(TransactionDecodingError::LegacyDecodingError)?
        }
        1 => {
            let mut rawtx = &bytes[1..];
            alloy::consensus::transaction::TxEip2930::decode(&mut rawtx)
                .map(Box::new)
                .map_err(TransactionDecodingError::Eip2930DecodingError)?
        }
        2 => {
            let mut rawtx = &bytes[1..];
            alloy::consensus::transaction::TxEip1559::rlp_decode_signed(&mut rawtx)
                .map(Box::new)
                .map_err(TransactionDecodingError::Eip1559DecodingError)?
        }
        invalid_type => {
            return Err(TransactionDecodingError::InvalidTransactionType(
                *invalid_type,
            ));
        }
    };
    Ok(tx)
}
