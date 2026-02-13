//! Chunking logic for batching rows

use bytes::Bytes;

/// Row chunk (batch of raw JSON bytes)
pub struct RowChunk {
    rows: Vec<Bytes>,
}

impl RowChunk {
    /// Create new chunk
    pub fn new() -> Self {
        Self { rows: Vec::new() }
    }

    /// Create with capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            rows: Vec::with_capacity(capacity),
        }
    }

    /// Add row to chunk
    pub fn push(&mut self, row: Bytes) {
        self.rows.push(row);
    }

    /// Check if chunk is empty
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Get chunk size
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Consume chunk and return rows
    pub fn into_rows(self) -> Vec<Bytes> {
        self.rows
    }
}

impl Default for RowChunk {
    fn default() -> Self {
        Self::new()
    }
}

/// Chunking strategy
pub struct ChunkingStrategy {
    chunk_size: usize,
}

impl ChunkingStrategy {
    /// Create new strategy with given chunk size
    pub fn new(chunk_size: usize) -> Self {
        Self { chunk_size }
    }

    /// Check if chunk is full
    pub fn is_full(&self, chunk: &RowChunk) -> bool {
        chunk.len() >= self.chunk_size
    }

    /// Create new chunk with appropriate capacity
    pub fn new_chunk(&self) -> RowChunk {
        RowChunk::with_capacity(self.chunk_size)
    }
}

impl Default for ChunkingStrategy {
    fn default() -> Self {
        Self::new(256)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_operations() {
        let mut chunk = RowChunk::new();
        assert!(chunk.is_empty());

        chunk.push(Bytes::from_static(b"{}"));
        assert_eq!(chunk.len(), 1);
        assert!(!chunk.is_empty());
    }

    #[test]
    fn test_chunking_strategy() {
        let strategy = ChunkingStrategy::new(2);
        let mut chunk = strategy.new_chunk();

        assert!(!strategy.is_full(&chunk));

        chunk.push(Bytes::from_static(b"{}"));
        assert!(!strategy.is_full(&chunk));

        chunk.push(Bytes::from_static(b"{}"));
        assert!(strategy.is_full(&chunk));
    }
}
