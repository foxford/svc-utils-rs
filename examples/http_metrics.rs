use std::sync::Arc;

use axum::{extract, handler::get, AddExtensionLayer, Router};
use svc_utils::metrics::MetricsServer;
use futures::StreamExt;
use prometheus::{IntCounter, IntGauge, Opts, Registry};
use signal_hook::consts::TERM_SIGNALS;

// Simple http server that collects two metrics (requests counter and inc - dec requests gauge) and three routes:
//      /     - increases counter
//      /inc  - increases counter and gauge
//      /dec  - increases counter and decreases gauge
// The servers listens on http://localhost:8080/, metrics are served from http://localhost:8081/metrics

#[tokio::main]
async fn main() {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "foxford_ulms_svc_utils::metrics=debug")
    }
    tracing_subscriber::fmt::init();

    eprintln!("Hello world");
    eprintln!("Listening on http://localhost:8080/\nAlso try visiting http://localhost:8080/inc and http://localhost:8080/dec");
    eprintln!("Metrics on http://localhost:8081/metrics");

    // Create a Counter.
    let counter = {
        let opts = Opts::new(
            "total_requests_counter",
            "Number of requests processed by the server",
        );
        IntCounter::with_opts(opts).unwrap()
    };
    let gauge = {
        let opts = Opts::new(
            "requests_gauge",
            "Number of requests processed by the server",
        );
        IntGauge::with_opts(opts).unwrap()
    };

    let r = Registry::new();
    r.register(Box::new(counter.clone())).unwrap();
    r.register(Box::new(gauge.clone())).unwrap();

    let shared_state = Arc::new(State(counter, gauge));

    let metrics_server = MetricsServer::new_with_registry(r, "0.0.0.0:8081".parse().unwrap());

    let app = Router::new()
        .route("/", get(root))
        .route("/inc", get(inc))
        .route("/dec", get(dec))
        .layer(AddExtensionLayer::new(shared_state));

    let mut signals_stream = signal_hook_tokio::Signals::new(TERM_SIGNALS)
        .unwrap()
        .fuse();
    let signals = signals_stream.next();

    axum::Server::bind(&"0.0.0.0:8080".parse().unwrap())
        .serve(app.into_make_service())
        .with_graceful_shutdown(async {
            signals.await;
            eprintln!("\nServer shutting down...")
        })
        .await
        .unwrap();

    metrics_server.shutdown().await;
    eprintln!("Goodbye");
}

struct State(IntCounter, IntGauge);

async fn root(state: extract::Extension<Arc<State>>) -> &'static str {
    state.0 .0.inc();
    "Hello world"
}

async fn inc(state: extract::Extension<Arc<State>>) -> &'static str {
    state.0 .0.inc();
    state.0 .1.inc();

    "Increased!"
}

async fn dec(state: extract::Extension<Arc<State>>) -> &'static str {
    state.0 .0.inc();
    state.0 .1.dec();

    "Decreased!"
}
