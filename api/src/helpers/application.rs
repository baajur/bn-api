use crate::auth::user::User as AuthUser;
use crate::errors::*;
use actix_web::dev::{self, Body, BodySize, MessageBody};
use actix_web::{error, http, http::StatusCode, HttpResponse, Responder};
use bytes::{BufMut, Bytes, BytesMut};
use futures::stream::StreamExt;
use serde_json::{self, Value};
use std::collections::HashMap;
use std::str;

const DEFAULT_BUF_SIZE: usize = 64 * 1024;

pub fn unauthorized<T: Responder>(
    user: Option<AuthUser>,
    additional_data: Option<HashMap<&'static str, Value>>,
) -> Result<T, ApiError> {
    unauthorized_with_message("User does not have the required permissions", user, additional_data)
}

pub fn unauthorized_with_message<T: Responder>(
    message: &str,
    auth_user: Option<AuthUser>,
    additional_data: Option<HashMap<&'static str, Value>>,
) -> Result<T, ApiError> {
    if let Some(auth_user) = auth_user {
        auth_user.log_unauthorized_access_attempt(additional_data.unwrap_or(HashMap::new()));
    }

    Err(AuthError::new(AuthErrorType::Unauthorized, message.into()).into())
}

pub fn forbidden<T: Responder>(message: &str) -> Result<T, ApiError> {
    Err(AuthError::new(AuthErrorType::Forbidden, message.into()).into())
}

pub fn unprocessable<T: Responder>(message: &str) -> Result<T, ApiError> {
    Err(ApplicationError::new_with_type(ApplicationErrorType::Unprocessable, message.to_string()).into())
}
pub fn bad_request<T: Responder>(message: &str) -> Result<T, ApiError> {
    Err(ApplicationError::new_with_type(ApplicationErrorType::BadRequest, message.to_string()).into())
}

pub fn internal_server_error<T: Responder>(message: &str) -> Result<T, ApiError> {
    error!("Internal Server Error: {}", message);
    Err(ApplicationError::new(message.to_string()).into())
}

pub fn no_content() -> Result<HttpResponse, ApiError> {
    Ok(HttpResponse::new(StatusCode::NO_CONTENT))
}

pub fn not_found() -> Result<HttpResponse, ApiError> {
    warn!("Not found");
    Ok(HttpResponse::new(StatusCode::NOT_FOUND))
}
pub fn method_not_allowed() -> Result<HttpResponse, ApiError> {
    warn!("Method not allowed");
    Ok(HttpResponse::new(StatusCode::METHOD_NOT_ALLOWED))
}

pub fn created(json: serde_json::Value) -> Result<HttpResponse, ApiError> {
    Ok(HttpResponse::Created().json(json))
}

pub fn redirect(url: &str) -> Result<HttpResponse, ApiError> {
    Ok(HttpResponse::Found().header(http::header::LOCATION, url).finish())
}

pub fn unwrap_body_to_string(response: &HttpResponse) -> Result<&str, &'static str> {
    match response.body() {
        dev::ResponseBody::Body(Body::Bytes(binary)) | dev::ResponseBody::Other(Body::Bytes(binary)) => {
            match str::from_utf8(binary.as_ref()) {
                Ok(value) => Ok(value),
                Err(_) => Err("Unable to unwrap body"),
            }
        }
        _ => Err("Unexpected response body"),
    }
}

pub async fn extract_response_bytes<B: MessageBody>(body: &mut dev::ResponseBody<B>) -> Result<Bytes, error::Error> {
    let size_hint = match body.size() {
        BodySize::Sized(n) => n,
        _ => DEFAULT_BUF_SIZE,
    };
    let mut buf = BytesMut::with_capacity(size_hint);
    while let Some(item) = body.next().await {
        buf.put(item?);
    }
    Ok(buf.freeze())
}
