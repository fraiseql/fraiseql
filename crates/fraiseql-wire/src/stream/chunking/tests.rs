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
