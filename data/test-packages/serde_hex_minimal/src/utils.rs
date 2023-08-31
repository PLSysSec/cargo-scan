//! various helper functions.
use std::borrow::Borrow;
use std::io;
use types::{Error, ParseHexError};

/// Helper function which takes a mutable slice of expected byte-length and
/// attempts to parse an immutable slice of bytes as hexadecimal characters.
/// Returns an error if `src` is not exactly twice the size of `buf`, or if
/// any non-hexadecimal characters are found.
pub fn fromhex(buf: &mut [u8], src: &[u8]) -> Result<(), ParseHexError> {
    let expect = buf.len() * 2;
    let actual = src.len();
    if expect == actual {
        for (idx, pair) in src.chunks(2).enumerate() {
            buf[idx] = intobyte(pair[0], pair[1])?;
        }
        Ok(())
    } else {
        Err(ParseHexError::Size { expect, actual })
    }
}

/// write hex to buffer.
///
/// # panics
///
/// panics if `buff` is not exactly twice the size of `src`.
pub fn intohex(buf: &mut [u8], src: &[u8]) {
    if buf.len() == src.len() * 2 {
        for (i, byte) in src.iter().enumerate() {
            let (a, b) = frombyte(*byte);
            let idx = i * 2;
            buf[idx] = a;
            buf[idx + 1] = b;
        }
    } else {
        panic!("invalid buffer sizes");
    }
}

/// write uppercase hex to buffer.
///
/// # panics
///
/// panics if `buff` is not exactly twice the size of `src`.
pub fn intohexcaps(buf: &mut [u8], src: &[u8]) {
    if buf.len() == src.len() * 2 {
        for (i, byte) in src.iter().enumerate() {
            let (a, b) = frombytecaps(*byte);
            let idx = i * 2;
            buf[idx] = a;
            buf[idx + 1] = b;
        }
    } else {
        panic!("invalid buffer sizes");
    }
}

/// Helper function which attempts to convert an immutable set of bytes into
/// hexadecimal characters and write them to some destination.
pub fn writehex<S, B, D>(src: S, mut dst: D) -> Result<(), Error>
where
    S: IntoIterator<Item = B>,
    B: Borrow<u8>,
    D: io::Write,
{
    for byte in src.into_iter() {
        let (a, b) = frombyte(*byte.borrow());
        dst.write_all(&[a, b])?;
    }
    Ok(())
}

/// Helper function which attempts to convert an immutable set of bytes into
/// capital hexadecimal characters and write them to some destination.
pub fn writehexcaps<S, B, D>(src: S, mut dst: D) -> Result<(), Error>
where
    S: IntoIterator<Item = B>,
    B: Borrow<u8>,
    D: io::Write,
{
    for byte in src.into_iter() {
        let (a, b) = frombytecaps(*byte.borrow());
        dst.write_all(&[a, b])?;
    }
    Ok(())
}
