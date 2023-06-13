use std::time::Duration;

use http::{
    header::{HeaderName, AUTHORIZATION, CONTENT_TYPE},
    Method,
};
use tower::Layer;
use tower_http::cors::{Any, Cors, CorsLayer as TowerCorsLayer};

#[derive(Default, Clone)]
pub struct CorsLayer;

impl CorsLayer {
    pub fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for CorsLayer {
    type Service = Cors<S>;

    fn layer(&self, inner: S) -> Self::Service {
        let cors = TowerCorsLayer::new()
            .allow_methods([
                Method::GET,
                Method::PUT,
                Method::POST,
                Method::PATCH,
                Method::DELETE,
            ])
            .allow_headers([
                AUTHORIZATION,
                CONTENT_TYPE,
                HeaderName::from_static("ulms-app-audience"),
                HeaderName::from_static("ulms-scope"),
                HeaderName::from_static("ulms-app-version"),
                HeaderName::from_static("ulms-app-label"),
                HeaderName::from_static("x-agent-label"),
            ])
            .allow_origin(Any)
            .max_age(Duration::from_secs(3600));

        cors.layer(inner)
    }
}
