//! Streaming abstractions

mod chunking;
mod filter;
mod json_stream;
mod memory_estimator;
mod typed_stream;

pub use chunking::{ChunkingStrategy, RowChunk};
pub use filter::{FilteredStream, Predicate};
pub use json_stream::{extract_json_bytes, parse_json, JsonStream, StreamStats};
pub use memory_estimator::{ConservativeEstimator, FixedEstimator, MemoryEstimator};
pub use typed_stream::TypedJsonStream;
