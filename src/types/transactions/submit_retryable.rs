use std::ops::Add;

use alloy::eips::{
    Decodable2718, Encodable2718, eip2930::AccessList, eip7702::SignedAuthorization,
};
use alloy_consensus::{Transaction, Typed2718};
use alloy_primitives::{Address, B256, Bytes, ChainId, TxHash, TxKind, U256, keccak256};
use alloy_rlp::{BufMut, Decodable, Encodable, Header};
use serde::{Deserialize, Serialize};
///Main module for the sequencer reader crate.
///
///
///
///
///
// https://github.com/OffchainLabs/nitro/blob/23cae22e1f76cf3675f965d78e268fd2870d8708/arbos/parse_l2.go#L292
// Options here are not actually used in the raw representation of the retryable transaction, but are used in the nitro client to provide additional context for the transaction.
// the method "finalize_after_decode" will set these fields after decoding the transaction.
#[derive(PartialEq, Debug, Clone, Eq, Serialize, Deserialize)]
pub struct TxSubmitRetryable {
    chain_id: Option<ChainId>,
    request_id: Option<B256>,
    from: Address,
    l1_base_fee: Option<U256>, // base fee of the L1 transaction that created this retryable

    deposit_value: U256,
    gas_fee_cap: U256, //maxFeePerGas
    gas_limit: u64,
    retry_to: Option<Address>,
    retry_value: U256,    //call value
    beneficiary: Address, //callvalue refund address
    max_submission_fee: U256,
    fee_refund_address: Address,
    retry_data: Bytes,
}

impl TxSubmitRetryable {
    pub fn finalize_after_decode(
        &mut self,
        chain_id: Option<ChainId>,
        request_id: Option<B256>,
        from: Address,
        l1_base_fee: Option<U256>,
    ) {
        self.chain_id = chain_id;
        self.request_id = request_id;
        self.from = from;
        self.l1_base_fee = l1_base_fee;
    }

    pub fn tx_hash(&self) -> TxHash {
        let buffer = &mut Vec::with_capacity(self.rlp_encoded_length());
        self.encode_2718(buffer);
        keccak256(buffer)
    }

    fn rlp_encoded_fields_length(&self) -> usize {
        // retry to will be encoded as 20 bytes, even when None
        self.retry_to.map_or(20, |a| a.length())
            + self.retry_value.length()
            + self.deposit_value.length()
            + self.max_submission_fee.length()
            + self.fee_refund_address.length()
            + self.beneficiary.length()
            + self.gas_limit.length()
            + self.gas_fee_cap.length()
            + self.retry_data.length()
    }
    fn rlp_encode_fields(&self, out: &mut dyn BufMut) {
        match self.retry_to.as_ref() {
            Some(a) => a.encode(out),
            None => Address::default().encode(out),
        }
        self.retry_value.encode(out);
        self.deposit_value.encode(out);
        self.max_submission_fee.encode(out);
        self.fee_refund_address.encode(out);
        self.beneficiary.encode(out);
        self.gas_limit.encode(out);
        self.gas_fee_cap.encode(out);
        self.retry_data.encode(out);
    }

    fn rlp_decode_fields(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let retry_to_decoded: Address = Decodable::decode(buf)?;
        let retry_to = if retry_to_decoded == Address::default() {
            None
        } else {
            Some(retry_to_decoded)
        };

        Ok(Self {
            retry_to,
            retry_value: Decodable::decode(buf)?,
            deposit_value: Decodable::decode(buf)?,
            max_submission_fee: Decodable::decode(buf)?,
            fee_refund_address: Decodable::decode(buf)?,
            beneficiary: Decodable::decode(buf)?,
            gas_limit: Decodable::decode(buf)?,
            gas_fee_cap: Decodable::decode(buf)?,
            retry_data: Decodable::decode(buf)?,
            chain_id: None, // chain_id is not part of the retryable transaction encoding
            request_id: None, // request_id is not part of the retryable transaction encoding
            from: Address::default(), // from is not part of the retryable transaction encoding
            l1_base_fee: None, // l1_base_fee is not part of the retryable transaction encoding
        })
    }

    fn rlp_header(&self) -> alloy_rlp::Header {
        alloy_rlp::Header {
            list: true,
            payload_length: self.rlp_encoded_fields_length(),
        }
    }
    fn rlp_header_length(&self) -> usize {
        self.rlp_header().length()
    }
    fn rlp_encoded_length(&self) -> usize {
        self.rlp_header_length() + self.rlp_encoded_fields_length()
    }
    fn rlp_encode(&self, out: &mut dyn BufMut) {
        self.rlp_header().encode(out);
        self.rlp_encode_fields(out);
    }
    pub fn rlp_decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let header = Header::decode(buf)?;
        if !header.list {
            return Err(alloy_rlp::Error::UnexpectedString);
        }
        let remaining = buf.len();

        if header.payload_length > remaining {
            return Err(alloy_rlp::Error::InputTooShort);
        }

        let this = Self::rlp_decode_fields(buf)?;

        if buf.len() + header.payload_length != remaining {
            return Err(alloy_rlp::Error::UnexpectedLength);
        }

        Ok(this)
    }
}

impl Decodable for TxSubmitRetryable {
    fn decode(data: &mut &[u8]) -> alloy_rlp::Result<Self> {
        Self::rlp_decode(data)
    }
}

impl Decodable2718 for TxSubmitRetryable {
    fn typed_decode(ty: u8, buf: &mut &[u8]) -> alloy::eips::eip2718::Eip2718Result<Self> {
        if ty != 105 {
            return Err(alloy::eips::eip2718::Eip2718Error::UnexpectedType(ty));
        }
        let tx = Self::rlp_decode_fields(buf)?;
        Ok(tx)
    }

    fn fallback_decode(buf: &mut &[u8]) -> alloy::eips::eip2718::Eip2718Result<Self> {
        Ok(Self::decode(buf)?)
    }
}

impl Transaction for TxSubmitRetryable {
    #[doc = " Get `chain_id`."]
    fn chain_id(&self) -> Option<ChainId> {
        self.chain_id
    }

    #[doc = " Get `nonce`."]
    fn nonce(&self) -> u64 {
        0
    }

    #[doc = " Get `gas_limit`."]
    fn gas_limit(&self) -> u64 {
        self.gas_limit
    }

    #[doc = " Get `gas_price`."]
    fn gas_price(&self) -> Option<u128> {
        Some(self.gas_fee_cap.to())
    }

    /// This returns the gas fee cap, same as gas_price. Retryable transactions dont have 1559 style fees.
    fn max_fee_per_gas(&self) -> u128 {
        self.gas_fee_cap.to()
    }

    ///returns none for retryable transactions
    fn max_priority_fee_per_gas(&self) -> Option<u128> {
        None
    }

    /// None for retryable transactions
    fn max_fee_per_blob_gas(&self) -> Option<u128> {
        None
    }
    /// Returns the gas fee cap, same as gas_price.
    fn priority_fee_or_price(&self) -> u128 {
        self.gas_fee_cap.to()
    }
    /// Dont use this for retryable transactions, it returns 0.
    fn effective_gas_price(&self, base_fee: Option<u64>) -> u128 {
        0
    }

    #[doc = " Returns `true` if the transaction supports dynamic fees."]
    fn is_dynamic_fee(&self) -> bool {
        false
    }

    #[doc = " Returns the transaction kind."]
    fn kind(&self) -> TxKind {
        if self.retry_to.is_none() {
            TxKind::Create
        } else {
            TxKind::Call(self.retry_to.unwrap())
        }
    }

    #[doc = " Returns true if the transaction is a contract creation."]
    #[doc = " We don\'t provide a default implementation via `kind` as it copies the 21-byte"]
    #[doc = " [`TxKind`] for this simple check. A proper implementation shouldn\'t allocate."]
    fn is_create(&self) -> bool {
        self.retry_to.is_none()
    }

    #[doc = " Get `value`."]
    fn value(&self) -> U256 {
        self.deposit_value
    }

    #[doc = " Get `data`."]
    fn input(&self) -> &Bytes {
        &self.retry_data
    }

    /// Doesn't apply to retryable transactions.
    fn access_list(&self) -> Option<&AccessList> {
        None
    }

    /// Doesn't apply to retryable transactions.
    fn blob_versioned_hashes(&self) -> Option<&[B256]> {
        None
    }

    /// Doesn't apply to retryable transactions.
    fn authorization_list(&self) -> Option<&[SignedAuthorization]> {
        None
    }
}

impl Typed2718 for TxSubmitRetryable {
    #[doc = " Returns the EIP-2718 type flag."]
    fn ty(&self) -> u8 {
        105
    }
}

impl Encodable2718 for TxSubmitRetryable {
    #[doc = " The length of the 2718 encoded envelope. This is the length of the type"]
    #[doc = " flag + the length of the inner encoding."]
    fn encode_2718_len(&self) -> usize {
        self.rlp_encoded_length() + 1
    }

    #[doc = " Encode the transaction according to [EIP-2718] rules. First a 1-byte"]
    #[doc = " type flag in the range 0x0-0x7f, then the body of the transaction."]
    #[doc = ""]
    #[doc = " [EIP-2718] inner encodings are unspecified, and produce an opaque"]
    #[doc = " bytestring."]
    #[doc = ""]
    #[doc = " [EIP-2718]: https://eips.ethereum.org/EIPS/eip-2718"]
    fn encode_2718(&self, out: &mut dyn BufMut) {
        out.put_u8(self.ty());
        self.rlp_encode(out);
    }
}
