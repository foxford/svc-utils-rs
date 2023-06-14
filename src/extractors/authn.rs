use std::sync::Arc;

use axum::{
    async_trait,
    extract::{Extension, FromRequestParts, Json},
    http::{request::Parts, StatusCode},
};
use svc_agent::{AccountId, AgentId};
use svc_authn::{
    jose::ConfigMap as AuthnConfig,
    token::jws_compact::extract::decode_jws_compact_with_config,
};
use svc_error::Error;
use tracing::{field, Span};

/// Extracts `AccountId` from "Authorization: Bearer ..." headers.
pub struct AccountIdExtractor(pub AccountId);

#[async_trait]
impl<S: Send + Sync> FromRequestParts<S> for AccountIdExtractor {
    type Rejection = (StatusCode, Json<Error>);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        use axum::RequestPartsExt;
        let Extension(authn) = parts
            .extract::<Extension<Arc<AuthnConfig>>>()
            .await
            .ok()
            .ok_or((
                StatusCode::UNAUTHORIZED,
                Json(Error::new(
                    "no_authn_config",
                    "No authn config",
                    StatusCode::UNAUTHORIZED,
                )),
            ))?;

        let auth_header = parts
            .headers
            .get("Authorization")
            .and_then(|x| x.to_str().ok())
            .and_then(|x| x.get("Bearer ".len()..));
        let access_token = url::form_urlencoded::parse(parts.uri.query().unwrap_or("").as_bytes())
            .find(|(key, _)| key == "access_token")
            .map(|(_, val)| val);

        let claims = match (auth_header, access_token) {
            (Some(token), _) => decode_jws_compact_with_config::<String>(token, &authn),
            (_, Some(token)) => decode_jws_compact_with_config::<String>(&token, &authn),
            (None, None) => {
                let Extension(id) = parts
                    .extract::<Extension<Arc<AccountId>>>()
                    .await
                    .ok()
                    .ok_or((
                        StatusCode::UNAUTHORIZED,
                        Json(Error::new(
                            "invalid_authentication",
                            "Invalid authentication",
                            StatusCode::UNAUTHORIZED,
                        )),
                    ))?;
                let audience = id.audience();
                return Ok(Self(AccountId::new("anonymous", audience)));
            }
        }
        .map_err(|e| {
            let err = e.to_string();
            (
                StatusCode::UNAUTHORIZED,
                Json(Error::new(
                    "invalid_authentication",
                    &err,
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
impl<S: Send + Sync> FromRequestParts<S> for AgentIdExtractor {
    type Rejection = (StatusCode, Json<Error>);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let agent_label = parts
            .headers
            .get("X-Agent-Label")
            .and_then(|x| x.to_str().ok())
            .unwrap_or("http")
            .to_string();

        let AccountIdExtractor(account_id) =
            AccountIdExtractor::from_request_parts(parts, state).await?;

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
