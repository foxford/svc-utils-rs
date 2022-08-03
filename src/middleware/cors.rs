use std::task::{Context, Poll};

use futures::future::BoxFuture;
use http::{header, HeaderValue};
use hyper::{Request, Response};
use tower::{Layer, Service};

#[derive(Clone)]
pub struct Middleware<S> {
    service: S,
}

#[allow(clippy::declare_interior_mutable_const)]
const ALLOWED_METHODS: HeaderValue = HeaderValue::from_static("GET, PUT, POST, PATCH, DELETE");
#[allow(clippy::declare_interior_mutable_const)]
const ALLOWED_HEADERS: HeaderValue = HeaderValue::from_static(
    "authorization, ulms-app-audience, ulms-scope, ulms-app-version, ulms-app-label, content-type, x-agent-label"
);
#[allow(clippy::declare_interior_mutable_const)]
const ALLOW_CREDENTIALS: HeaderValue = HeaderValue::from_static("true");
#[allow(clippy::declare_interior_mutable_const)]
const MAX_AGE: HeaderValue = HeaderValue::from_static("3600");

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for Middleware<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
    ResBody: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        // best practice is to clone the inner service like this
        // see https://github.com/tower-rs/tower/issues/547 for details
        let origin = req.headers().get("Origin").map(ToOwned::to_owned);
        let method = req.method().clone();

        let clone = self.service.clone();
        let mut inner = std::mem::replace(&mut self.service, clone);

        Box::pin(async move {
            let mut res: Response<ResBody> = inner.call(req).await?;

            match (method, res.status()) {
                (http::Method::OPTIONS, http::StatusCode::METHOD_NOT_ALLOWED) => {
                    *res.status_mut() = http::StatusCode::OK;
                }
                _ => {}
            }

            let h = res.headers_mut();
            h.insert(header::ACCESS_CONTROL_ALLOW_METHODS, ALLOWED_METHODS);
            if let Some(origin) = origin {
                h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
            }
            h.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, ALLOWED_HEADERS);
            h.insert(header::ACCESS_CONTROL_ALLOW_CREDENTIALS, ALLOW_CREDENTIALS);
            h.insert("Access-Control-Max-Age", MAX_AGE);

            Ok(res)
        })
    }
}

#[derive(Debug, Clone)]
pub struct MiddlewareLayer;

impl<S> Layer<S> for MiddlewareLayer {
    type Service = Middleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        Middleware { service }
    }
}
