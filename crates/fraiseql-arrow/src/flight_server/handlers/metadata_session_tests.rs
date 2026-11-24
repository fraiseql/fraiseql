//! Every Flight metadata call requires the session `DoGet` requires: without one it refuses
//! `Unauthenticated`, with one it answers. They answered anyone, so a caller with no session
//! could list the served views and read each one's schema.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Schema};
use arrow_flight::{Criteria, FlightDescriptor, flight_service_server::FlightService};
use futures::StreamExt;
use tonic::{Code, Request};

use super::super::session_test_support as support;
use crate::{flight_server::FraiseQLFlightService, ticket::FlightTicket};

fn service() -> FraiseQLFlightService {
    let service = support::service();
    service.schema_registry.clear();
    let schema = Schema::new(vec![Field::new("id", DataType::Int64, false)]);
    service.schema_registry.register("sales_by_region", Arc::new(schema));
    service
}

/// `message`, with a session when `session` is set.
fn request<T>(message: T, session: bool) -> Request<T> {
    if session {
        support::authorized(message)
    } else {
        Request::new(message)
    }
}

fn descriptor() -> FlightDescriptor {
    let ticket = FlightTicket::OptimizedView {
        view:     "sales_by_region".to_string(),
        filter:   None,
        order_by: None,
        limit:    None,
        offset:   None,
    };
    FlightDescriptor::new_path(vec![String::from_utf8(ticket.encode().unwrap()).unwrap()])
}

/// Each metadata call's outcome: `Ok(())` when it answered, the refusal's code otherwise.
async fn outcomes(session: bool) -> Vec<(&'static str, Result<(), Code>)> {
    let svc = service();
    let code = |status: tonic::Status| status.code();
    let criteria = Criteria {
        expression: vec![].into(),
    };
    let listed = match svc.list_flights(request(criteria, session)).await {
        Ok(response) => {
            let mut stream = response.into_inner();
            let mut first = Ok(());
            while let Some(item) = stream.next().await {
                if let Err(status) = item {
                    first = Err(code(status));
                    break;
                }
            }
            first
        },
        Err(status) => Err(code(status)),
    };
    vec![
        ("ListFlights", listed),
        (
            "GetSchema",
            svc.get_schema(request(descriptor(), session)).await.map(|_| ()).map_err(code),
        ),
        (
            "GetFlightInfo",
            svc.get_flight_info(request(descriptor(), session))
                .await
                .map(|_| ())
                .map_err(code),
        ),
        (
            "PollFlightInfo",
            svc.poll_flight_info(request(descriptor(), session))
                .await
                .map(|_| ())
                .map_err(code),
        ),
    ]
}

#[tokio::test]
async fn without_a_session_every_metadata_call_refuses_unauthenticated() {
    for (call, outcome) in outcomes(false).await {
        assert_eq!(outcome, Err(Code::Unauthenticated), "{call}");
    }
}

#[tokio::test]
async fn with_a_session_every_metadata_call_answers() {
    for (call, outcome) in outcomes(true).await {
        assert_eq!(outcome, Ok(()), "{call}");
    }
}
