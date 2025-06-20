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

///Main module for the sequencer reader crate.
pub mod reader;
///Types for decoding sequencer data.
pub mod types;
