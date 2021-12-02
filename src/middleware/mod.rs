#[cfg(feature = "cors-middleware")]
pub use cors::{Middleware as CorsMiddleware, MiddlewareLayer as CorsLayer};

#[cfg(feature = "log-middleware")]
pub use log::LogLayer;

#[cfg(feature = "cors-middleware")]
mod cors;

#[cfg(feature = "log-middleware")]
mod log;
