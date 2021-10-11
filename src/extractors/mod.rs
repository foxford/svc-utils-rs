#[cfg(feature = "authn-extractor")]
pub use authn::Extractor as AuthnExtractor;

#[cfg(feature = "authn-extractor")]
mod authn;
