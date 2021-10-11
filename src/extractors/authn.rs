use axum::{
    async_trait,
    extract::{FromRequest, RequestParts},
};
use http::StatusCode;
use svc_agent::{AccountId, AgentId};
use svc_authn::jose::ConfigMap as AuthnConfig;
use svc_authn::token::jws_compact::extract::decode_jws_compact_with_config;

pub struct Extractor(pub AgentId);

#[async_trait]
impl FromRequest for Extractor {
    type Rejection = (StatusCode, &'static str);

    async fn from_request(req: &mut RequestParts) -> Result<Self, Self::Rejection> {
        let ctx = req
            .extensions()
            .and_then(|x| x.get::<AuthnConfig>())
            .ok_or_else(|| (StatusCode::UNAUTHORIZED, "No authn config"))
            .expect("AuthnConfig must be present");

        let auth_header = req
            .headers()
            .and_then(|x| x.get("Authorization"))
            .and_then(|x| x.to_str().ok())
            .and_then(|x| x.get("Bearer ".len()..))
            .ok_or_else(|| (StatusCode::UNAUTHORIZED, "Invalid authentication"))?;

        let claims = decode_jws_compact_with_config::<String>(auth_header, &ctx.authn())
            .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid authentication"))?
            .claims;
        let account = AccountId::new(claims.subject(), claims.audience());
        let agent_id = AgentId::new("http", account);
        Ok(Extractor(agent_id))
    }
}
