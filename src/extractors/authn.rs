use std::sync::Arc;

use axum::{
    async_trait,
    body::Body,
    extract::{FromRequest, RequestParts},
};
use http::StatusCode;
use svc_agent::{AccountId, AgentId};
use svc_authn::jose::ConfigMap as AuthnConfig;
use svc_authn::token::jws_compact::extract::decode_jws_compact_with_config;
use tracing::{field, Span};

pub struct Extractor(pub AgentId);

#[async_trait]
impl FromRequest<Body> for Extractor {
    type Rejection = (StatusCode, &'static str);

    async fn from_request(req: &mut RequestParts<Body>) -> Result<Self, Self::Rejection> {
        let authn = req
            .extensions()
            .get::<Arc<AuthnConfig>>()
            .ok_or((StatusCode::UNAUTHORIZED, "No authn config"))?;

        let auth_header = req
            .headers()
            .get("Authorization")
            .and_then(|x| x.to_str().ok())
            .and_then(|x| x.get("Bearer ".len()..))
            .ok_or((StatusCode::UNAUTHORIZED, "Invalid authentication"))?;

        let claims = decode_jws_compact_with_config::<String>(auth_header, authn)
            .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid authentication"))?
            .claims;
        let account = AccountId::new(claims.subject(), claims.audience());

        Span::current().record("account_id", &field::display(&account));

        let agent_id = AgentId::new("http", account);
        Ok(Extractor(agent_id))
    }
}
