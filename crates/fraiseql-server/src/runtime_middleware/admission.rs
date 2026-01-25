use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use axum::{
    body::Body,
    http::{Request, Response, StatusCode},
    response::IntoResponse,
};
use pin_project::pin_project;
use tower::{Layer, Service};

use crate::{
    lifecycle::shutdown::ShutdownCoordinator, resilience::backpressure::AdmissionController,
};

/// Layer for admission control
#[derive(Clone)]
pub struct AdmissionLayer {
    controller: Arc<AdmissionController>,
    shutdown:   Arc<ShutdownCoordinator>,
}

impl AdmissionLayer {
    pub fn new(
        max_concurrent: usize,
        max_queue_depth: usize,
        shutdown: Arc<ShutdownCoordinator>,
    ) -> Self {
        Self {
            controller: Arc::new(AdmissionController::new(max_concurrent, max_queue_depth as u64)),
            shutdown,
        }
    }
}

impl<S> Layer<S> for AdmissionLayer {
    type Service = AdmissionService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AdmissionService {
            inner,
            controller: self.controller.clone(),
            shutdown: self.shutdown.clone(),
        }
    }
}

/// Service wrapper for admission control
#[derive(Clone)]
pub struct AdmissionService<S> {
    inner:      S,
    controller: Arc<AdmissionController>,
    shutdown:   Arc<ShutdownCoordinator>,
}

impl<S, ReqBody> Service<Request<ReqBody>> for AdmissionService<S>
where
    S: Service<Request<ReqBody>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send,
    ReqBody: Send + 'static,
{
    type Error = S::Error;
    type Future = AdmissionFuture<S::Future>;
    type Response = S::Response;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        // Check if shutting down
        if self.shutdown.is_shutting_down() {
            return AdmissionFuture::Rejected(service_unavailable_response());
        }

        // Try to acquire admission permit
        match self.controller.try_acquire() {
            Some(_permit) => {
                // Note: We don't track individual requests in shutdown coordinator from middleware
                // The shutdown coordinator is only used to check if we're shutting down
                // Request counting is handled by the permit itself
                AdmissionFuture::Permitted {
                    future: self.inner.call(req),
                    _permit,
                }
            },
            None => {
                // System overloaded
                AdmissionFuture::Rejected(overloaded_response())
            },
        }
    }
}

#[pin_project(project = AdmissionFutureProj)]
pub enum AdmissionFuture<F> {
    Permitted {
        #[pin]
        future:  F,
        _permit: crate::resilience::backpressure::AdmissionPermit<'static>,
    },
    Rejected(Response<Body>),
}

impl<F, E> Future for AdmissionFuture<F>
where
    F: Future<Output = Result<Response<Body>, E>>,
{
    type Output = Result<Response<Body>, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project() {
            AdmissionFutureProj::Permitted { future, .. } => future.poll(cx),
            AdmissionFutureProj::Rejected(response) => {
                // Create a default response to replace the one we're taking
                let default_response = Response::new(Body::default());
                let actual_response = std::mem::replace(response, default_response);
                Poll::Ready(Ok(actual_response))
            },
        }
    }
}

fn service_unavailable_response() -> Response<Body> {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [("Retry-After", "5")],
        "Service is shutting down",
    )
        .into_response()
}

fn overloaded_response() -> Response<Body> {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [("Retry-After", "1")],
        "Server is overloaded, please retry",
    )
        .into_response()
}
