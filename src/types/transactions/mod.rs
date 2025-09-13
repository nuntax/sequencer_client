use alloy_consensus::{Signed, TransactionEnvelope, TxEip1559, TxEip2930, TxEip7702, TxLegacy};
use alloy_primitives::{Address, TxHash};
pub use submit_retryable::TxSubmitRetryable;
mod deposit;
pub use deposit::TxDeposit;
mod submit_retryable;
pub mod unsigned;
mod util;
#[derive(Debug, Clone, TransactionEnvelope)]
#[envelope(tx_type_name = ArbTxType)]
pub enum ArbTxEnvelope {
    #[envelope(ty = 0)]
    Legacy(Signed<TxLegacy>),
    #[envelope(ty = 1)]
    Eip2930(Signed<TxEip2930>),
    #[envelope(ty = 2)]
    Eip1559(Signed<TxEip1559>),
    #[envelope(ty = 4)]
    Eip7702(Signed<TxEip7702>),
    #[envelope(ty = 0x64)]
    DepositTx(TxDeposit),
    #[envelope(ty = 0x69)]
    SubmitRetryableTx(TxSubmitRetryable),
}

impl ArbTxEnvelope {
    /// Returns the transaction type.
    pub fn hash(&self) -> TxHash {
        match self {
            ArbTxEnvelope::Legacy(tx) => *tx.hash(),
            ArbTxEnvelope::Eip2930(tx) => *tx.hash(),
            ArbTxEnvelope::Eip1559(tx) => *tx.hash(),
            ArbTxEnvelope::Eip7702(tx) => *tx.hash(),
            ArbTxEnvelope::SubmitRetryableTx(tx) => tx.tx_hash(),
            ArbTxEnvelope::DepositTx(tx) => tx.tx_hash(),
        }
    }
    /// Recover the sender address.
    pub fn sender(&self) -> Result<Address, alloy_primitives::SignatureError> {
        match self {
            ArbTxEnvelope::Legacy(tx) => tx.recover_signer(),
            ArbTxEnvelope::Eip2930(tx) => tx.recover_signer(),
            ArbTxEnvelope::Eip1559(tx) => tx.recover_signer(),
            ArbTxEnvelope::Eip7702(tx) => tx.recover_signer(),
            ArbTxEnvelope::SubmitRetryableTx(tx) => Ok(tx.from()),
            ArbTxEnvelope::DepositTx(tx) => Ok(tx.from()),
        }
    }
}
