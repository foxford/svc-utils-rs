use std::task::{Context, Poll};
use std::collections::HashMap;
use std::convert::{Infallible, TryFrom};
use std::iter::FromIterator;
use std::sync::Arc;

use axum::routing::Router;
use hyper::{Method, StatusCode};
use once_cell::sync::{Lazy, OnceCell};
use prometheus::{
    register_histogram_vec, register_int_counter_vec, Histogram,
    HistogramTimer, HistogramVec, IntCounter, IntCounterVec,
};
use tracing::error;
use futures::future::BoxFuture;
use hyper::Request;
use hyper::Response;
use tower::{Layer, Service};

static METRICS: Lazy<Metrics> = Lazy::new(Metrics::new);

struct Metrics {
    duration_vec: HistogramVec,
    status_vec: IntCounterVec,
}

impl Metrics {
    fn new() -> Self {
        Metrics {
            duration_vec: register_histogram_vec!(
                "request_duration",
                "Request duration",
                &["path", "method"]
            )
            .expect("Can't create stats metrics"),
            status_vec: register_int_counter_vec!(
                "request_stats",
                "Request stats",
                &["path", "method", "status_code"]
            )
            .expect("Can't create stats metrics"),
        }
    }
}

#[derive(Clone)]
struct MethodStatusCounters(Arc<HashMap<(Method, StatusCode), OnceCell<IntCounter>>>);

impl FromIterator<((Method, StatusCode), OnceCell<IntCounter>)> for MethodStatusCounters {
    fn from_iter<T: IntoIterator<Item = ((Method, StatusCode), OnceCell<IntCounter>)>>(
        iter: T,
    ) -> MethodStatusCounters {
        let mut map: HashMap<(Method, StatusCode), OnceCell<IntCounter>> = HashMap::new();
        map.extend(iter);
        MethodStatusCounters(Arc::new(map))
    }
}

impl MethodStatusCounters {
    fn inc_counter(&self, method: Method, status: StatusCode, path: &str) {
        let counter = self.0.get(&(method.clone(), status)).and_then(|c| {
            c.get_or_try_init(|| {
                METRICS
                    .status_vec
                    .get_metric_with_label_values(&[path, method.as_ref(), &status.to_string()])
                    .map_err(|err| {
                        error!(
                            path,
                            %method,
                            ?status,
                            "Creating counter for metrics errored: {:?}", err
                        );
                    })
            })
            .ok()
        });
        if let Some(counter) = counter {
            counter.inc()
        }
    }
}

#[derive(Clone)]
struct MetricsMiddleware<S> {
    durations: HashMap<Method, OnceCell<Histogram>>,
    stats: MethodStatusCounters,
    path: String,
    service: S,
}

impl<S> MetricsMiddleware<S> {
    fn new(service: S, path: &str) -> Self {
        let path = path.trim_start_matches('/').replace('/', "_");
        let methods = [
            Method::PUT,
            Method::POST,
            Method::OPTIONS,
            Method::GET,
            Method::PATCH,
            Method::HEAD,
        ];
        let status_codes = (100..600).filter_map(|x| StatusCode::try_from(x).ok());
        let durations = methods
            .iter()
            .map(|method| (method.to_owned(), OnceCell::new()))
            .collect();
        let stats = status_codes
            .flat_map(|s| {
                methods
                    .iter()
                    .map(move |m| ((m.to_owned(), s), OnceCell::new()))
            })
            .collect();
        Self {
            durations,
            stats,
            path,
            service,
        }
    }

    fn start_timer(&self, method: Method) -> Option<HistogramTimer> {
        self.durations
            .get(&method)
            .and_then(|h| {
                h.get_or_try_init(|| {
                    METRICS
                        .duration_vec
                        .get_metric_with_label_values(&[&self.path, method.as_ref()])
                        .map_err(|err| {
                            error!(
                                path = %self.path,
                                %method,
                                "Creating timer for metrics errored: {:?}", err
                            )
                        })
                })
                .ok()
            })
            .map(|x| x.start_timer())
    }
}


impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for MetricsMiddleware<S>
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
        let clone = self.service.clone();
        let mut inner = std::mem::replace(&mut self.service, clone);
        let method = req.method().to_owned();
        let timer = self.start_timer(method.clone());

        let path = self.path.clone();
        let counters = self.stats.clone();
        Box::pin(async move {
            let res: Response<ResBody> = inner.call(req).await?;
            counters.inc_counter(method, res.status(), &path);
            drop(timer);
            Ok(res)
        })
    }
}

#[derive(Debug, Clone)]
struct MetricsMiddlewareLayer {
    path: String,
}

impl MetricsMiddlewareLayer {
    fn new(path: String) -> Self {
        Self { path }
    }
}

impl<S> Layer<S> for MetricsMiddlewareLayer {
    type Service = MetricsMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        MetricsMiddleware::new(service, &self.path)
    }
}

use axum::body::Body;

pub trait MeteredRoute<H>
where
    H: Service<Request<Body>, Error = Infallible> + Send,
{
    type Output;

    fn metered_route(self, path: &str, svc: H) -> Self::Output;
}

impl<H> MeteredRoute<H> for Router
where
    H: Service<Request<Body>, Response = Response<axum::body::BoxBody>, Error = Infallible>
        + Clone
        + Send
        + 'static,
    H::Future: Send + 'static,
{
    type Output = Router;

    fn metered_route(self, path: &str, svc: H) -> Self::Output {
        let handler = MetricsMiddlewareLayer::new(path.to_owned()).layer(svc);
        self.route(path, handler)
    }
}
