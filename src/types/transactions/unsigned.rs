use alloy_primitives::{Address, B256, Bytes, U256};
use alloy_rlp::Decodable;
use eyre::Result;
use serde::{Deserialize, Serialize};

use crate::{reader::L2MessageKind, types::transactions::unsigned};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnsignedTx {
    UserTx(UnsignedUserTx),
    ContractTx(UnsignedContractTx),
}

impl UnsignedTx {
    pub fn value(&self) -> U256 {
        match self {
            UnsignedTx::UserTx(tx) => tx.value,
            UnsignedTx::ContractTx(tx) => tx.value,
        }
    }

    // Utility methods to get inner structs
    pub fn as_user_tx(&self) -> Option<&UnsignedUserTx> {
        match self {
            UnsignedTx::UserTx(tx) => Some(tx),
            _ => None,
        }
    }

    pub fn as_contract_tx(&self) -> Option<&UnsignedContractTx> {
        match self {
            UnsignedTx::ContractTx(tx) => Some(tx),
            _ => None,
        }
    }

    // Common field accessors
    pub fn chain_id(&self) -> U256 {
        match self {
            UnsignedTx::UserTx(tx) => tx.chain_id,
            UnsignedTx::ContractTx(tx) => tx.chain_id,
        }
    }

    pub fn from(&self) -> Address {
        match self {
            UnsignedTx::UserTx(tx) => tx.from,
            UnsignedTx::ContractTx(tx) => tx.from,
        }
    }

    pub fn gas_fee_cap(&self) -> U256 {
        match self {
            UnsignedTx::UserTx(tx) => tx.gas_fee_cap,
            UnsignedTx::ContractTx(tx) => tx.gas_fee_cap,
        }
    }

    pub fn gas_limit(&self) -> U256 {
        match self {
            UnsignedTx::UserTx(tx) => tx.gas_limit,
            UnsignedTx::ContractTx(tx) => tx.gas_limit,
        }
    }

    pub fn to(&self) -> Address {
        match self {
            UnsignedTx::UserTx(tx) => tx.to,
            UnsignedTx::ContractTx(tx) => tx.to,
        }
    }

    pub fn data(&self) -> &Bytes {
        match self {
            UnsignedTx::UserTx(tx) => &tx.data,
            UnsignedTx::ContractTx(tx) => &tx.data,
        }
    }

    // Variant-specific field accessors
    pub fn nonce(&self) -> Option<U256> {
        match self {
            UnsignedTx::UserTx(tx) => Some(tx.nonce),
            UnsignedTx::ContractTx(_) => None,
        }
    }

    pub fn request_id(&self) -> Option<B256> {
        match self {
            UnsignedTx::UserTx(_) => None,
            UnsignedTx::ContractTx(tx) => Some(tx.request_id),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsignedUserTx {
    pub chain_id: U256,
    pub from: Address,
    pub nonce: U256,
    pub gas_fee_cap: U256,
    pub gas_limit: U256,
    pub to: Address,
    pub value: U256,
    pub data: Bytes,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsignedContractTx {
    pub chain_id: U256,
    pub request_id: B256,
    pub from: Address,
    pub gas_fee_cap: U256,
    pub gas_limit: U256,
    pub to: Address,
    pub value: U256,
    pub data: Bytes,
}

pub fn parse_unsigned_tx(
    buf: &mut &[u8],
    sender: Address,
    request_id: B256,
    chain_id: U256,
) -> Result<UnsignedTx> {
    let kind_byte: u8 = Decodable::decode(buf)?;
    let kind = L2MessageKind::try_from(kind_byte)?;
    let gas_limit: U256 = Decodable::decode(buf)?;
    let gas_fee_cap: U256 = Decodable::decode(buf)?;
    let mut nonce: U256 = U256::ZERO;
    if kind == L2MessageKind::UnsignedUserTx {
        nonce = Decodable::decode(buf)?;
    }
    let to: Address = Decodable::decode(buf)?;
    let value: U256 = Decodable::decode(buf)?;
    let data: Bytes = Decodable::decode(buf)?;
    match kind {
        L2MessageKind::UnsignedUserTx => {
            let unsigned_tx = UnsignedUserTx {
                chain_id,
                from: sender,
                nonce,
                gas_fee_cap,
                gas_limit,
                to,
                value,
                data,
            };
            Ok(UnsignedTx::UserTx(unsigned_tx))
        }
        L2MessageKind::ContractTx => {
            let unsigned_tx = UnsignedContractTx {
                chain_id,
                request_id,
                from: sender,
                gas_fee_cap,
                gas_limit,
                to,
                value,
                data,
            };
            Ok(UnsignedTx::ContractTx(unsigned_tx))
        }
        _ => Err(eyre::eyre!(
            "This can't happen, please consider opening a github issue: {:?}",
            kind
        )),
    }
}
