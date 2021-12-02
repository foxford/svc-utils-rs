use std::time::Duration;

use http::{Method, Request, Response};
use hyper::{body::HttpBody, Body};
use tower::Layer;
use tower_http::classify::{ServerErrorsAsFailures, SharedClassifier};
use tower_http::trace::TraceLayer;
use tower_http::trace::{DefaultOnRequest, MakeSpan, OnResponse};
use tracing::{
    error,
    field::{self, Empty},
    info, Span,
};

pub struct LogLayer;

impl LogLayer {
    pub fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for LogLayer {
    type Service = <TraceLayer<
        SharedClassifier<ServerErrorsAsFailures>,
        SpanMaker,
        DefaultOnRequest,
        OnResp,
    > as Layer<S>>::Service;

    fn layer(&self, service: S) -> Self::Service {
        let layer = TraceLayer::new_for_http()
            .make_span_with(SpanMaker)
            .on_response(OnResp);

        layer.layer(service)
    }
}

#[derive(Debug, Clone)]
pub struct SpanMaker;

impl MakeSpan<Body> for SpanMaker {
    fn make_span(&mut self, request: &Request<Body>) -> Span {
        let span = tracing::error_span!(
            "http-api-request",
            status_code = Empty,
            path = request.uri().path(),
            query = request.uri().query(),
            method = %request.method(),
            account_id = Empty,
        );

        if request.method() != Method::GET && request.method() != Method::OPTIONS {
            span.record(
                "body_size",
                &field::debug(request.body().size_hint().upper()),
            );
        }

        span
    }
}

#[derive(Debug, Clone)]
pub struct OnResp;

impl<B> OnResponse<B> for OnResp {
    fn on_response(self, response: &Response<B>, latency: Duration, span: &Span) {
        span.record("status_code", &field::debug(response.status()));
        if response.status().is_client_error() || response.status().is_server_error() {
            error!("response generated in {:?}", latency)
        } else {
            info!("response generated in {:?}", latency)
        }
    }
}
