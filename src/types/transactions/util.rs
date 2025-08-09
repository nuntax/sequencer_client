use alloy_primitives::{Bytes, FixedBytes, U256};
use bytes::Buf;

pub fn decode<const N: usize, F: From<FixedBytes<N>>>(buf: &mut &[u8]) -> alloy_rlp::Result<F> {
    //read the length of T from the cursor
    if buf.len() < N {
        return Err(alloy_rlp::Error::InputTooShort);
    }
    let data: FixedBytes<N> = FixedBytes::from(
        &buf[..N]
            .try_into()
            .map_err(|_| alloy_rlp::Error::InputTooShort)?,
    );
    buf.advance(N);
    Ok(F::from(data))
}
pub fn decode_rest_with_len(buf: &mut &[u8]) -> alloy_rlp::Result<Bytes> {
    // read one u256 specifying the length of the data, note that this is a special function only used
    let _: U256 = decode(buf)?;
    let data = Bytes::from(buf.to_vec());
    *buf = &[];
    Ok(data)
}
