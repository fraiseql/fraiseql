//! `ListFlights` lists the views the schema registry holds: on `fraiseql-server`, exactly the
//! declared `flight_views`, which the server registers after clearing the library's demo
//! views. It looked up four demo names instead, so a server listed none of its views. Each
//! listed descriptor's path is the view's `OptimizedView` ticket, so it can be passed back to
//! `GetSchema` unchanged.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Schema};
use arrow_flight::{Criteria, FlightDescriptor, flight_service_server::FlightService};
use futures::StreamExt;

use super::super::session_test_support as support;
use crate::{flight_server::FraiseQLFlightService, ticket::FlightTicket};

/// A view's schema: one column named after the view, so each view's is distinct.
fn schema_of(view: &str) -> Schema {
    Schema::new(vec![Field::new(view, DataType::Int64, false)])
}

/// A service whose registry holds exactly `views`, as the server leaves it.
fn serving(views: &[&str]) -> FraiseQLFlightService {
    let service = support::service();
    service.schema_registry.clear();
    for view in views {
        service.schema_registry.register(*view, Arc::new(schema_of(view)));
    }
    service
}

/// Every listed descriptor, in listing order.
async fn listed(service: &FraiseQLFlightService) -> Vec<FlightDescriptor> {
    let mut stream = service
        .list_flights(support::authorized(Criteria {
            expression: vec![].into(),
        }))
        .await
        .unwrap()
        .into_inner();
    let mut descriptors = Vec::new();
    while let Some(info) = stream.next().await {
        descriptors.push(info.unwrap().flight_descriptor.unwrap());
    }
    descriptors
}

/// The view a listed descriptor's ticket names.
fn view_of(descriptor: &FlightDescriptor) -> String {
    assert_eq!(descriptor.path.len(), 1, "{descriptor:?}");
    match FlightTicket::decode(descriptor.path[0].as_bytes()).unwrap() {
        FlightTicket::OptimizedView { view, .. } => view,
        other => panic!("not a view's ticket: {other:?}"),
    }
}

#[tokio::test]
async fn the_listed_flights_are_the_declared_views() {
    let service = serving(&["sales_by_region", "va_invoice"]);
    let views: Vec<String> = listed(&service).await.iter().map(view_of).collect();
    assert_eq!(views, ["sales_by_region", "va_invoice"]);
}

#[tokio::test]
async fn with_no_view_declared_nothing_is_listed() {
    assert!(listed(&serving(&[])).await.is_empty());
}

/// A listed descriptor passed back unchanged reads that view's schema.
#[tokio::test]
async fn a_listed_descriptor_passed_back_reads_its_views_schema() {
    let service = serving(&["sales_by_region", "va_invoice"]);
    for descriptor in listed(&service).await {
        let view = view_of(&descriptor);
        let read = service.get_schema(support::authorized(descriptor)).await.unwrap().into_inner();
        assert_eq!(Schema::try_from(&read).unwrap(), schema_of(&view), "{view}");
    }
}
