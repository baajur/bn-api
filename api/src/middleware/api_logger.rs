use crate::utils::logging::{log_request, RequestLogData};
use actix_service::Service;
use actix_web::http::StatusCode;
use actix_web::{dev, error};
use futures::future::{ok, Ready};
use log::Level;

pub struct ApiLogger;

impl ApiLogger {
    pub fn new() -> Self {
        Self {}
    }

    // log message at the start of request lifecycle
    pub fn start(sreq: &dev::ServiceRequest) -> RequestLogData {
        let data: RequestLogData = sreq.into();
        if data.uri != "/status" {
            log_request(
                Level::Info,
                "api::big_neon_logger",
                format!("{} {} starting", data.method, data.uri).as_str(),
                &data,
                (),
            );
        };
        data
    }

    // log message at the end of request lifecycle
    pub fn finish<B>(
        data: &RequestLogData,
        resp: error::Result<dev::ServiceResponse<B>>,
    ) -> error::Result<dev::ServiceResponse<B>> {
        let error = match resp {
            Err(ref error) => Some(error),
            Ok(ref resp) => resp.response().error(),
        };
        if let Some(error) = error {
            let level = match error.as_response_error().status_code() {
                StatusCode::UNAUTHORIZED => Level::Info,
                s if s.is_client_error() => Level::Warn,
                _ => Level::Error,
            };
            log_request(level, "api::big_neon_logger", &error.to_string(), data, ());
        };
        resp
    }
}

impl<S, B> dev::Transform<S> for ApiLogger
where
    S: Service<Request = dev::ServiceRequest, Response = dev::ServiceResponse<B>, Error = error::Error> + 'static,
    B: dev::MessageBody,
{
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type InitError = ();
    type Transform = LoggerService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(LoggerService::new(service))
    }
}

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

pub struct LoggerService<S> {
    service: Rc<RefCell<S>>,
}

impl<S> LoggerService<S> {
    fn new(service: S) -> Self {
        Self {
            service: Rc::new(RefCell::new(service)),
        }
    }
}

impl<S, B> Service for LoggerService<S>
where
    S: Service<Request = dev::ServiceRequest, Response = dev::ServiceResponse<B>, Error = error::Error> + 'static,
    B: dev::MessageBody,
{
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.borrow_mut().poll_ready(cx).map_err(error::Error::from)
    }

    fn call(&mut self, request: Self::Request) -> Self::Future {
        let service = self.service.clone();
        Box::pin(async move {
            let data = ApiLogger::start(&request);
            let fut = service.borrow_mut().call(request);
            let response = fut.await;
            ApiLogger::finish(&data, response)
        })
    }
}
