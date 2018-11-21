pub mod database;
pub mod test_request;

use actix_web::{http::StatusCode, Body::Binary, HttpResponse};
use bigneon_api::auth::user::User as AuthUser;
use bigneon_db::models::{Organization, Roles, User};
use serde_json;
use std::collections::HashMap;
use std::str;
use support::database::TestDatabase;
use support::test_request::TestRequest;
use validator::ValidationError;

#[derive(Debug, Deserialize)]
pub struct ValidationResponse {
    pub error: String,
    pub fields: HashMap<String, Vec<ValidationError>>,
}

pub fn unwrap_body_to_string(response: &HttpResponse) -> Result<&str, &'static str> {
    match response.body() {
        Binary(binary) => Ok(str::from_utf8(binary.as_ref()).unwrap()),
        _ => Err("Unexpected response body"),
    }
}

pub fn validation_response_from_response(
    response: &HttpResponse,
) -> Result<ValidationResponse, &'static str> {
    Ok(serde_json::from_str(unwrap_body_to_string(response)?).unwrap())
}

pub fn create_auth_user(
    role: Roles,
    organization: Option<&Organization>,
    database: &TestDatabase,
) -> AuthUser {
    let user_for_auth = database.create_user().finish();
    create_auth_user_from_user(&user_for_auth, role, organization, database)
}

pub fn create_auth_user_from_user(
    user: &User,
    role: Roles,
    organization: Option<&Organization>,
    database: &TestDatabase,
) -> AuthUser {
    let test_request = TestRequest::create();
    if [Roles::Admin, Roles::User].contains(&role) {
        let user = user.add_role(role, &database.connection).unwrap();
        AuthUser::new(user, &test_request.request)
    } else {
        let organization = match organization {
            Some(organization) => (*organization).clone(),
            None => database.create_organization().finish(),
        };

        if role == Roles::OrgOwner {
            organization
                .set_owner(user.id, &database.connection)
                .unwrap();
        } else {
            organization
                .add_user(user.id, &database.connection)
                .unwrap();
        }

        AuthUser::new(user.clone(), &test_request.request)
    }
}

pub fn expects_unauthorized(response: &HttpResponse) {
    let expected_json = HttpResponse::Unauthorized()
        .json(json!({"error": "User does not have the required permissions"}));
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let expected_text = unwrap_body_to_string(&expected_json).unwrap();
    let body = unwrap_body_to_string(&response).unwrap();
    assert_eq!(body, expected_text);
}

pub fn expects_forbidden(response: &HttpResponse, message: Option<&str>) {
    let expected_json = HttpResponse::Forbidden().json(json!({
        "error": message.unwrap_or("You do not have access to this order")
    }));
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let expected_text = unwrap_body_to_string(&expected_json).unwrap();
    let body = unwrap_body_to_string(&response).unwrap();
    assert_eq!(body, expected_text);
}
