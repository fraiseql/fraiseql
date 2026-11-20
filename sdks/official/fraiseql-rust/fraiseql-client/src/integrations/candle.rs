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
    /// `path` names where the embeddings are in the response's `data`, one field per
    /// segment: `&["document", "embedding"]` for `{ document(id: 7) { embedding } }`. A
    /// segment that meets a list applies the rest of the path to each item, and their
    /// vectors are concatenated in order, so `&["documents", "embedding"]` over four
    /// documents of three dimensions reads a `[4, 3]` tensor.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails; [`FraiseQLError::Serialization`] naming the
    /// position in `data` that is missing, null or not an array of numbers, or the number
    /// of values read when `dims` does not hold exactly that many.
    pub async fn fetch_embeddings(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        path: &[&str],
        dims: &[usize],
    ) -> Result<Tensor> {
        let data: serde_json::Value = self.query(query, variables).await?;
        let mut flat = Vec::new();
        collect_embeddings(&data, path, "data", &mut flat)?;
        let expected: usize = dims.iter().product();
        if flat.len() != expected {
            return Err(FraiseQLError::Serialization(format!(
                "read {} embedding values at `data.{}`, but dims {dims:?} hold {expected}",
                flat.len(),
                path.join(".")
            )));
        }
        Tensor::from_vec(flat, dims, &candle_core::Device::Cpu)
            .map_err(|e| FraiseQLError::Other(format!("failed to create tensor: {e}")))
    }
}

/// Append the numbers at `path` below `value`, reached at `at`, to `out`; through a list,
/// those of each item in order.
fn collect_embeddings(
    value: &serde_json::Value,
    path: &[&str],
    at: &str,
    out: &mut Vec<f32>,
) -> Result<()> {
    use serde::Deserialize as _;
    use serde_json::Value;

    let refused =
        |what: &str| Err(FraiseQLError::Serialization(format!("embeddings at `{at}`: {what}")));
    match (value, path.split_first()) {
        (Value::Array(items), Some(_)) => {
            for (index, item) in items.iter().enumerate() {
                collect_embeddings(item, path, &format!("{at}.{index}"), out)?;
            }
            Ok(())
        },
        (Value::Object(fields), Some((key, rest))) => {
            let Some(field) = fields.get(*key) else {
                return refused(&format!("no field `{key}`"));
            };
            collect_embeddings(field, rest, &format!("{at}.{key}"), out)
        },
        (Value::Array(items), None) => {
            for (index, item) in items.iter().enumerate() {
                match f32::deserialize(item) {
                    Ok(number) => out.push(number),
                    Err(_) => return refused(&format!("item {index} is {item}, not a number")),
                }
            }
            Ok(())
        },
        (Value::Null, _) => refused("null"),
        (_, Some(_)) => refused("neither an object nor a list"),
        (_, None) => refused("not an array of numbers"),
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

        let fetch =
            server_answering(json!({ "document": { "embedding": sent_embedding(&store).await } }))
                .await;
        let fetched = FraiseQLClientBuilder::new(fetch.uri())
            .build()
            .fetch_embeddings("query", None, &["document", "embedding"], &[2, 3])
            .await
            .unwrap();

        assert_eq!(fetched.dims(), original.dims());
        assert_eq!(fetched.to_vec2::<f32>().unwrap(), original.to_vec2::<f32>().unwrap());
    }

    /// What a FraiseQL server sends for `{ docs(...) { id embedding } }`, pinned against a
    /// live server by `graphql_vector_e2e_pg`.
    const SERVED: &str = include_str!("../../tests/fixtures/vector_docs_response.json");

    #[tokio::test]
    async fn fetch_embeddings_reads_the_vectors_a_server_sends() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(SERVED, "application/json"))
            .mount(&server)
            .await;

        let fetched = FraiseQLClientBuilder::new(server.uri())
            .build()
            .fetch_embeddings("{ docs { id embedding } }", None, &["docs", "embedding"], &[4, 3])
            .await
            .unwrap();

        assert_eq!(
            fetched.to_vec2::<f32>().unwrap(),
            vec![
                vec![1.0, 0.0, 0.0],
                vec![5.0, 0.01, 0.0],
                vec![0.1, 0.02, 0.0],
                vec![0.0, 1.0, 0.0]
            ]
        );
    }

    /// The error `fetch_embeddings` gives for a server answering `data` at `path`, with `dims`.
    async fn refusal(data: serde_json::Value, path: &[&str], dims: &[usize]) -> String {
        let server = server_answering(data).await;
        let err = FraiseQLClientBuilder::new(server.uri())
            .build()
            .fetch_embeddings("query", None, path, dims)
            .await
            .expect_err("refused");
        assert!(matches!(err, crate::FraiseQLError::Serialization(_)), "{err:?}");
        err.to_string()
    }

    #[tokio::test]
    async fn a_missing_field_is_refused_naming_where() {
        let message =
            refusal(json!({ "docs": [{ "embedding": [1.0] }, {}] }), &["docs", "embedding"], &[2])
                .await;
        assert!(message.contains("`data.docs.1`: no field `embedding`"), "{message}");
    }

    #[tokio::test]
    async fn a_null_vector_is_refused_naming_where() {
        let message =
            refusal(json!({ "document": { "embedding": null } }), &["document", "embedding"], &[3])
                .await;
        assert!(message.contains("`data.document.embedding`: null"), "{message}");
    }

    #[tokio::test]
    async fn a_value_that_is_not_a_number_is_refused_naming_where() {
        let message = refusal(
            json!({ "document": { "embedding": [1.0, "2"] } }),
            &["document", "embedding"],
            &[2],
        )
        .await;
        assert!(message.contains("`data.document.embedding`: item 1 is \"2\""), "{message}");
    }

    #[tokio::test]
    async fn a_path_ending_short_of_the_numbers_is_refused() {
        let message =
            refusal(json!({ "document": { "embedding": [1.0] } }), &["document"], &[1]).await;
        assert!(message.contains("`data.document`: not an array of numbers"), "{message}");
    }

    #[tokio::test]
    async fn a_count_dims_do_not_hold_is_refused_before_any_tensor_is_built() {
        let message = refusal(
            json!({ "docs": [{ "embedding": [1.0, 2.0] }, { "embedding": [3.0] }] }),
            &["docs", "embedding"],
            &[2, 2],
        )
        .await;
        assert!(
            message.contains("read 3 embedding values at `data.docs.embedding`"),
            "{message}"
        );
        assert!(message.contains("dims [2, 2] hold 4"), "{message}");
    }

    #[tokio::test]
    async fn a_graphql_error_is_still_reported_as_one() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    json!({ "data": null, "errors": [{ "message": "no such field" }] }),
                ),
            )
            .mount(&server)
            .await;
        let err = FraiseQLClientBuilder::new(server.uri())
            .build()
            .fetch_embeddings("query", None, &["docs", "embedding"], &[1])
            .await
            .expect_err("refused");
        assert!(matches!(err, crate::FraiseQLError::GraphQL { .. }), "{err:?}");
    }
}
