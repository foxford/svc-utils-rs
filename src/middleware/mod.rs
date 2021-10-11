#[cfg(feature = "cors-middleware")]
pub use cors::{MiddlewareLayer as CorsLayer, Middleware as CorsMiddleware};

#[cfg(feature = "cors-middleware")]
mod cors;
