//! Byte manipulation utilities for protocol parsing

use bytes::{Buf, Bytes};
use std::io;

/// Extension trait for Bytes operations
pub trait BytesExt {
    /// Read a null-terminated string
    fn read_cstr(&mut self) -> io::Result<String>;

    /// Read a 32-bit big-endian integer
    fn read_i32_be(&mut self) -> io::Result<i32>;

    /// Read a 16-bit big-endian integer
    fn read_i16_be(&mut self) -> io::Result<i16>;
}

impl BytesExt for Bytes {
    fn read_cstr(&mut self) -> io::Result<String> {
        let null_pos = self
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no null terminator"))?;

        let s = String::from_utf8(self.slice(..null_pos).to_vec())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        self.advance(null_pos + 1);
        Ok(s)
    }

    fn read_i32_be(&mut self) -> io::Result<i32> {
        if self.remaining() < 4 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "not enough bytes",
            ));
        }
        Ok(self.get_i32())
    }

    fn read_i16_be(&mut self) -> io::Result<i16> {
        if self.remaining() < 2 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "not enough bytes",
            ));
        }
        Ok(self.get_i16())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_cstr() {
        let mut data = Bytes::from_static(b"hello\0world\0");
        assert_eq!(data.read_cstr().unwrap(), "hello");
        assert_eq!(data.read_cstr().unwrap(), "world");
    }

    #[test]
    fn test_read_i32() {
        let mut data = Bytes::from_static(&[0x00, 0x00, 0x01, 0x00]);
        assert_eq!(data.read_i32_be().unwrap(), 256);
    }
}
