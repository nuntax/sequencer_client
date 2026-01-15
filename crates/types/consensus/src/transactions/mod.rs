use std::fmt::Display;

use alloy_consensus::{Signed, TransactionEnvelope, TxEip1559, TxEip2930, TxEip7702, TxLegacy};
use alloy_primitives::{Address, TxHash};
pub use deposit::TxDeposit;

use crate::transactions::{internal::ArbitrumInternalTx, submit_retryable::SubmitRetryableTx};
pub mod batchpostingreport;
pub mod deposit;
pub mod internal;
pub mod retryable;
pub mod submit_retryable;
pub mod typed;
pub mod util;
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
    SubmitRetryableTx(SubmitRetryableTx),
    #[envelope(ty = 0x6a)]
    ArbitrumInternal(ArbitrumInternalTx),
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
            ArbTxEnvelope::ArbitrumInternal(tx) => tx.tx_hash(),
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
            ArbTxEnvelope::ArbitrumInternal(tx) => Ok(tx.from()),
        }
    }
}

impl From<ArbitrumInternalTx> for ArbTxEnvelope {
    fn from(tx: ArbitrumInternalTx) -> Self {
        ArbTxEnvelope::ArbitrumInternal(tx)
    }
}
impl From<TxDeposit> for ArbTxEnvelope {
    fn from(tx: TxDeposit) -> Self {
        ArbTxEnvelope::DepositTx(tx)
    }
}
impl From<SubmitRetryableTx> for ArbTxEnvelope {
    fn from(tx: SubmitRetryableTx) -> Self {
        ArbTxEnvelope::SubmitRetryableTx(tx)
    }
}
impl From<Signed<TxLegacy>> for ArbTxEnvelope {
    fn from(tx: Signed<TxLegacy>) -> Self {
        ArbTxEnvelope::Legacy(tx)
    }
}
impl From<Signed<TxEip2930>> for ArbTxEnvelope {
    fn from(tx: Signed<TxEip2930>) -> Self {
        ArbTxEnvelope::Eip2930(tx)
    }
}
impl From<Signed<TxEip1559>> for ArbTxEnvelope {
    fn from(tx: Signed<TxEip1559>) -> Self {
        ArbTxEnvelope::Eip1559(tx)
    }
}
impl From<Signed<TxEip7702>> for ArbTxEnvelope {
    fn from(tx: Signed<TxEip7702>) -> Self {
        ArbTxEnvelope::Eip7702(tx)
    }
}

impl Display for ArbTxType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArbTxType::Legacy => write!(f, "Legacy"),
            ArbTxType::Eip2930 => write!(f, "EIP-2930"),
            ArbTxType::Eip1559 => write!(f, "EIP-1559"),
            ArbTxType::Eip7702 => write!(f, "EIP-7702"),
            ArbTxType::DepositTx => write!(f, "DepositTx"),
            ArbTxType::SubmitRetryableTx => write!(f, "SubmitRetryableTx"),
            ArbTxType::ArbitrumInternal => write!(f, "ArbitrumInternal"),
        }
    }
}
