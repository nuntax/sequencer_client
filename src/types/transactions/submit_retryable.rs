use crate::types::transactions::util::{decode, decode_rest};
use alloy::eips::{
    Decodable2718, Encodable2718, eip2930::AccessList, eip7702::SignedAuthorization,
};
use alloy_consensus::{Transaction, Typed2718};
use alloy_primitives::{Address, B256, Bytes, ChainId, TxHash, TxKind, U256, keccak256};
use alloy_rlp::{BufMut, Decodable, Encodable, Header};
use bytes::Buf;
use serde::{Deserialize, Serialize};
///Main module for the sequencer reader crate.
///
///
///
///
///
/// https://github.com/OffchainLabs/nitro/blob/23cae22e1f76cf3675f965d78e268fd2870d8708/arbos/parse_l2.go#L292
/// Options here are not actually used in the raw representation of the retryable transaction, but are used in the nitro client to provide additional context for the transaction.
/// the method "finalize_after_decode" will set these fields after decoding the transaction.
/// What we do through out this struct is to not use rlp encoding/decoding, since arbitrum nitro also doeesnt use rlp encoding/decoding but rather the solidity abi encoding/decoding.
/// We also explicitly dont use the type id, its only really for explorers and other tools to identify the transaction type.
/// All these modifications are so encoded(decoded(x)) == x, where x is the original retryable transaction bytes received from the sequencer.
#[derive(PartialEq, Debug, Clone, Eq, Serialize, Deserialize)]
pub struct TxSubmitRetryable {
    chain_id: Option<U256>,
    request_id: Option<U256>, // THIS IS THE MESSAGE NUMBER, WHY ARE THEY CALLING IT REQUEST_ID?
    from: Address,
    l1_base_fee: Option<U256>, // base fee of the L1 transaction that created this retryable

    deposit_value: U256,
    gas_fee_cap: U256, //maxFeePerGas
    gas_limit: U256,
    retry_to: Option<Address>,
    retry_value: U256,    //call value
    beneficiary: Address, //callvalue refund address
    max_submission_fee: U256,
    fee_refund_address: Address,
    retry_data_size: U256,
    retry_data: Bytes,
    sequence_number: Option<U256>,
}

impl TxSubmitRetryable {
    pub fn finalize_after_decode(
        &mut self,
        chain_id: Option<U256>,
        request_id: Option<U256>,
        from: Address,
        l1_base_fee: Option<U256>,
    ) {
        self.chain_id = chain_id;
        self.request_id = request_id;
        self.from = from;
        self.l1_base_fee = l1_base_fee;
    }

    pub fn tx_hash(&self) -> TxHash {
        let buffer = &mut Vec::with_capacity(self.rlp_encoded_fields_length());
        self.encode_2718(buffer);
        keccak256(buffer)
    }

    // ...existing code...
    fn rlp_decode_fields(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let chain_id: Option<U256> = Some(decode(buf)?);
        let request_id: Option<U256> = Some(decode(buf)?);
        let from: Address = decode(buf)?;
        let l1_base_fee: Option<U256> = Some(decode(buf)?);
        let deposit_value: U256 = decode(buf)?;
        let gas_fee_cap: U256 = decode(buf)?;
        let gas_limit: U256 = decode(buf)?;
        let retry_to: Option<Address> = Some(decode(buf)?);
        let retry_value: U256 = decode(buf)?;
        let beneficiary: Address = decode(buf)?;
        let max_submission_fee: U256 = decode(buf)?;
        let fee_refund_address: Address = decode(buf)?;
        let retry_data_size: U256 = decode(buf)?;
        let retry_data = decode_rest(buf);

        Ok(Self {
            chain_id,
            request_id,
            from,
            l1_base_fee,
            deposit_value,
            gas_fee_cap,
            gas_limit,
            retry_to,
            retry_value,
            beneficiary,
            max_submission_fee,
            fee_refund_address,
            retry_data_size,
            retry_data,
            sequence_number: None, // sequence number is not part of the retryable transaction encoding
        })
    }
    // ...existing code...

    //this matches nitros way of encoding.
    fn rlp_encode_fields(&self, out: &mut dyn BufMut) {
        let header = self.rlp_header();
        Header::encode(&header, out);
        Encodable::encode(&self.chain_id.unwrap_or_default(), out);
        let request_id = self.request_id.unwrap_or_default().to_be_bytes::<32>();
        Encodable::encode(&request_id, out);
        Encodable::encode(&self.from, out);
        Encodable::encode(&self.l1_base_fee.unwrap_or_default(), out);
        Encodable::encode(&self.deposit_value, out);
        Encodable::encode(&self.gas_fee_cap, out);
        Encodable::encode(&self.gas_limit, out);
        if let Some(retry_to) = &self.retry_to {
            Encodable::encode(retry_to, out);
        }
        Encodable::encode(&self.retry_value, out);
        Encodable::encode(&self.beneficiary, out);
        Encodable::encode(&self.max_submission_fee, out);
        Encodable::encode(&self.fee_refund_address, out);
        Encodable::encode(&self.retry_data, out);
    }

    pub fn rlp_header(&self) -> Header {
        Header {
            list: true,
            payload_length: self.rlp_encoded_fields_length(),
        }
    }

    /// Returns the length of the encoding produced by encode_for_hash
    pub fn rlp_encoded_fields_length(&self) -> usize {
        let mut len = 0;
        len += self.chain_id.unwrap_or_default().length();
        // RequestId (zero padded to 32 bytes after stripping zeros)
        len += 32;
        len += self.from.length();
        len += self.l1_base_fee.unwrap_or_default().length();
        len += self.deposit_value.length();
        len += self.gas_fee_cap.length();
        len += self.gas_limit.length();

        // RetryTo (address, empty if zero address or None)
        if let Some(retry_to) = &self.retry_to {
            len += retry_to.length();
        }
        len += self.retry_value.length();
        len += self.beneficiary.length();
        len += self.max_submission_fee.length();
        len += self.fee_refund_address.length();
        len += self.retry_data.length();
        len += 1;
        len
    }

    pub fn decode_fields_sequencer(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        println!("{:?}", buf.len());
        buf.advance(12);
        let retry_to_decoded: Address = decode(buf)?;
        // if retry_to is zero address, we set it to None
        let retry_to = if retry_to_decoded == Address::default() {
            None
        } else {
            Some(retry_to_decoded)
        };
        Ok(Self {
            retry_to,
            retry_value: decode(buf)?,
            deposit_value: decode(buf)?,
            max_submission_fee: decode(buf).inspect(|_| {
                //we have to advance the buffer here because addresses are 20 bytes but nitro for some reason encodes them as 32 bytes, maybe its some rlp thing?
                buf.advance(12);
            })?,
            fee_refund_address: decode(buf).inspect(|_| {
                buf.advance(12);
            })?,
            beneficiary: decode(buf)?,
            gas_limit: decode(buf)?,
            gas_fee_cap: decode(buf)?,
            retry_data_size: decode(buf)?,
            retry_data: decode_rest(buf),
            chain_id: None, // chain_id is not part of the retryable transaction encoding
            request_id: None, // request_id is not part of the retryable transaction encoding
            from: Address::default(), // from is not part of the retryable transaction encoding
            l1_base_fee: None, // l1_base_fee is not part of the retryable transaction encoding
            sequence_number: None, // sequencer number is not part of the retryable transaction encoding
        })
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

    fn rlp_encoded_length(&self) -> usize {
        self.rlp_encoded_fields_length()
    }
    fn rlp_encode(&self, out: &mut dyn BufMut) {
        self.rlp_encode_fields(out);
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
        self.chain_id.map(|id| id.to())
    }

    #[doc = " Get `nonce`."]
    fn nonce(&self) -> u64 {
        0
    }

    #[doc = " Get `gas_limit`."]
    fn gas_limit(&self) -> u64 {
        self.gas_limit.to()
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
    #[allow(unused_variables)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::hex::FromHex;
    use alloy_primitives::address;
    #[test]
    fn test_decode_submit_retryable() {
        //this test first decodes the tx from the sequencer, then encodes it back and checks if the hash matches the original tx hash.
        //https://arbiscan.io/tx/0x6d3e1568b91fdc70f2e267c315d0b8387fe08552199028ea8b5eac336f6c1f4a
        let encoded = hex::decode(
            "0000000000000000000000001f4ef5dee700ad835a36b160ad9caeb4b80c0e500000000000000000000000000000000000000000000000015af1d78b58c400000000000000000000000000000000000000000000000000015af1da51da16ee60000000000000000000000000000000000000000000000000000001462397986000000000000000000000000008f9c294928efd8b76498c360a179927910f35ce0000000000000000000000001f4ef5dee700ad835a36b160ad9caeb4b80c0e500000000000000000000000000000000000000000000000000000000000006b7a00000000000000000000000000000000000000000000000000000000039387000000000000000000000000000000000000000000000000000000000000000000",
        ).unwrap();
        let mut buf = &encoded[..];
        println!(
            "Buffer: {:?}, length: {}",
            hex::encode(&buf),
            hex::encode(&buf).len()
        );
        let request_id_bytes =
            hex::decode("00000000000000000000000000000000000000000000000000000000001fa928")
                .unwrap();
        let from = address!("0x1A0ac294928EFd8b76498c360A179927910F46dF");
        let l1_base_fee = Some(U256::from(236119188));
        let mut tx: TxSubmitRetryable =
            TxSubmitRetryable::decode_fields_sequencer(&mut buf).unwrap();
        let request_id = U256::from_be_slice(&request_id_bytes);
        tx.finalize_after_decode(Some(U256::from(42161)), Some(request_id), from, l1_base_fee);
        let hash = tx.tx_hash();
        assert_eq!(
            hash,
            TxHash::from_hex("0x6d3e1568b91fdc70f2e267c315d0b8387fe08552199028ea8b5eac336f6c1f4a")
                .unwrap()
        )
    }
}
