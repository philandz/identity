use tonic::{Request, Status};

/// Extract the raw Bearer token from the `authorization` gRPC metadata header.
#[allow(clippy::result_large_err)]
pub fn extract_bearer_token<T>(request: &Request<T>) -> Result<String, Status> {
    let auth = request
        .metadata()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| Status::unauthenticated("Missing authorization metadata"))?;

    auth.strip_prefix("Bearer ")
        .map(|t| t.to_string())
        .ok_or_else(|| Status::unauthenticated("Authorization metadata must start with 'Bearer '"))
}

/// Extract the `x-user-id` metadata header injected by the gateway.
#[allow(clippy::result_large_err)]
pub fn user_id_from_metadata(metadata: &tonic::metadata::MetadataMap) -> Result<String, Status> {
    metadata
        .get("x-user-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .ok_or_else(|| Status::unauthenticated("Missing x-user-id metadata"))
}
