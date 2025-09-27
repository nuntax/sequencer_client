use alloy_primitives::{Address, FixedBytes, U256};
use alloy_rlp::Decodable;
use eyre::Result;

use crate::types::consensus::transactions::util::decode;

#[derive(Debug)]
pub struct BatchPostingReport {
    pub batch_timestamp: U256,
    pub batch_poster_address: Address,
    pub data_hash: FixedBytes<32>,
    pub batch_num: u64,
    pub l1_base_fee: U256,
    pub extra_gas: Option<u64>,
}

impl BatchPostingReport {
    pub fn decode_fields_sequencer(buf: &mut &[u8]) -> Result<Self> {
        let time_stamp = decode(buf)?;
        let batch_poster = decode(buf)?;
        let data_hash = decode(buf)?;
        let batch_num = decode(buf)?;
        let l1_base_fee = decode(buf)?;
        let extra_gas = if buf.is_empty() {
            None
        } else {
            Some(decode(buf)?)
        };
        Ok(Self {
            batch_timestamp: time_stamp,
            batch_poster_address: batch_poster,
            data_hash,
            batch_num,
            l1_base_fee,
            extra_gas,
        })
    }
}
