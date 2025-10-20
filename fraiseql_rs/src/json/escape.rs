//! JSON escaping utilities
//!
//! This module provides JSON string escaping operations,
//! critical for high-performance JSON writing without allocations.

/// JSON string escaping with proper character handling
pub fn escape_json_string_scalar(input: &[u8], output: &mut Vec<u8>) {
    for &byte in input {
        match byte {
            b'"' => output.extend_from_slice(b"\\\""),
            b'\\' => output.extend_from_slice(b"\\\\"),
            b'\n' => output.extend_from_slice(b"\\n"),
            b'\r' => output.extend_from_slice(b"\\r"),
            b'\t' => output.extend_from_slice(b"\\t"),
            0..=0x1F => {
                output.extend_from_slice(b"\\u00");
                let hex = byte / 16;
                output.push(b"0123456789abcdef"[hex as usize]);
                let hex = byte % 16;
                output.push(b"0123456789abcdef"[hex as usize]);
            }
            _ => output.push(byte),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_json_string_scalar() {
        let mut output = Vec::new();
        escape_json_string_scalar(b"Hello \"World\"!", &mut output);
        assert_eq!(output, b"Hello \\\"World\\\"!");

        output.clear();
        escape_json_string_scalar(b"Line 1\nLine 2", &mut output);
        assert_eq!(output, b"Line 1\\nLine 2");

        output.clear();
        escape_json_string_scalar(b"Tab\there", &mut output);
        assert_eq!(output, b"Tab\\there");
    }
}
