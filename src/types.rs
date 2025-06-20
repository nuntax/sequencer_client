use serde_derive::Deserialize;
use serde_derive::Serialize;
use serde_json::Value;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Root {
    pub version: u8,
    pub messages: Vec<BroadcastFeedMessage>,
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
    pub l1_incoming_message: L1IncomingMessageHeader,
    pub delayed_messages_read: u64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct L1IncomingMessageHeader {
    pub header: Header,
    #[serde(rename = "l2Msg")]
    pub l2msg: String,
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
}
