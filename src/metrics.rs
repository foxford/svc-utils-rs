use std::net::SocketAddr;
use std::time::Duration;

use axum::{extract, routing, routing::Router, AddExtensionLayer, Server};
use hyper::{Body, Request, Response};
use prometheus::{Encoder, Registry, TextEncoder};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tower_http::trace::TraceLayer;

use tracing::{error, field::Empty, info, warn, Span};

/// Http server with graceful shutdown that serves prometheus metrics
///
/// Runs in a separate tokio task
pub struct MetricsServer {
    join_handle: JoinHandle<Result<(), hyper::Error>>,
    closer: oneshot::Sender<()>,
}

impl MetricsServer {
    /// Create new server with prometheus default registry. This will spawn a new tokio task.
    ///
    /// # Arguments
    ///
    /// * `registry` - prometheus registry to gather metrics from
    /// * `bind_addr` - address to bind server to
    pub fn new(bind_addr: SocketAddr) -> Self {
        let app = Router::new().route("/metrics", routing::get(metrics_handler));

        Self::new_(app, bind_addr)
    }

    /// Create new server with a given registry. This will spawn a new tokio task.
    ///
    /// # Arguments
    ///
    /// * `registry` - prometheus registry to gather metrics from
    /// * `bind_addr` - address to bind server to
    pub fn new_with_registry(registry: Registry, bind_addr: SocketAddr) -> Self {
        let app = Router::new();

        let app = app
            .route("/metrics", routing::get(metrics_handler_with_registry))
            .layer(AddExtensionLayer::new(registry));

        Self::new_(app, bind_addr)
    }

    fn new_(app: Router, bind_addr: SocketAddr) -> Self {
        let app = app.layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request<_>| {
                    // TODO: Option will be recorded simpler
                    // when https://github.com/tokio-rs/tracing/pull/1393 lands

                    let span = tracing::info_span!(
                        "http-metrics-request",
                        status_code = Empty,
                        path = request.uri().path(),
                        query = Empty
                    );
                    if let Some(query) = request.uri().query() {
                        span.record("query", &query);
                    }
                    span
                })
                .on_response(|response: &Response<_>, latency: Duration, span: &Span| {
                    span.record("status_code", &tracing::field::display(response.status()));
                    info!("response generated in {:?}", latency)
                }),
        );

        let (closer, rx) = oneshot::channel::<()>();

        let join_handle = tokio::task::spawn(async move {
            Server::bind(&bind_addr)
                .serve(app.into_make_service())
                .with_graceful_shutdown(async {
                    rx.await.ok();
                })
                .await
        });

        Self {
            join_handle,
            closer,
        }
    }

    /// Shutdowns the server
    pub async fn shutdown(self) {
        info!("Received signal, triggering metrics server shutdown");

        let _ = self.closer.send(());
        let fut = tokio::time::timeout(Duration::from_secs(3), self.join_handle);

        match fut.await {
            Err(e) => {
                error!("Metrics server timed out during shutdown, error = {:?}", e);
            }
            Ok(Err(e)) => {
                error!("Metrics server failed during shutdown, error = {:?}", e);
            }
            Ok(Ok(_)) => {
                info!("Metrics server successfully exited");
            }
        }
    }
}

async fn metrics_handler() -> Response<Body> {
    let mut buffer = vec![];
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let response = match encoder.encode(&metric_families, &mut buffer) {
        Ok(_) => Response::builder().status(200).body(buffer.into()).unwrap(),
        Err(err) => {
            warn!("Metrics not gathered: {:?}", err);
            Response::builder().status(500).body(vec![].into()).unwrap()
        }
    };
    response
}

async fn metrics_handler_with_registry(state: extract::Extension<Registry>) -> Response<Body> {
    let registry = state.0;
    let mut buffer = vec![];
    let encoder = TextEncoder::new();
    let metric_families = registry.gather();
    let response = match encoder.encode(&metric_families, &mut buffer) {
        Ok(_) => Response::builder().status(200).body(buffer.into()).unwrap(),
        Err(err) => {
            warn!("Metrics not gathered: {:?}", err);
            Response::builder().status(500).body(vec![].into()).unwrap()
        }
    };
    response
}
