use crate::Error;

pub(crate) const FILE_MAGIC: [u8; 8] = *b"UCOF\r\n\x1a\n";
pub(crate) const RECORD_MAGIC: [u8; 4] = *b"UCRD";
pub(crate) const FOOTER_MAGIC: [u8; 8] = *b"UCFTR001";

pub(crate) const HEADER_LEN: usize = 32;
pub(crate) const RECORD_HEADER_LEN: usize = 40;
pub(crate) const FOOTER_LEN: usize = 80;

pub(crate) fn push_u16_le(target: &mut Vec<u8>, value: u16) {
    target.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn push_u32_le(target: &mut Vec<u8>, value: u32) {
    target.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn push_u64_le(target: &mut Vec<u8>, value: u64) {
    target.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn read_u16_le(bytes: &[u8], offset: usize, context: &'static str) -> Result<u16, Error> {
    let raw = take(bytes, offset, 2, context)?;
    Ok(u16::from_le_bytes([raw[0], raw[1]]))
}

pub(crate) fn read_u32_le(bytes: &[u8], offset: usize, context: &'static str) -> Result<u32, Error> {
    let raw = take(bytes, offset, 4, context)?;
    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

pub(crate) fn read_u64_le(bytes: &[u8], offset: usize, context: &'static str) -> Result<u64, Error> {
    let raw = take(bytes, offset, 8, context)?;
    Ok(u64::from_le_bytes([
        raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
    ]))
}

pub(crate) fn take<'a>(
    bytes: &'a [u8],
    offset: usize,
    length: usize,
    context: &'static str,
) -> Result<&'a [u8], Error> {
    let end = offset
        .checked_add(length)
        .ok_or(Error::RangeOutOfBounds(context))?;
    bytes.get(offset..end).ok_or(Error::Truncated(context))
}

pub(crate) fn checked_range(
    offset: u64,
    length: u64,
    upper_bound: usize,
    context: &'static str,
) -> Result<std::ops::Range<usize>, Error> {
    let end = offset
        .checked_add(length)
        .ok_or(Error::RangeOutOfBounds(context))?;
    let start = usize::try_from(offset).map_err(|_| Error::RangeOutOfBounds(context))?;
    let end = usize::try_from(end).map_err(|_| Error::RangeOutOfBounds(context))?;
    if end > upper_bound {
        return Err(Error::RangeOutOfBounds(context));
    }
    Ok(start..end)
}
