use alloy_consensus::{TxEip1559, TxEip2930, TxEip7702, TxLegacy};

use crate::types::consensus::transactions::{
    ArbTxEnvelope, TxDeposit, submit_retryable::TxSubmitRetryable,
};

pub enum ArbitrumTypedTransaction {
    Legacy(TxLegacy),
    Eip2930(TxEip2930),
    Eip1559(TxEip1559),
    Eip7702(TxEip7702),
    DepositTx(TxDeposit),
    SubmitRetryableTx(TxSubmitRetryable),
}

impl From<ArbTxEnvelope> for ArbitrumTypedTransaction {
    fn from(envelope: ArbTxEnvelope) -> Self {
        match envelope {
            ArbTxEnvelope::Legacy(tx) => Self::Legacy(tx.tx().clone()),
            ArbTxEnvelope::Eip2930(tx) => Self::Eip2930(tx.tx().clone()),
            ArbTxEnvelope::Eip1559(tx) => Self::Eip1559(tx.tx().clone()),
            ArbTxEnvelope::Eip7702(tx) => Self::Eip7702(tx.tx().clone()),
            ArbTxEnvelope::DepositTx(tx) => Self::DepositTx(tx),
            ArbTxEnvelope::SubmitRetryableTx(tx) => Self::SubmitRetryableTx(tx),
        }
    }
}
