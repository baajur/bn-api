use crate::extractors::AccessTokenExtractor;
use actix_web::{dev, http::header, HttpRequest};
use log::Level;
use serde_json::{json, Value};

pub fn log_request<IM, IV>(level: Level, module: &str, message: &str, req: IM, data: IV)
where
    IM: Into<LogMeta>,
    IV: Into<Value>,
{
    let meta: LogMeta = (req, data.into()).into();
    logging::transform_message(level, Some(module), message, Some(meta.into()));
}

pub struct LogMeta(Value);

impl From<&RequestLogData> for LogMeta {
    fn from(data: &RequestLogData) -> Self {
        LogMeta(json!({
            "user_id": data.user,
            "ip_address": data.ip_address,
            "uri": data.uri,
            "method": data.method,
            "api_version": env!("CARGO_PKG_VERSION"),
            "user_agent": data.user_agent
        }))
    }
}

impl From<&HttpRequest> for LogMeta {
    fn from(req: &HttpRequest) -> Self {
        let data: RequestLogData = req.into();
        (&data).into()
    }
}

impl<IM> From<(IM, Value)> for LogMeta
where
    IM: Into<LogMeta>,
{
    fn from(data: (IM, Value)) -> Self {
        let (mut meta, mut value) = (data.0.into(), data.1);
        if value.is_object() {
            let map = value.as_object_mut().unwrap();
            meta.0.as_object_mut().map(|v| v.append(map));
        } else {
            meta.0.as_object_mut().map(|v| v.insert("custom".to_string(), value));
        }
        meta
    }
}

impl From<LogMeta> for Value {
    fn from(meta: LogMeta) -> Value {
        meta.0
    }
}

pub struct RequestLogData {
    pub user: Option<uuid::Uuid>,
    pub ip_address: Option<String>,
    pub method: String,
    pub user_agent: Option<String>,
    pub uri: String,
}

impl From<&dev::ServiceRequest> for RequestLogData {
    fn from(req: &dev::ServiceRequest) -> Self {
        let uri = req.uri().to_string();
        let user = AccessTokenExtractor::from_request(req)
            .ok()
            .map(|token| token.get_id().ok())
            .flatten();
        let ip_address = req.connection_info().remote().map(|i| i.to_string());
        let method = req.method().to_string();
        let user_agent = if let Some(ua) = req.headers().get(header::USER_AGENT) {
            let s = ua.to_str().unwrap_or("");
            Some(s.to_string())
        } else {
            None
        };
        Self {
            user,
            ip_address,
            method,
            user_agent,
            uri,
        }
    }
}

impl From<&HttpRequest> for RequestLogData {
    fn from(req: &HttpRequest) -> Self {
        let uri = req.uri().to_string();
        let user = AccessTokenExtractor::from_request(req)
            .ok()
            .map(|token| token.get_id().ok())
            .flatten();
        let ip_address = req.connection_info().remote().map(|i| i.to_string());
        let method = req.method().to_string();
        let user_agent = if let Some(ua) = req.headers().get(header::USER_AGENT) {
            let s = ua.to_str().unwrap_or("");
            Some(s.to_string())
        } else {
            None
        };
        Self {
            user,
            ip_address,
            method,
            user_agent,
            uri,
        }
    }
}
