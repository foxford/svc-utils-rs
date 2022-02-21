use std::task::{Context, Poll};

use axum::response::{IntoResponse, Response};
use futures::future::BoxFuture;
use http::{Request};
use hyper::{body::HttpBody, Body};
use tower::{Layer, Service};

#[derive(Clone)]
pub struct Middleware<S> {
    body_size_limit: u64,
    service: S,
}

impl<S> Service<Request<Body>> for Middleware<S>
where
    S: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send,
{
    type Response = Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let limit = self.body_size_limit;
        // best practice is to clone the inner service like this
        // see https://github.com/tower-rs/tower/issues/547 for details
        let clone = self.service.clone();
        let mut inner = std::mem::replace(&mut self.service, clone);

        Box::pin(async move {
            if let Some(len) = req.body().size_hint().exact() {
                if len > limit {
                    let resp_body: Response = Default::default();
                    return Ok((http::StatusCode::PAYLOAD_TOO_LARGE, resp_body).into_response());
                }
            }

            inner.call(req).await
        })
    }
}

pub struct BodyLimitLayer {
    body_size_limit: u64,
}

impl BodyLimitLayer {
    pub fn new(body_size_limit: u64) -> Self {
        Self { body_size_limit }
    }
}

impl<S> Layer<S> for BodyLimitLayer {
    type Service = Middleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        Middleware {
            service,
            body_size_limit: self.body_size_limit,
        }
    }
}
