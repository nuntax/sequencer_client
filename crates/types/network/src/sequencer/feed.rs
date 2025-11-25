use serde::*;
use serde_json::Value;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Root {
    pub version: u8,
    pub messages: Option<Vec<BroadcastFeedMessage>>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastFeedMessage {
    pub sequence_number: u64,
    #[serde(rename = "message")]
    pub message_with_meta_data: MessageWithMetadata,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageWithMetadata {
    #[serde(rename = "message")]
    pub l1_incoming_message: L1IncomingMessage,
    pub delayed_messages_read: u64,
}
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchDataStats {
    #[serde(rename = "Length")]
    pub length: u64,
    #[serde(rename = "NonZeros")]
    pub non_zeros: u64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct L1IncomingMessage {
    pub header: Header,
    #[serde(rename = "l2Msg")]
    pub l2msg: String,
    #[serde(rename = "batchGasCost")]
    pub legacy_batch_gas_cost: Option<u64>,
    #[serde(rename = "batchDataTokens")]
    pub batch_data_stats: Option<BatchDataStats>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    pub kind: u8,
    pub sender: String,
    pub block_number: u64,
    pub timestamp: u64,
    pub request_id: Value,
    pub base_fee_l1: Value,
    pub poster: String,
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
