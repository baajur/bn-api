use actix_web::Query;
use actix_web::{HttpResponse, State};
use auth::user::User as AuthUser;
use auth::TokenResponse;
use bigneon_db::prelude::*;
use db::Connection;
use errors::*;
use extractors::*;
use facebook::nodes::Event as FBEvent;
use facebook::prelude::{CoverPhoto, FacebookClient, FBID};
use helpers::application;
use itertools::Itertools;
use log::Level::Debug;
use models::FacebookWebLoginToken;
use reqwest;
use serde_json;
use server::AppState;
use uuid::Uuid;

const FACEBOOK_GRAPH_URL: &str = "https://graph.facebook.com";
#[derive(Deserialize)]
struct FacebookGraphResponse {
    id: String,
    first_name: String,
    last_name: String,
    email: Option<String>,
}

// TODO: Not covered by tests
pub fn web_login(
    (state, connection, auth_token, auth_user): (
        State<AppState>,
        Connection,
        Json<FacebookWebLoginToken>,
        OptionalUser,
    ),
) -> Result<HttpResponse, BigNeonError> {
    let url = format!(
        "{}/me?fields=id,email,first_name,last_name",
        FACEBOOK_GRAPH_URL
    );
    let connection = connection.get();
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header(
            "Authorization",
            format!("Bearer {}", auth_token.access_token),
        )
        .send()?
        .text()?;

    jlog!(Debug, "Facebook Login Response", { "response": &response });

    let facebook_graph_response: FacebookGraphResponse = serde_json::from_str(&response)?;

    if auth_token.link_to_user_id {
        let auth_user = auth_user.into_inner();
        if auth_user.is_none() {
            return application::unauthorized_with_message(
                "User must be logged in to link Facebook",
                auth_user,
                None,
            );
        }

        let auth_user = auth_user.unwrap();
        auth_user.user.add_or_replace_external_login(
            Some(auth_user.user.id),
            facebook_graph_response.id.clone(),
            FACEBOOK_SITE.to_string(),
            auth_token.access_token.clone(),
            vec![],
            connection,
        )?;
        let response = TokenResponse::create_from_user(
            &state.config.token_secret,
            &state.config.token_issuer,
            &state.config.jwt_expiry_time,
            &auth_user.user,
        )?;
        return Ok(HttpResponse::Ok().json(response));
    }

    let existing_user =
        ExternalLogin::find_user(&facebook_graph_response.id, FACEBOOK_SITE, connection)?;
    let user = match existing_user {
        Some(u) => User::find(u.user_id, connection)?,
        None => {
            if let Some(email) = facebook_graph_response.email.as_ref() {
                // Link account if email exists
                match User::find_by_email(&email, connection) {
                    Ok(user) => {
                        user.add_external_login(
                            None,
                            facebook_graph_response.id.clone(),
                            FACEBOOK_SITE.to_string(),
                            auth_token.access_token.clone(),
                            vec![],
                            connection,
                        )?;
                        user
                    }
                    Err(e) => {
                        match e.code {
                            // Not found
                            2000 => User::create_from_external_login(
                                facebook_graph_response.id.clone(),
                                facebook_graph_response.first_name.clone(),
                                facebook_graph_response.last_name.clone(),
                                facebook_graph_response.email.clone(),
                                FACEBOOK_SITE.to_string(),
                                auth_token.access_token.clone(),
                                vec![],
                                None,
                                connection,
                            )?,
                            _ => return Err(e.into()),
                        }
                    }
                }
            } else {
                User::create_from_external_login(
                    facebook_graph_response.id.clone(),
                    facebook_graph_response.first_name.clone(),
                    facebook_graph_response.last_name.clone(),
                    None,
                    FACEBOOK_SITE.to_string(),
                    auth_token.access_token.clone(),
                    vec![],
                    None,
                    connection,
                )?
            }
        }
    };
    let response = TokenResponse::create_from_user(
        &state.config.token_secret,
        &state.config.token_issuer,
        &state.config.jwt_expiry_time,
        &user,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

pub fn request_manage_page_access(
    (_connection, state, user): (Connection, State<AppState>, AuthUser),
) -> Result<HttpResponse, BigNeonError> {
    // TODO Sign/encrypt the user id passed through so that we can verify it has not been spoofed
    let redirect_url = FacebookClient::get_login_url(
        state.config.facebook_app_id.as_ref().ok_or_else(|| {
            ApplicationError::new_with_type(
                ApplicationErrorType::Unprocessable,
                "Facebook App ID has not been configured".to_string(),
            )
        })?,
        None,
        &user.id().to_string(),
        &["manage-pages"],
    );

    #[derive(Serialize)]
    struct R {
        redirect_url: String,
    };

    let r = R { redirect_url };

    Ok(HttpResponse::Ok().json(r))
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct AuthCallbackPathParameters {
    code: Option<String>,
    state: Option<String>,
    error_reason: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

/// Callback for converting an FB code into an access token
pub fn auth_callback(
    (query, state, connection): (
        Query<AuthCallbackPathParameters>,
        State<AppState>,
        Connection,
    ),
) -> Result<HttpResponse, BigNeonError> {
    info!("Auth callback received");
    if query.error.is_some() {
        return Err(ApplicationError::new_with_type(
            ApplicationErrorType::Internal,
            "Facebook login failed".to_string(),
        )
        .into());
    }

    let app_id = state.config.facebook_app_id.as_ref().ok_or_else(|| {
        ApplicationError::new_with_type(
            ApplicationErrorType::ServerConfigError,
            "Facebook App ID has not been configured".to_string(),
        )
    })?;

    let app_secret = state.config.facebook_app_secret.as_ref().ok_or_else(|| {
        ApplicationError::new_with_type(
            ApplicationErrorType::ServerConfigError,
            "Facebook App secret has not been configured".to_string(),
        )
    })?;

    let conn = connection.get();

    let _user = match query.state.as_ref() {
        Some(user_id) => {
            // TODO check signature of state to make sure it was sent from us
            User::find(user_id.parse()?, conn)?
        }
        _ => {
            return Err(ApplicationError::new_with_type(
                ApplicationErrorType::BadRequest,
                "State was not provided from Facebook".to_string(),
            )
            .into());
        }
    };

    // Note this must be the same as the redirect url used to in the original call.
    let redirect_url = None;

    let _access_token = FacebookClient::get_access_token(
        app_id,
        app_secret,
        redirect_url,
        query.code.as_ref().ok_or_else(|| {
            ApplicationError::new_with_type(
                ApplicationErrorType::Internal,
                "Code was not provided from Facebook".to_string(),
            )
        })?,
    )?;

    unimplemented!()
    //user.add_external_login()
}

/// Returns a list of pages that a user has access to manage
pub fn pages((connection, user): (Connection, AuthUser)) -> Result<HttpResponse, BigNeonError> {
    let conn = connection.get();
    let db_user = user.user;
    let fb_login = db_user
        .find_external_login(FACEBOOK_SITE, conn)
        .optional()?;
    if fb_login.is_none() {
        return application::forbidden("User is not linked to Facebook");
    }
    let fb_login = fb_login.unwrap();

    let client = FacebookClient::from_access_token(fb_login.access_token.clone());
    let permissions = client.permissions.list(&fb_login.external_user_id)?;
    let list_pages_permission = permissions
        .data
        .iter()
        .find(|p| p.permission == "pages_show_list");
    if list_pages_permission
        .as_ref()
        .map(|p| &p.status)
        .unwrap_or(&"declined".to_string())
        != "granted"
    {
        return application::forbidden("User must be granted access to view pages on Facebook");
    }

    let pages = client
        .me
        .accounts
        .list()?
        .into_iter()
        .map(|p| FacebookPage {
            id: p.id,
            name: p.name,
        })
        .collect_vec();
    Ok(HttpResponse::Ok().json(pages))
}

#[derive(Serialize)]
pub struct FacebookPage {
    pub id: String,
    pub name: String,
}

pub fn scopes((connection, user): (Connection, AuthUser)) -> Result<HttpResponse, BigNeonError> {
    let conn = connection.get();
    let db_user = user.user;
    let external_login = db_user.find_external_login(FACEBOOK_SITE, conn)?;
    Ok(HttpResponse::Ok().json(external_login.scopes))
}

pub fn create_event(
    (connection, user, data): (Connection, AuthUser, Json<CreateFacebookEvent>),
) -> Result<HttpResponse, BigNeonError> {
    let conn = connection.get();
    let event = Event::find(data.event_id, conn)?;
    if !event.is_published() {
        return Err(ApplicationError::unprocessable(
            "Cannot create this event on Facebook until it is published",
        )
        .into());
    }
    let mut organization = event.organization(conn)?;
    user.requires_scope_for_organization_event(Scopes::EventWrite, &organization, &event, conn)?;

    let client = FacebookClient::from_access_token(
        user.user
            .find_external_login(FACEBOOK_SITE, conn)?
            .access_token,
    );

    let fb_event = FBEvent::new(
        data.category.parse()?,
        event.name.clone(),
        event.additional_info.clone().unwrap_or("".to_string()),
        FBID(data.page_id.clone()),
        event
            .venue(conn)?
            .ok_or_else(|| {
                ApplicationError::unprocessable(
                    "Cannot publish this event on Facebook without a venue",
                )
            })?
            .timezone,
        event
            .cover_image_url
            .as_ref()
            .map(|u| CoverPhoto::new(u.to_string())),
        event
            .event_start
            .ok_or_else(|| {
                ApplicationError::unprocessable(
                    "Cannot publish this event in Facebook without a start time",
                )
            })?
            .to_string(),
    );
    let _fb_id = client.official_events.create(fb_event)?;

    // Save fb_id onto event
    unimplemented!();
}

#[derive(Deserialize)]
pub struct CreateFacebookEvent {
    event_id: Uuid,
    page_id: String,
    category: String,
}
