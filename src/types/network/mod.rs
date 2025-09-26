use alloy::network::Network;

use crate::types::consensus::transactions::{
    ArbTxEnvelope, ArbTxType, typed::ArbitrumTypedTransaction,
};

/// Types for an Arbitrum-like network.
#[derive(Clone, Copy, Debug)]
pub struct Arbitrum {
    _private: (),
}

// impl Network for Arbitrum {
//     type TxType = ArbTxType;
//     type TxEnvelope = ArbTxEnvelope;
//     type UnsignedTx = ArbitrumTypedTransaction;
//     type ReceiptResponse = ArbTransactionReceipt;
// }
