use std::sync::Arc;

use axum::{
    async_trait,
    body::Body,
    extract::{FromRequest, Json, RequestParts},
};
use http::StatusCode;
use svc_agent::{AccountId, AgentId};
use svc_authn::jose::ConfigMap as AuthnConfig;
use svc_authn::token::jws_compact::extract::decode_jws_compact_with_config;
use svc_error::Error;
use tracing::{field, Span};

/// Extracts `AccountId` from "Authorization: Bearer ..." headers.
pub struct AccountIdExtractor(pub AccountId);

#[async_trait]
impl FromRequest<Body> for AccountIdExtractor {
    type Rejection = (StatusCode, Json<Error>);

    async fn from_request(req: &mut RequestParts<Body>) -> Result<Self, Self::Rejection> {
        let authn = req.extensions().get::<Arc<AuthnConfig>>().ok_or((
            StatusCode::UNAUTHORIZED,
            Json(Error::new(
                "no_authn_config",
                "No authn config",
                StatusCode::UNAUTHORIZED,
            )),
        ))?;

        let auth_header = req
            .headers()
            .get("Authorization")
            .and_then(|x| x.to_str().ok())
            .and_then(|x| x.get("Bearer ".len()..))
            .ok_or((
                StatusCode::UNAUTHORIZED,
                Json(Error::new(
                    "invalid_authentication",
                    "Invalid authentication",
                    StatusCode::UNAUTHORIZED,
                )),
            ))?;

        let claims = decode_jws_compact_with_config::<String>(auth_header, authn)
            .map_err(|_| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(Error::new(
                        "invalid_authentication",
                        "Invalid authentication",
                        StatusCode::UNAUTHORIZED,
                    )),
                )
            })?
            .claims;
        let account_id = AccountId::new(claims.subject(), claims.audience());

        Span::current().record("account_id", &field::display(&account_id));

        Ok(Self(account_id))
    }
}

/// Extracts `AgentId`. User should provide 2 headers to make this work:
///
/// * "Authorization: Bearer <token>"
/// * "X-Agent-Label: <label>"
pub struct AgentIdExtractor(pub AgentId);

#[async_trait]
impl FromRequest<Body> for AgentIdExtractor {
    type Rejection = (StatusCode, Json<Error>);

    async fn from_request(req: &mut RequestParts<Body>) -> Result<Self, Self::Rejection> {
        let AccountIdExtractor(account_id) = AccountIdExtractor::from_request(req).await?;

        let agent_label = req
            .headers()
            .get("X-Agent-Label")
            .and_then(|x| x.to_str().ok())
            .unwrap_or("http");

        // TODO: later missing header will be hard error
        // .ok_or((
        //     StatusCode::UNAUTHORIZED,
        //     Json(Error::new(
        //         "invalid_agent_label",
        //         "Invalid agent label",
        //         StatusCode::UNAUTHORIZED,
        //     )),
        // ))?;

        let agent_id = AgentId::new(agent_label, account_id);

        Span::current().record("agent_id", &field::display(&agent_id));

        Ok(Self(agent_id))
    }
}
