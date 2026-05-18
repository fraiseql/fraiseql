//! Chunking logic for batching rows

use bytes::Bytes;

/// Row chunk (batch of raw JSON bytes)
pub struct RowChunk {
    rows: Vec<Bytes>,
}

impl RowChunk {
    /// Create new chunk
    #[must_use] 
    pub const fn new() -> Self {
        Self { rows: Vec::new() }
    }

    /// Create with capacity
    #[must_use] 
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
    #[must_use] 
    pub const fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Get chunk size
    #[must_use] 
    pub const fn len(&self) -> usize {
        self.rows.len()
    }

    /// Consume chunk and return rows
    #[must_use] 
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
    #[must_use] 
    pub const fn new(chunk_size: usize) -> Self {
        Self { chunk_size }
    }

    /// Check if chunk is full
    #[must_use] 
    pub const fn is_full(&self, chunk: &RowChunk) -> bool {
        chunk.len() >= self.chunk_size
    }

    /// Create new chunk with appropriate capacity
    #[must_use] 
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
mod tests;
