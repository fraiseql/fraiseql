//! `ListFlights` on the server's Flight service lists exactly the `flight_views` it serves:
//! none of the library's demo views, which the server clears, and every declared view it
//! registered.

use std::sync::Arc;

use fraiseql_arrow::{
    FlightTicket,
    arrow_flight::{Criteria, flight_service_server::FlightService as _},
};
use fraiseql_server::arrow::{create_flight_service, register_flight_views};
use futures::StreamExt as _;

use super::adapter;

async fn listed(service: &fraiseql_arrow::FraiseQLFlightService) -> Vec<String> {
    let mut stream = service
        .list_flights(tonic::Request::new(Criteria {
            expression: vec![].into(),
        }))
        .await
        .unwrap()
        .into_inner();
    let mut names = Vec::new();
    while let Some(info) = stream.next().await {
        for path in info.unwrap().flight_descriptor.unwrap().path {
            match FlightTicket::decode(path.as_bytes()).unwrap() {
                FlightTicket::OptimizedView { view, .. } => names.push(view),
                other => panic!("not a view's ticket: {other:?}"),
            }
        }
    }
    names
}

#[tokio::test]
#[ignore = "needs DATABASE_URL; run by the Dagger integration leg"]
async fn list_flights_lists_exactly_the_declared_views() {
    let pg = adapter().await;
    let service = create_flight_service(Arc::clone(&pg), &[]);
    assert!(listed(&service).await.is_empty(), "nothing is declared, so nothing is listed");

    let declared = ["p_flight_dec_v".to_string(), "p_flight_dec_nan".to_string()];
    let served = register_flight_views(&service, &declared).await;
    assert_eq!(served, declared);
    assert_eq!(listed(&service).await, ["p_flight_dec_nan", "p_flight_dec_v"]);
}
