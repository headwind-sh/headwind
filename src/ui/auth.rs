use axum::{
    Json,
    extract::FromRequestParts,
    http::{StatusCode, header::AUTHORIZATION, request::Parts},
    response::{IntoResponse, Response},
};
use k8s_openapi::api::authentication::v1::{TokenReview, TokenReviewSpec};
use kube::{Api, Client, api::PostParams};
use serde::Serialize;
use std::env;
use tracing::{debug, error, info};

/// Authentication mode for the Web UI
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    /// No authentication - logs "web-ui-user"
    None,
    /// Simple username input - trusts user-provided username
    Simple,
    /// Kubernetes TokenReview - validates K8s token and extracts username
    Token,
    /// Proxy/Ingress auth - reads username from HTTP headers
    Proxy,
}

impl AuthMode {
    /// Get auth mode from environment variable
    pub fn from_env() -> Self {
        match env::var("HEADWIND_UI_AUTH_MODE")
            .unwrap_or_else(|_| "none".to_string())
            .to_lowercase()
            .as_str()
        {
            "simple" => AuthMode::Simple,
            "token" => AuthMode::Token,
            "proxy" => AuthMode::Proxy,
            _ => AuthMode::None,
        }
    }

    /// Get the header name for proxy mode
    pub fn proxy_header() -> String {
        env::var("HEADWIND_UI_PROXY_HEADER").unwrap_or_else(|_| "X-Forwarded-User".to_string())
    }
}

/// User identity extracted from the request
#[derive(Clone, Debug)]
pub struct UserIdentity {
    pub username: String,
    pub auth_mode: AuthMode,
}

/// Authentication error response
#[derive(Serialize)]
pub struct AuthError {
    pub error: String,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        (StatusCode::UNAUTHORIZED, Json(self)).into_response()
    }
}

/// Extract user identity from the request based on configured auth mode
impl<S> FromRequestParts<S> for UserIdentity
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let auth_mode = AuthMode::from_env();

        match auth_mode {
            AuthMode::None => {
                // No authentication - use default username
                Ok(UserIdentity {
                    username: "web-ui-user".to_string(),
                    auth_mode,
                })
            },

            AuthMode::Simple => {
                // Simple mode - read username from X-User header (set by frontend)
                let username = parts
                    .headers
                    .get("X-User")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "anonymous".to_string());

                debug!("Simple auth: username={}", username);

                Ok(UserIdentity {
                    username,
                    auth_mode,
                })
            },

            AuthMode::Token => {
                // Token mode - validate Kubernetes token
                let auth_header = parts
                    .headers
                    .get(AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    .ok_or_else(|| AuthError {
                        error: "Missing Authorization header".to_string(),
                    })?;

                let token = auth_header
                    .strip_prefix("Bearer ")
                    .ok_or_else(|| AuthError {
                        error: "Invalid Authorization header format. Expected: Bearer <token>"
                            .to_string(),
                    })?;

                let username = validate_token_and_get_username(token).await.map_err(|e| {
                    error!("Token validation failed: {}", e);
                    AuthError {
                        error: format!("Token validation failed: {}", e),
                    }
                })?;

                debug!("Token auth: username={}", username);

                Ok(UserIdentity {
                    username,
                    auth_mode,
                })
            },

            AuthMode::Proxy => {
                // Proxy mode - read username from configured header
                let header_name = AuthMode::proxy_header();
                let username = parts
                    .headers
                    .get(&header_name)
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| AuthError {
                        error: format!("Missing {} header", header_name),
                    })?
                    .to_string();

                debug!("Proxy auth: username={}", username);

                Ok(UserIdentity {
                    username,
                    auth_mode,
                })
            },
        }
    }
}

/// Validate a Kubernetes token and extract the username
async fn validate_token_and_get_username(token: &str) -> Result<String, String> {
    // Create a Kubernetes client using the operator's service account
    let client = Client::try_default()
        .await
        .map_err(|e| format!("Failed to create Kubernetes client: {}", e))?;

    // Create a TokenReview request
    let token_review = TokenReview {
        metadata: Default::default(),
        spec: TokenReviewSpec {
            token: Some(token.to_string()),
            audiences: None,
        },
        status: None,
    };

    // Submit the TokenReview
    let api: Api<TokenReview> = Api::all(client);
    let result = api
        .create(&PostParams::default(), &token_review)
        .await
        .map_err(|e| format!("TokenReview API call failed: {}", e))?;

    // Check if token is authenticated
    let status = result.status.ok_or("TokenReview returned no status")?;

    if !status.authenticated.unwrap_or(false) {
        return Err("Token is not authenticated".to_string());
    }

    // Extract username
    let user_info = status.user.ok_or("TokenReview returned no user info")?;
    let username = user_info.username.ok_or("Username not found in token")?;

    Ok(username)
}

/// Audit log entry for tracking user actions in the Web UI
#[derive(Debug, Serialize)]
pub struct AuditLogEntry {
    pub timestamp: String,
    pub username: String,
    pub action: String,
    pub resource_type: String,
    pub resource_namespace: String,
    pub resource_name: String,
    pub result: String,
    pub reason: Option<String>,
}

impl AuditLogEntry {
    pub fn new(
        username: String,
        action: String,
        resource_type: String,
        resource_namespace: String,
        resource_name: String,
        result: String,
        reason: Option<String>,
    ) -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            username,
            action,
            resource_type,
            resource_namespace,
            resource_name,
            result,
            reason,
        }
    }

    /// Log this audit entry to the audit log
    pub fn log(&self) {
        info!(
            target: "headwind::audit",
            timestamp = %self.timestamp,
            username = %self.username,
            action = %self.action,
            resource_type = %self.resource_type,
            namespace = %self.resource_namespace,
            name = %self.resource_name,
            result = %self.result,
            reason = ?self.reason,
            "Audit log entry"
        );
    }
}
