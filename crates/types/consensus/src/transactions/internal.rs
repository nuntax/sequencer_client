use alloy_consensus::{Transaction, Typed2718};
use alloy_eips::{
    Decodable2718, Encodable2718,
    eip2718::{Eip2718Error, Eip2718Result},
    eip2930::AccessList,
    eip7702::SignedAuthorization,
};
use alloy_primitives::{Address, B256, Bytes, ChainId, TxHash, TxKind, U256};
use alloy_rlp::Decodable;
use bytes::BufMut;
use serde::{Deserialize, Serialize};

use crate::transactions::batchpostingreport::BatchPostingReport;

/// Enum for all internal Arbitrum transaction types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArbitrumInternalTx {
    BatchPostingReport(BatchPostingReport),
}

impl ArbitrumInternalTx {
    /// Get the transaction hash
    pub fn tx_hash(&self) -> TxHash {
        match self {
            ArbitrumInternalTx::BatchPostingReport(tx) => tx.tx_hash(),
        }
    }

    /// Get the sender address
    pub fn from(&self) -> Address {
        match self {
            ArbitrumInternalTx::BatchPostingReport(tx) => tx.from(45), //this is the current arbos version. it would be nice if we could use the specs here
        }
    }
}

impl Typed2718 for ArbitrumInternalTx {
    fn ty(&self) -> u8 {
        match self {
            ArbitrumInternalTx::BatchPostingReport(_) => 0x6a,
        }
    }
}

impl Decodable for ArbitrumInternalTx {
    fn decode(data: &mut &[u8]) -> alloy_rlp::Result<Self> {
        // Try to decode as BatchPostingReport
        match BatchPostingReport::decode(data) {
            Ok(tx) => Ok(ArbitrumInternalTx::BatchPostingReport(tx)),
            Err(e) => Err(e),
        }
    }
}

impl Decodable2718 for ArbitrumInternalTx {
    fn typed_decode(ty: u8, buf: &mut &[u8]) -> Eip2718Result<Self> {
        match ty {
            0x6a => {
                // ArbitrumInternal type
                let tx = BatchPostingReport::typed_decode(ty, buf)?;
                Ok(ArbitrumInternalTx::BatchPostingReport(tx))
            }
            _ => Err(Eip2718Error::UnexpectedType(ty)),
        }
    }

    fn fallback_decode(buf: &mut &[u8]) -> Eip2718Result<Self> {
        Ok(Self::decode(buf)?)
    }
}

impl Encodable2718 for ArbitrumInternalTx {
    fn encode_2718_len(&self) -> usize {
        match self {
            ArbitrumInternalTx::BatchPostingReport(tx) => tx.encode_2718_len(),
        }
    }

    fn encode_2718(&self, out: &mut dyn BufMut) {
        match self {
            ArbitrumInternalTx::BatchPostingReport(tx) => tx.encode_2718(out),
        }
    }
}

impl Transaction for ArbitrumInternalTx {
    fn chain_id(&self) -> Option<ChainId> {
        match self {
            ArbitrumInternalTx::BatchPostingReport(tx) => tx.chain_id(),
        }
    }

    fn nonce(&self) -> u64 {
        match self {
            ArbitrumInternalTx::BatchPostingReport(tx) => tx.nonce(),
        }
    }

    fn gas_limit(&self) -> u64 {
        match self {
            ArbitrumInternalTx::BatchPostingReport(tx) => tx.gas_limit(),
        }
    }

    fn gas_price(&self) -> Option<u128> {
        match self {
            ArbitrumInternalTx::BatchPostingReport(tx) => tx.gas_price(),
        }
    }

    fn max_fee_per_gas(&self) -> u128 {
        match self {
            ArbitrumInternalTx::BatchPostingReport(tx) => tx.max_fee_per_gas(),
        }
    }

    fn max_priority_fee_per_gas(&self) -> Option<u128> {
        match self {
            ArbitrumInternalTx::BatchPostingReport(tx) => tx.max_priority_fee_per_gas(),
        }
    }

    fn max_fee_per_blob_gas(&self) -> Option<u128> {
        match self {
            ArbitrumInternalTx::BatchPostingReport(tx) => tx.max_fee_per_blob_gas(),
        }
    }

    fn priority_fee_or_price(&self) -> u128 {
        match self {
            ArbitrumInternalTx::BatchPostingReport(tx) => tx.priority_fee_or_price(),
        }
    }

    fn effective_gas_price(&self, base_fee: Option<u64>) -> u128 {
        match self {
            ArbitrumInternalTx::BatchPostingReport(tx) => tx.effective_gas_price(base_fee),
        }
    }

    fn is_dynamic_fee(&self) -> bool {
        match self {
            ArbitrumInternalTx::BatchPostingReport(tx) => tx.is_dynamic_fee(),
        }
    }

    fn kind(&self) -> TxKind {
        match self {
            ArbitrumInternalTx::BatchPostingReport(tx) => tx.kind(),
        }
    }

    fn is_create(&self) -> bool {
        match self {
            ArbitrumInternalTx::BatchPostingReport(tx) => tx.is_create(),
        }
    }

    fn value(&self) -> U256 {
        match self {
            ArbitrumInternalTx::BatchPostingReport(tx) => tx.value(),
        }
    }

    fn input(&self) -> &Bytes {
        match self {
            ArbitrumInternalTx::BatchPostingReport(tx) => tx.input(),
        }
    }

    fn access_list(&self) -> Option<&AccessList> {
        match self {
            ArbitrumInternalTx::BatchPostingReport(tx) => tx.access_list(),
        }
    }

    fn blob_versioned_hashes(&self) -> Option<&[B256]> {
        match self {
            ArbitrumInternalTx::BatchPostingReport(tx) => tx.blob_versioned_hashes(),
        }
    }

    fn authorization_list(&self) -> Option<&[SignedAuthorization]> {
        match self {
            ArbitrumInternalTx::BatchPostingReport(tx) => tx.authorization_list(),
        }
    }

    fn effective_tip_per_gas(&self, base_fee: u64) -> Option<u128> {
        match self {
            ArbitrumInternalTx::BatchPostingReport(tx) => tx.effective_tip_per_gas(base_fee),
        }
    }

    fn to(&self) -> Option<Address> {
        match self {
            ArbitrumInternalTx::BatchPostingReport(tx) => tx.to(),
        }
    }
}
