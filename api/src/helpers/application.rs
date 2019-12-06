use actix_web::{http, http::StatusCode, Body::Binary, HttpResponse, Responder};
use auth::user::User as AuthUser;
use cache::CacheConnection;
use config::Config;
use errors::*;
use serde::Serialize;
use serde_json::{self, Value};
use std::borrow::Borrow;
use std::collections::HashMap;
use std::str;

pub fn unauthorized<T: Responder>(
    user: Option<AuthUser>,
    additional_data: Option<HashMap<&'static str, Value>>,
) -> Result<T, BigNeonError> {
    unauthorized_with_message("User does not have the required permissions", user, additional_data)
}

pub fn unauthorized_with_message<T: Responder>(
    message: &str,
    auth_user: Option<AuthUser>,
    additional_data: Option<HashMap<&'static str, Value>>,
) -> Result<T, BigNeonError> {
    if let Some(auth_user) = auth_user {
        auth_user.log_unauthorized_access_attempt(additional_data.unwrap_or(HashMap::new()));
    }

    Err(AuthError::new(AuthErrorType::Unauthorized, message.into()).into())
}

pub fn forbidden<T: Responder>(message: &str) -> Result<T, BigNeonError> {
    Err(AuthError::new(AuthErrorType::Forbidden, message.into()).into())
}

pub fn unprocessable<T: Responder>(message: &str) -> Result<T, BigNeonError> {
    Err(ApplicationError::new_with_type(ApplicationErrorType::Unprocessable, message.to_string()).into())
}
pub fn bad_request<T: Responder>(message: &str) -> Result<T, BigNeonError> {
    Err(ApplicationError::new_with_type(ApplicationErrorType::BadRequest, message.to_string()).into())
}

pub fn internal_server_error<T: Responder>(message: &str) -> Result<T, BigNeonError> {
    error!("Internal Server Error: {}", message);
    Err(ApplicationError::new(message.to_string()).into())
}

pub fn no_content() -> Result<HttpResponse, BigNeonError> {
    Ok(HttpResponse::new(StatusCode::NO_CONTENT))
}

pub fn not_found() -> Result<HttpResponse, BigNeonError> {
    warn!("Not found");
    Ok(HttpResponse::new(StatusCode::NOT_FOUND))
}

pub fn created(json: serde_json::Value) -> Result<HttpResponse, BigNeonError> {
    Ok(HttpResponse::new(StatusCode::CREATED).into_builder().json(json))
}

pub fn redirect(url: &str) -> Result<HttpResponse, BigNeonError> {
    Ok(HttpResponse::Found().header(http::header::LOCATION, url).finish())
}
pub fn unwrap_body_to_string(response: &HttpResponse) -> Result<&str, &'static str> {
    match response.body() {
        Binary(binary) => Ok(str::from_utf8(binary.as_ref()).unwrap()),
        _ => Err("Unexpected response body"),
    }
}
// Redis cache helper functions
pub(crate) fn set_cached_value<T: Serialize>(
    mut cache_connection: impl CacheConnection,
    config: &Config,
    http_response: &HttpResponse,
    query: &T,
) -> Result<(), BigNeonError> {
    let body = unwrap_body_to_string(http_response).map_err(|e| ApplicationError::new(e.to_string()))?;
    let cache_period = config.cache_period_milli.clone();
    let query_serialized = serde_json::to_string(query)?;
    let payload_json = serde_json::to_string(body)?;
    cache_connection
        .add(query_serialized.borrow(), payload_json.borrow(), cache_period)
        .ok();
    Ok(())
}

pub(crate) fn get_cached_value<T: Serialize>(
    mut cache_connection: impl CacheConnection,
    config: &Config,
    query: T,
) -> Option<HttpResponse> {
    let cache_period = config.cache_period_milli.clone();
    let query_serialized = serde_json::to_string(&query).ok()?;
    // only look for cached value if a cached_period is giving
    if cache_period.is_some() {
        let cached_value = cache_connection.get(query_serialized.borrow()).ok()?;
        return Some(HttpResponse::Ok().json(&cached_value));
    }
    None
}
