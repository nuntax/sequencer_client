use std::sync::OnceLock;

use crate::transactions::util::decode;
use alloy_consensus::Transaction;
use alloy_consensus::Typed2718;
use alloy_core::sol;
use alloy_core::sol_types::SolCall;
use alloy_eips::Decodable2718;
use alloy_eips::Encodable2718;
use alloy_eips::eip2718::Eip2718Error;
use alloy_eips::eip2718::Eip2718Result;
use alloy_eips::eip2930::AccessList;
use alloy_eips::eip7702::SignedAuthorization;
use alloy_primitives::B256;
use alloy_primitives::Bytes;
use alloy_primitives::ChainId;
use alloy_primitives::FixedBytes;
use alloy_primitives::Selector;
use alloy_primitives::TxHash;
use alloy_primitives::TxKind;
use alloy_primitives::{Address, U256, keccak256};
use alloy_rlp::Decodable;
use alloy_rlp::Encodable;
use alloy_rlp::Header;
use arb_sequencer_network::sequencer::feed::BatchDataStats;
use bytes::BufMut;
use serde::{Deserialize, Serialize};

sol! {
   #[sol(rpc)]
   "./src/interfaces/ArbosActs.sol"
}

fn construct_batchpostreport_data(
    batch_timestamp: U256,
    batch_poster: Address,
    batch_num: u64,
    batchgas: u64,
    l1_base_fee: U256,
) -> Bytes {
    ArbosActs::batchPostingReportCall::new((
        batch_timestamp,
        batch_poster,
        batch_num,
        batchgas,
        l1_base_fee,
    ))
    .abi_encode()
    .into()
}
fn construct_batchreportv2_data(
    batch_timestamp: U256,
    batch_poster: Address,
    batch_num: u64,
    batch_data_stats_length: u64,
    batch_data_stats_non_zeros: u64,
    extra_gas: u64,
    l1_base_fee: U256,
) -> Bytes {
    ArbosActs::batchPostingReportV2Call::new((
        batch_timestamp,
        batch_poster,
        batch_num,
        batch_data_stats_length,
        batch_data_stats_non_zeros,
        extra_gas,
        l1_base_fee,
    ))
    .abi_encode()
    .into()
}

fn deconstruct_batchpostreport_data(
    data: &Bytes,
) -> Result<ArbosActs::batchPostingReportCall, alloy_core::sol_types::Error> {
    ArbosActs::batchPostingReportCall::abi_decode(data)
}
fn deconstruct_batchreportv2_data(
    data: &Bytes,
) -> Result<ArbosActs::batchPostingReportV2Call, alloy_core::sol_types::Error> {
    ArbosActs::batchPostingReportV2Call::abi_decode(data)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchPostingReport {
    pub chain_id: ChainId,
    pub data: Bytes,
    #[serde(skip)]
    pub hash: OnceLock<TxHash>,
}
fn get_legacy_costs_from_batch_stats(stats: &BatchDataStats) -> u64 {
    let mut gas = 4 * (stats.length - stats.non_zeros) + 16 * stats.non_zeros;
    let keccak_words = words_for_bytes(stats.length);
    gas += 30 + (keccak_words * 6);
    gas += 2 * 20000;
    gas
}

fn words_for_bytes(nbytes: u64) -> u64 {
    nbytes.div_ceil(32)
}

impl BatchPostingReport {
    pub fn decode_fields_sequencer(
        buf: &mut &[u8],
        chain_id: ChainId,
        arbos_version: u64,
        batch_data_stats: Option<BatchDataStats>,
        legacy_batch_gas: Option<u64>,
    ) -> alloy_rlp::Result<Self> {
        let time_stamp: U256 = decode(buf)?;
        let batch_poster: Address = decode(buf)?;
        let _data_hash: FixedBytes<32> = decode(buf)?;
        let batch_num: U256 = decode(buf)?;
        let l1_base_fee: U256 = decode(buf)?;
        let extra_gas = if buf.is_empty() { 0 } else { decode(buf)? };
        let legacy_gas;
        if let Some(ref batch_data_stats) = batch_data_stats {
            legacy_gas = get_legacy_costs_from_batch_stats(batch_data_stats);
            if legacy_batch_gas.is_some() && legacy_batch_gas.unwrap() != legacy_gas {
                return Err(alloy_rlp::Error::Custom(
                    "Legacy gas doesn't fit local compute.",
                ));
            }
        } else {
            if legacy_batch_gas.is_none() {
                return Err(alloy_rlp::Error::Custom(
                    "Legacy gas doesn't fit local compute2",
                ));
            }
            legacy_gas = legacy_batch_gas.unwrap()
        }
        let data;
        if arbos_version < 50_u64 {
            let batchgas = legacy_gas.saturating_add(extra_gas);
            data = construct_batchpostreport_data(
                time_stamp,
                batch_poster,
                batch_num
                    .try_into()
                    .map_err(|_| alloy_rlp::Error::Overflow)?,
                batchgas,
                l1_base_fee,
            );
        } else if let Some(batch_data_stats) = batch_data_stats {
            data = construct_batchreportv2_data(
                time_stamp,
                batch_poster,
                batch_num
                    .try_into()
                    .map_err(|_| alloy_rlp::Error::Overflow)?,
                batch_data_stats.length,
                batch_data_stats.non_zeros,
                extra_gas,
                l1_base_fee,
            );
        } else {
            return Err(alloy_rlp::Error::Custom(
                "Batch data stats required for arbos version >= 50",
            ));
        }

        Ok(Self {
            chain_id,
            data,
            hash: OnceLock::new(),
        })
    }

    pub fn tx_hash(&self) -> TxHash {
        *self.hash.get_or_init(|| {
            let buffer = &mut Vec::with_capacity(self.rlp_encoded_fields_length());
            self.encode_2718(buffer);
            keccak256(buffer)
        })
    }

    pub fn from(&self, arbos_version: u64) -> Address {
        if arbos_version < 50_u64 {
            deconstruct_batchpostreport_data(&self.data)
                .unwrap()
                .batchPosterAddress
        } else {
            deconstruct_batchreportv2_data(&self.data)
                .unwrap()
                .batchPosterAddress
        }
    }

    pub fn rlp_encode_fields(&self, out: &mut dyn BufMut) {
        self.chain_id.encode(out);
        self.data.encode(out);
    }

    pub fn rlp_encoded_fields_length(&self) -> usize {
        self.chain_id.length() + self.data.length()
    }

    pub fn rlp_header(&self) -> Header {
        Header {
            list: true,
            payload_length: self.rlp_encoded_fields_length(),
        }
    }

    pub fn rlp_encode(&self, out: &mut dyn BufMut) {
        self.rlp_header().encode(out);
        self.rlp_encode_fields(out);
    }

    fn rlp_encoded_length(&self) -> usize {
        self.rlp_header().length_with_payload()
    }

    pub fn rlp_decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let header = Header::decode(buf)?;
        if !header.list {
            return Err(alloy_rlp::Error::Custom("Expected list header"));
        }
        let expected_len = header.payload_length;
        if buf.len() < expected_len {
            return Err(alloy_rlp::Error::Custom("Buffer too short for payload"));
        }
        let chain_id: ChainId = Decodable::decode(buf)?;
        let data: Bytes = Decodable::decode(buf)?;
        *buf = &buf[expected_len..];
        Ok(Self {
            chain_id,
            data,
            hash: OnceLock::new(),
        })
    }
}

impl Typed2718 for BatchPostingReport {
    fn ty(&self) -> u8 {
        0x6a // ArbitrumInternal type
    }
}

impl Decodable for BatchPostingReport {
    fn decode(data: &mut &[u8]) -> alloy_rlp::Result<Self> {
        Self::rlp_decode(data)
    }
}

impl Decodable2718 for BatchPostingReport {
    fn typed_decode(ty: u8, buf: &mut &[u8]) -> Eip2718Result<Self> {
        if ty != 0x6a {
            // ArbitrumInternal type
            return Err(Eip2718Error::UnexpectedType(ty));
        }
        let tx = Self::rlp_decode(buf)?;
        Ok(tx)
    }

    fn fallback_decode(buf: &mut &[u8]) -> Eip2718Result<Self> {
        Ok(Self::decode(buf)?)
    }
}

impl Encodable2718 for BatchPostingReport {
    fn encode_2718_len(&self) -> usize {
        self.rlp_encoded_length() + 1
    }

    fn encode_2718(&self, out: &mut dyn BufMut) {
        out.put_u8(self.ty());
        self.rlp_encode(out);
    }
}

impl Transaction for BatchPostingReport {
    fn chain_id(&self) -> Option<ChainId> {
        None // BatchPostingReport doesn't have a chain_id field
    }

    fn nonce(&self) -> u64 {
        0
    }

    fn gas_limit(&self) -> u64 {
        u64::MAX
    }

    fn gas_price(&self) -> Option<u128> {
        None
    }

    fn max_fee_per_gas(&self) -> u128 {
        0
    }

    fn max_priority_fee_per_gas(&self) -> Option<u128> {
        None
    }

    fn max_fee_per_blob_gas(&self) -> Option<u128> {
        None
    }

    fn priority_fee_or_price(&self) -> u128 {
        0
    }

    fn effective_gas_price(&self, _base_fee: Option<u64>) -> u128 {
        0
    }

    fn is_dynamic_fee(&self) -> bool {
        false
    }

    fn kind(&self) -> TxKind {
        if deconstruct_batchpostreport_data(&self.data).is_err() {
            TxKind::Call(
                deconstruct_batchreportv2_data(&self.data)
                    .unwrap()
                    .batchPosterAddress,
            )
        } else {
            TxKind::Call(
                deconstruct_batchpostreport_data(&self.data)
                    .unwrap()
                    .batchPosterAddress,
            )
        }
    }

    fn is_create(&self) -> bool {
        false
    }

    fn value(&self) -> U256 {
        U256::ZERO
    }

    fn input(&self) -> &Bytes {
        static EMPTY_BYTES: Bytes = Bytes::new();
        &EMPTY_BYTES
    }

    fn access_list(&self) -> Option<&AccessList> {
        None
    }

    fn blob_versioned_hashes(&self) -> Option<&[B256]> {
        None
    }

    fn authorization_list(&self) -> Option<&[SignedAuthorization]> {
        None
    }

    fn effective_tip_per_gas(&self, _base_fee: u64) -> Option<u128> {
        None
    }

    fn to(&self) -> Option<Address> {
        None
    }

    fn function_selector(&self) -> Option<&Selector> {
        None
    }

    fn blob_count(&self) -> Option<u64> {
        None
    }

    fn blob_gas_used(&self) -> Option<u64> {
        None
    }

    fn authorization_count(&self) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::{ChainId, hex};
    #[test]
    fn decode_from_sequencer() {
        //https://arbiscan.io/tx/0xb1cfd6bec3cf825c3f4408fa8ba4c3bef3f87bd13cd61bc2a2987be18eb2baf2
        let hex_str = "0000000000000000000000000000000000000000000000000000000068ebcb9fc1b634853cb333d3ad8663715b08f41a3aec47ccdce8f8854ba69539c300e589e8aba297b3428ab1cbe83288d0375a46319ffa6e00000000000000000000000000000000000000000000000000000000000fc7c800000000000000000000000000000000000000000000000000000000caebfb170000000000000000";
        let bytes = hex::decode(hex_str).unwrap();
        let mut slice = bytes.as_slice();
        let chain_id: ChainId = 42161u64.into();
        let version = 40;
        let batch_data_stats = None;
        let legacy_batch_gas = Some(42000);
        let report = super::BatchPostingReport::decode_fields_sequencer(
            &mut slice,
            chain_id,
            version,
            batch_data_stats,
            legacy_batch_gas,
        )
        .unwrap();
        let encoded = {
            let mut v = Vec::with_capacity(report.rlp_encoded_length());
            report.rlp_encode(&mut v);
            v
        };
        let to_hex_str = hex::encode(&encoded);
        println!("Encoded: {}", to_hex_str);
        println!("{:?}, hash: {:?}", report, report.tx_hash());
    }
}
