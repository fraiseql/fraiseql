//! Integration with the Candle ML framework.
//!
//! Enables storing and retrieving tensor embeddings via FraiseQL mutations/queries.

use candle_core::Tensor;

use crate::{FraiseQLClient, Result};

impl FraiseQLClient {
    /// Store a flat embedding tensor via a FraiseQL mutation.
    ///
    /// Serializes the tensor to a JSON float array and injects it as the
    /// `embedding` variable in the mutation.
    ///
    /// # Errors
    ///
    /// Returns an error if tensor serialization fails or the mutation fails.
    pub async fn store_embedding(
        &self,
        mutation: &str,
        tensor: &Tensor,
        mut variables: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let flat = tensor
            .flatten_all()
            .and_then(|t| t.to_vec1::<f32>())
            .map_err(|e| {
                crate::error::FraiseQLError::Network(
                    // Wrap candle error as a network error (closest semantic match)
                    reqwest::Error::from(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("candle tensor error: {e}"),
                    )),
                )
            })?;

        variables["embedding"] = serde_json::json!(flat);
        self.mutate(mutation, Some(&variables)).await
    }

    /// Retrieve stored embeddings and reconstruct them as a Candle tensor.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails or the result cannot be converted.
    pub async fn fetch_embeddings(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        dims: &[usize],
    ) -> Result<Tensor> {
        let data: serde_json::Value = self.query(query, variables).await?;
        let flat: Vec<f32> = serde_json::from_value(data).map_err(|e| {
            crate::error::FraiseQLError::Network(reqwest::Error::from(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("failed to deserialize embeddings: {e}"),
            )))
        })?;
        Tensor::from_vec(flat, dims, &candle_core::Device::Cpu).map_err(|e| {
            crate::error::FraiseQLError::Network(reqwest::Error::from(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("failed to create tensor: {e}"),
            )))
        })
    }
}
