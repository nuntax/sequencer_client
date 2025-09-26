use alloy_primitives::{Address, FixedBytes, U256};
use alloy_rlp::Decodable;
use eyre::Result;

#[derive(Debug)]
pub struct BatchPostingReport {
    batch_timestamp: U256,
    batch_poster_address: Address,
    data_hash: FixedBytes<32>,
    batch_num: u64,
    l1_base_fee: U256,
    extra_gas: Option<u64>,
}

impl BatchPostingReport {
    pub fn decode_fields_sequencer(buf: &mut &[u8]) -> Result<Self> {
        let time_stamp = Decodable::decode(buf)?;
        let batch_poster = Decodable::decode(buf)?;
        let data_hash = Decodable::decode(buf)?;
        let batch_num = Decodable::decode(buf)?;
        let l1_base_fee = Decodable::decode(buf)?;
        let extra_gas = if buf.is_empty() {
            None
        } else {
            Some(Decodable::decode(buf)?)
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
