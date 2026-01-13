//! JSON stream implementation

use crate::protocol::BackendMessage;
use crate::{Error, Result};
use bytes::Bytes;
use futures::stream::Stream;
use serde_json::Value;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;

/// JSON value stream
pub struct JsonStream {
    receiver: mpsc::Receiver<Result<Value>>,
    _cancel_tx: mpsc::Sender<()>, // Dropped when stream is dropped
}

impl JsonStream {
    /// Create new JSON stream
    pub(crate) fn new(
        receiver: mpsc::Receiver<Result<Value>>,
        cancel_tx: mpsc::Sender<()>,
    ) -> Self {
        Self {
            receiver,
            _cancel_tx: cancel_tx,
        }
    }
}

impl Stream for JsonStream {
    type Item = Result<Value>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}

/// Extract JSON bytes from DataRow message
pub fn extract_json_bytes(msg: &BackendMessage) -> Result<Bytes> {
    match msg {
        BackendMessage::DataRow(fields) => {
            if fields.len() != 1 {
                return Err(Error::Protocol(format!(
                    "expected 1 field, got {}",
                    fields.len()
                )));
            }

            let field = &fields[0];
            field
                .clone()
                .ok_or_else(|| Error::Protocol("null data field".into()))
        }
        _ => Err(Error::Protocol("expected DataRow".into())),
    }
}

/// Parse JSON bytes into Value
pub fn parse_json(data: Bytes) -> Result<Value> {
    let value: Value = serde_json::from_slice(&data)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_bytes() {
        let data = Bytes::from_static(b"{\"key\":\"value\"}");
        let msg = BackendMessage::DataRow(vec![Some(data.clone())]);

        let extracted = extract_json_bytes(&msg).unwrap();
        assert_eq!(extracted, data);
    }

    #[test]
    fn test_extract_null_field() {
        let msg = BackendMessage::DataRow(vec![None]);
        assert!(extract_json_bytes(&msg).is_err());
    }

    #[test]
    fn test_parse_json() {
        let data = Bytes::from_static(b"{\"key\":\"value\"}");
        let value = parse_json(data).unwrap();

        assert_eq!(value["key"], "value");
    }

    #[test]
    fn test_parse_invalid_json() {
        let data = Bytes::from_static(b"not json");
        assert!(parse_json(data).is_err());
    }
}
