//! Bearer token authentication helpers.

use cab_core::CabError;

pub fn extract_bearer_token(authorization: Option<&str>) -> Option<String> {
    authorization.and_then(|value| {
        value
            .strip_prefix("Bearer ")
            .or_else(|| value.strip_prefix("bearer "))
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .map(str::to_string)
    })
}

pub fn extract_gateway_token(
    authorization: Option<&str>,
    x_api_key: Option<&str>,
) -> Option<String> {
    extract_bearer_token(authorization).or_else(|| {
        x_api_key
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .map(str::to_string)
    })
}

pub async fn verify(
    pool: &crate::InMemoryStore,
    authorization: Option<&str>,
) -> Result<(), CabError> {
    verify_with_api_key(pool, authorization, None).await
}

pub async fn verify_with_api_key(
    pool: &crate::InMemoryStore,
    authorization: Option<&str>,
    x_api_key: Option<&str>,
) -> Result<(), CabError> {
    let settings = crate::settings::get(pool)
        .await
        .map_err(CabError::Database)?;
    if !settings.auth_enabled {
        return Ok(());
    }

    let token = extract_gateway_token(authorization, x_api_key).ok_or(CabError::Unauthorized)?;
    if token != settings.gateway_key {
        return Err(CabError::Unauthorized);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_bearer_token() {
        assert_eq!(
            extract_bearer_token(Some("Bearer cab-token-abc")).as_deref(),
            Some("cab-token-abc")
        );
    }

    #[test]
    fn extract_gateway_token_accepts_x_api_key() {
        assert_eq!(
            extract_gateway_token(None, Some("cab-token-abc")).as_deref(),
            Some("cab-token-abc")
        );
        assert_eq!(
            extract_gateway_token(Some("Bearer bearer-wins"), Some("cab-token-abc")).as_deref(),
            Some("bearer-wins")
        );
    }
}
