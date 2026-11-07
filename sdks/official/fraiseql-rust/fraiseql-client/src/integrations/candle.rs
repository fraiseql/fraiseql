//! Integration with the Candle ML framework.
//!
//! Enables storing and retrieving tensor embeddings via FraiseQL mutations/queries.

use candle_core::Tensor;

use crate::{error::FraiseQLError, FraiseQLClient, Result};

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
            .map_err(|e| FraiseQLError::Other(format!("candle tensor error: {e}")))?;

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
            FraiseQLError::Serialization(format!("failed to deserialize embeddings: {e}"))
        })?;
        Tensor::from_vec(flat, dims, &candle_core::Device::Cpu)
            .map_err(|e| FraiseQLError::Other(format!("failed to create tensor: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use candle_core::{Device, Tensor};
    use serde_json::json;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::FraiseQLClientBuilder;

    /// A 2×3 tensor whose six values all differ, so a transposed or reshaped conversion
    /// changes the order the server receives and the tensor the caller gets back.
    fn tensor_2x3() -> Tensor {
        Tensor::from_vec(vec![1.0_f32, 2.0, 3.0, 4.0, 5.0, 6.0], (2, 3), &Device::Cpu).unwrap()
    }

    async fn server_answering(data: serde_json::Value) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": data })))
            .mount(&server)
            .await;
        server
    }

    /// The `embedding` variable of the one request `server` received.
    async fn sent_embedding(server: &MockServer) -> serde_json::Value {
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        body["variables"]["embedding"].clone()
    }

    #[tokio::test]
    async fn store_embedding_sends_the_tensor_in_row_major_order() {
        let server = server_answering(json!({ "ok": true })).await;
        let client = FraiseQLClientBuilder::new(server.uri()).build();

        let data = client
            .store_embedding("mutation", &tensor_2x3(), json!({ "id": 7 }))
            .await
            .unwrap();

        assert_eq!(data, json!({ "ok": true }));
        assert_eq!(sent_embedding(&server).await, json!([1.0, 2.0, 3.0, 4.0, 5.0, 6.0]));
    }

    #[tokio::test]
    async fn fetch_embeddings_rebuilds_the_tensor_store_embedding_sent() {
        let original = tensor_2x3();
        let store = server_answering(json!({ "ok": true })).await;
        FraiseQLClientBuilder::new(store.uri())
            .build()
            .store_embedding("mutation", &original, json!({}))
            .await
            .unwrap();

        let fetch = server_answering(sent_embedding(&store).await).await;
        let fetched = FraiseQLClientBuilder::new(fetch.uri())
            .build()
            .fetch_embeddings("query", None, &[2, 3])
            .await
            .unwrap();

        assert_eq!(fetched.dims(), original.dims());
        assert_eq!(fetched.to_vec2::<f32>().unwrap(), original.to_vec2::<f32>().unwrap());
    }
}
