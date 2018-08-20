use actix_web::Query;
use actix_web::{http::StatusCode, FromRequest, Path};
use bigneon_api::controllers::events::SearchParameters;
use bigneon_api::controllers::events::{self, PathParameters};
use bigneon_api::database::ConnectionGranting;
use bigneon_db::models::{Artist, Event, Organization, Roles, User, Venue};
use chrono::prelude::*;
use functional::base;
use serde_json;
use support;
use support::database::TestDatabase;
use support::test_request::TestRequest;

#[test]
pub fn index() {
    let database = TestDatabase::new();
    let user = User::create(
        "Jeff",
        "Roen",
        "jeff@tari.com",
        "555-555-5555",
        "examplePassword",
    ).commit(&*database.get_connection())
        .unwrap();
    let artist = Artist::create("Example")
        .commit(&*database.get_connection())
        .unwrap();
    let organization = Organization::create(user.id, "Organization")
        .commit(&*database.get_connection())
        .unwrap();
    let venue = Venue::create(&"Venue")
        .commit(&*database.get_connection())
        .unwrap();
    let event = Event::create(
        "NewEvent",
        organization.id,
        venue.id,
        NaiveDate::from_ymd(2016, 7, 8).and_hms(9, 10, 11),
    ).commit(&*database.get_connection())
        .unwrap();
    event
        .add_artist(artist.id, &*database.get_connection())
        .unwrap();
    let event2 = Event::create(
        "NewEvent2",
        organization.id,
        venue.id,
        NaiveDate::from_ymd(2015, 7, 8).and_hms(9, 10, 11),
    ).commit(&*database.get_connection())
        .unwrap();
    event2
        .add_artist(artist.id, &*database.get_connection())
        .unwrap();

    let expected_events = vec![event2, event];
    let events_expected_json = serde_json::to_string(&expected_events).unwrap();

    let test_request = TestRequest::create_with_uri(database, "/events?name=New");
    let state = test_request.extract_state();
    let query = Query::<SearchParameters>::from_request(&test_request.request, &()).unwrap();
    let response = events::index((state, query));

    let body = support::unwrap_body_to_string(&response).unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body, events_expected_json);
}

#[test]
pub fn show() {
    let database = TestDatabase::new();
    let connection = database.get_connection();
    let user = User::create(
        "Jeff",
        "Roen",
        "jeff@tari.com",
        "555-555-5555",
        "examplePassword",
    ).commit(&*connection)
        .unwrap();
    let organization = Organization::create(user.id, "Organization")
        .commit(&*connection)
        .unwrap();
    let venue = Venue::create(&"Venue").commit(&*connection).unwrap();
    let event = Event::create(
        "NewEvent",
        organization.id,
        venue.id,
        NaiveDate::from_ymd(2016, 7, 8).and_hms(9, 10, 11),
    ).commit(&*connection)
        .unwrap();
    let event_expected_json = serde_json::to_string(&event).unwrap();

    let test_request = TestRequest::create(database);
    let mut path = Path::<PathParameters>::extract(&test_request.request).unwrap();
    path.id = event.id;
    let state = test_request.extract_state();

    let response = events::show((state, path));

    let body = support::unwrap_body_to_string(&response).unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body, event_expected_json);
}

#[cfg(test)]
mod create_tests {
    use super::*;
    #[test]
    fn create_org_member() {
        base::events::create(Roles::OrgMember, true);
    }
    #[test]
    fn create_guest() {
        base::events::create(Roles::Guest, false);
    }
    #[test]
    fn create_admin() {
        base::events::create(Roles::Admin, true);
    }
    #[test]
    fn create_user() {
        base::events::create(Roles::User, false);
    }
    #[test]
    fn create_org_owner() {
        base::events::create(Roles::OrgOwner, true);
    }
}

#[cfg(test)]
mod update_tests {
    use super::*;
    #[test]
    fn update_org_member() {
        base::events::update(Roles::OrgMember, true);
    }
    #[test]
    fn update_guest() {
        base::events::update(Roles::Guest, false);
    }
    #[test]
    fn update_admin() {
        base::events::update(Roles::Admin, true);
    }
    #[test]
    fn update_user() {
        base::events::update(Roles::User, false);
    }
    #[test]
    fn update_org_owner() {
        base::events::update(Roles::OrgOwner, true);
    }
}

#[test]
pub fn show_from_organizations() {
    let database = TestDatabase::new();
    let connection = database.get_connection();
    //create prerequisites
    let user = User::create(
        "Jeff",
        "Roen",
        "jeff@tari.com",
        "555-555-5555",
        "examplePassword",
    ).commit(&*connection)
        .unwrap();
    let organization = Organization::create(user.id, "Organization")
        .commit(&*connection)
        .unwrap();
    let venue = Venue::create(&"Venue").commit(&*connection).unwrap();
    let event = Event::create(
        "NewEvent",
        organization.id,
        venue.id,
        NaiveDate::from_ymd(2016, 7, 8).and_hms(9, 10, 11),
    ).commit(&*connection)
        .unwrap();
    let event2 = Event::create(
        "NewEvent2",
        organization.id,
        venue.id,
        NaiveDate::from_ymd(2016, 7, 8).and_hms(9, 10, 11),
    ).commit(&*connection)
        .unwrap();

    let all_events = vec![event, event2];
    let event_expected_json = serde_json::to_string(&all_events).unwrap();
    //find venue from organization
    let test_request = TestRequest::create(database);
    let state = test_request.extract_state();

    let mut path = Path::<PathParameters>::extract(&test_request.request).unwrap();
    path.id = organization.id;
    let response = events::show_from_organizations((state, path));

    let body = support::unwrap_body_to_string(&response).unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body, event_expected_json);
}

#[test]
pub fn show_from_venues() {
    let database = TestDatabase::new();
    let connection = database.get_connection();
    //create prerequisites
    let user = User::create(
        "Jeff",
        "Roen",
        "jeff@tari.com",
        "555-555-5555",
        "examplePassword",
    ).commit(&*connection)
        .unwrap();
    let organization = Organization::create(user.id, "Organization")
        .commit(&*connection)
        .unwrap();
    let venue = Venue::create(&"Venue").commit(&*connection).unwrap();
    let event = Event::create(
        "NewEvent",
        organization.id,
        venue.id,
        NaiveDate::from_ymd(2016, 7, 8).and_hms(9, 10, 11),
    ).commit(&*connection)
        .unwrap();
    let event2 = Event::create(
        "NewEvent2",
        organization.id,
        venue.id,
        NaiveDate::from_ymd(2016, 7, 8).and_hms(9, 10, 11),
    ).commit(&*connection)
        .unwrap();
    //find venue from organization

    let all_events = vec![event, event2];
    let event_expected_json = serde_json::to_string(&all_events).unwrap();
    //find venue from organization
    let test_request = TestRequest::create(database);
    let state = test_request.extract_state();

    let mut path = Path::<PathParameters>::extract(&test_request.request).unwrap();
    path.id = venue.id;
    let response = events::show_from_venues((state, path));

    let body = support::unwrap_body_to_string(&response).unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body, event_expected_json);
}