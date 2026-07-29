use crate::{Error, Limits};
use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Unsigned(u64),
    Bytes(Vec<u8>),
    Text(String),
    Array(Vec<Value>),
    Map(Vec<(Value, Value)>),
    Bool(bool),
    Null,
}

pub fn encode_canonical(value: &Value) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    encode_into(value, &mut output)?;
    Ok(output)
}

fn encode_into(value: &Value, output: &mut Vec<u8>) -> Result<(), Error> {
    match value {
        Value::Unsigned(value) => encode_head(0, *value, output),
        Value::Bytes(bytes) => {
            encode_head(2, u64::try_from(bytes.len()).map_err(|_| Error::InvalidLength("CBOR byte string"))?, output);
            output.extend_from_slice(bytes);
        }
        Value::Text(text) => {
            encode_head(3, u64::try_from(text.len()).map_err(|_| Error::InvalidLength("CBOR text string"))?, output);
            output.extend_from_slice(text.as_bytes());
        }
        Value::Array(values) => {
            encode_head(4, u64::try_from(values.len()).map_err(|_| Error::InvalidLength("CBOR array"))?, output);
            for value in values {
                encode_into(value, output)?;
            }
        }
        Value::Map(entries) => {
            let mut encoded = Vec::with_capacity(entries.len());
            for (key, value) in entries {
                encoded.push((encode_canonical(key)?, encode_canonical(value)?));
            }
            encoded.sort_by(|left, right| compare_key_bytes(&left.0, &right.0));
            for pair in encoded.windows(2) {
                if compare_key_bytes(&pair[0].0, &pair[1].0) != Ordering::Less {
                    return Err(Error::NonCanonicalMetadata("duplicate map key"));
                }
            }
            encode_head(5, u64::try_from(encoded.len()).map_err(|_| Error::InvalidLength("CBOR map"))?, output);
            for (key, value) in encoded {
                output.extend_from_slice(&key);
                output.extend_from_slice(&value);
            }
        }
        Value::Bool(false) => output.push(0xf4),
        Value::Bool(true) => output.push(0xf5),
        Value::Null => output.push(0xf6),
    }
    Ok(())
}

fn encode_head(major: u8, argument: u64, output: &mut Vec<u8>) {
    let prefix = major << 5;
    match argument {
        0..=23 => output.push(prefix | u8::try_from(argument).expect("bounded by match")),
        24..=0xff => {
            output.push(prefix | 24);
            output.push(u8::try_from(argument).expect("bounded by match"));
        }
        0x100..=0xffff => {
            output.push(prefix | 25);
            output.extend_from_slice(&u16::try_from(argument).expect("bounded by match").to_be_bytes());
        }
        0x1_0000..=0xffff_ffff => {
            output.push(prefix | 26);
            output.extend_from_slice(&u32::try_from(argument).expect("bounded by match").to_be_bytes());
        }
        _ => {
            output.push(prefix | 27);
            output.extend_from_slice(&argument.to_be_bytes());
        }
    }
}

fn compare_key_bytes(left: &[u8], right: &[u8]) -> Ordering {
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

pub fn decode_canonical(bytes: &[u8], limits: &Limits) -> Result<Value, Error> {
    if u64::try_from(bytes.len()).map_err(|_| Error::LimitExceeded("metadata bytes"))?
        > limits.max_metadata_bytes
    {
        return Err(Error::LimitExceeded("metadata bytes"));
    }
    let mut decoder = Decoder {
        bytes,
        offset: 0,
        items: 0,
        limits,
    };
    let value = decoder.value(0)?;
    if decoder.offset != bytes.len() {
        return Err(Error::NonCanonicalMetadata("trailing CBOR bytes"));
    }
    Ok(value)
}

struct Decoder<'a> {
    bytes: &'a [u8],
    offset: usize,
    items: u64,
    limits: &'a Limits,
}

impl Decoder<'_> {
    fn value(&mut self, depth: usize) -> Result<Value, Error> {
        if depth > self.limits.max_metadata_depth {
            return Err(Error::LimitExceeded("metadata depth"));
        }
        self.items = self
            .items
            .checked_add(1)
            .ok_or(Error::LimitExceeded("metadata item count"))?;
        if self.items > self.limits.max_container_items {
            return Err(Error::LimitExceeded("metadata item count"));
        }

        let initial = self.byte("CBOR initial byte")?;
        let major = initial >> 5;
        let additional = initial & 0x1f;
        match major {
            0 => Ok(Value::Unsigned(self.argument(additional)?)),
            2 => {
                let length = self.argument(additional)?;
                if length > self.limits.max_byte_string_bytes {
                    return Err(Error::LimitExceeded("CBOR byte-string bytes"));
                }
                let bytes = self.bytes(length, "CBOR byte string")?.to_vec();
                Ok(Value::Bytes(bytes))
            }
            3 => {
                let length = self.argument(additional)?;
                if length > self.limits.max_text_bytes {
                    return Err(Error::LimitExceeded("CBOR text bytes"));
                }
                let raw = self.bytes(length, "CBOR text string")?;
                let text = std::str::from_utf8(raw)
                    .map_err(|_| Error::NonCanonicalMetadata("invalid UTF-8 text"))?
                    .to_owned();
                Ok(Value::Text(text))
            }
            4 => {
                let length = self.argument(additional)?;
                self.check_container_length(length)?;
                let capacity = usize::try_from(length)
                    .map_err(|_| Error::LimitExceeded("CBOR array items"))?;
                let mut values = Vec::with_capacity(capacity);
                for _ in 0..length {
                    values.push(self.value(depth + 1)?);
                }
                Ok(Value::Array(values))
            }
            5 => {
                let length = self.argument(additional)?;
                self.check_container_length(length)?;
                let capacity = usize::try_from(length)
                    .map_err(|_| Error::LimitExceeded("CBOR map entries"))?;
                let mut entries = Vec::with_capacity(capacity);
                let mut previous_key: Option<Vec<u8>> = None;
                for _ in 0..length {
                    let key = self.value(depth + 1)?;
                    let key_bytes = encode_canonical(&key)?;
                    if let Some(previous) = &previous_key {
                        if compare_key_bytes(previous, &key_bytes) != Ordering::Less {
                            return Err(Error::NonCanonicalMetadata(
                                "map keys are duplicate or out of order",
                            ));
                        }
                    }
                    previous_key = Some(key_bytes);
                    let value = self.value(depth + 1)?;
                    entries.push((key, value));
                }
                Ok(Value::Map(entries))
            }
            7 => match additional {
                20 => Ok(Value::Bool(false)),
                21 => Ok(Value::Bool(true)),
                22 => Ok(Value::Null),
                _ => Err(Error::NonCanonicalMetadata(
                    "unsupported CBOR simple or floating-point value",
                )),
            },
            _ => Err(Error::NonCanonicalMetadata(
                "unsupported CBOR major type",
            )),
        }
    }

    fn check_container_length(&self, length: u64) -> Result<(), Error> {
        if length > self.limits.max_container_items {
            return Err(Error::LimitExceeded("CBOR container items"));
        }
        Ok(())
    }

    fn argument(&mut self, additional: u8) -> Result<u64, Error> {
        match additional {
            0..=23 => Ok(u64::from(additional)),
            24 => {
                let value = u64::from(self.byte("CBOR argument")?);
                if value < 24 {
                    return Err(Error::NonCanonicalMetadata("non-shortest CBOR argument"));
                }
                Ok(value)
            }
            25 => {
                let raw = self.fixed::<2>("CBOR argument")?;
                let value = u64::from(u16::from_be_bytes(raw));
                if value <= 0xff {
                    return Err(Error::NonCanonicalMetadata("non-shortest CBOR argument"));
                }
                Ok(value)
            }
            26 => {
                let raw = self.fixed::<4>("CBOR argument")?;
                let value = u64::from(u32::from_be_bytes(raw));
                if value <= 0xffff {
                    return Err(Error::NonCanonicalMetadata("non-shortest CBOR argument"));
                }
                Ok(value)
            }
            27 => {
                let raw = self.fixed::<8>("CBOR argument")?;
                let value = u64::from_be_bytes(raw);
                if value <= 0xffff_ffff {
                    return Err(Error::NonCanonicalMetadata("non-shortest CBOR argument"));
                }
                Ok(value)
            }
            31 => Err(Error::NonCanonicalMetadata(
                "indefinite-length CBOR is not permitted",
            )),
            _ => Err(Error::NonCanonicalMetadata("reserved CBOR argument")),
        }
    }

    fn byte(&mut self, context: &'static str) -> Result<u8, Error> {
        let byte = *self.bytes.get(self.offset).ok_or(Error::Truncated(context))?;
        self.offset += 1;
        Ok(byte)
    }

    fn fixed<const N: usize>(&mut self, context: &'static str) -> Result<[u8; N], Error> {
        let end = self
            .offset
            .checked_add(N)
            .ok_or(Error::RangeOutOfBounds(context))?;
        let raw = self
            .bytes
            .get(self.offset..end)
            .ok_or(Error::Truncated(context))?;
        self.offset = end;
        raw.try_into().map_err(|_| Error::Truncated(context))
    }

    fn bytes(&mut self, length: u64, context: &'static str) -> Result<&[u8], Error> {
        let length = usize::try_from(length).map_err(|_| Error::RangeOutOfBounds(context))?;
        let end = self
            .offset
            .checked_add(length)
            .ok_or(Error::RangeOutOfBounds(context))?;
        let raw = self
            .bytes
            .get(self.offset..end)
            .ok_or(Error::Truncated(context))?;
        self.offset = end;
        Ok(raw)
    }
}
