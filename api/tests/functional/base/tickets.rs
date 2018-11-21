use actix_web::{http::StatusCode, FromRequest, HttpResponse, Json, Path};
use bigneon_api::controllers::tickets::{self, ShowTicketResponse, TicketRedeemRequest};
use bigneon_api::models::PathParameters;
use bigneon_db::models::*;
use serde_json;
use support;
use support::database::TestDatabase;
use support::test_request::TestRequest;

pub fn show_other_user_ticket(role: Roles, should_test_succeed: bool) {
    let database = TestDatabase::new();
    let user = database.create_user().finish();
    let request = TestRequest::create();
    let organization = database.create_organization().finish();
    let auth_user =
        support::create_auth_user_from_user(&user, role, Some(&organization), &database);

    let event = database
        .create_event()
        .with_organization(&organization)
        .with_ticket_pricing()
        .finish();
    let user2 = database.create_user().finish();
    let conn = &database.connection;
    let ticket_type = event.ticket_types(conn).unwrap().remove(0);
    let ticket = database
        .create_purchased_tickets(&user2, ticket_type.id, 1)
        .remove(0);

    let mut path = Path::<PathParameters>::extract(&request.request).unwrap();
    path.id = ticket.id;

    let response: HttpResponse =
        tickets::show((database.connection.clone().into(), path, auth_user)).into();
    if should_test_succeed {
        assert_eq!(response.status(), StatusCode::OK);
        let body = support::unwrap_body_to_string(&response).unwrap();
        let ticket_response: ShowTicketResponse = serde_json::from_str(&body).unwrap();
        let expected_ticket = DisplayTicket {
            id: ticket.id,
            ticket_type_name: ticket_type.name.clone(),
            status: "Purchased".to_string(),
        };

        let expected_result = ShowTicketResponse {
            ticket: expected_ticket,
            user: Some(user2.into()),
            event: event.for_display(conn).unwrap(),
        };
        assert_eq!(expected_result, ticket_response);
    } else {
        support::expects_unauthorized(&response);
    }
}

pub fn redeem_ticket(role: Roles, should_test_succeed: bool) {
    let database = TestDatabase::new();
    let user = database.create_user().finish();
    let request = TestRequest::create();
    let organization = database.create_organization().finish();
    let auth_user =
        support::create_auth_user_from_user(&user, role, Some(&organization), &database);

    let event = database
        .create_event()
        .with_organization(&organization)
        .with_ticket_pricing()
        .finish();
    let user2 = database.create_user().finish();
    let conn = &database.connection;
    let ticket_type = event.ticket_types(conn).unwrap()[0].id;
    let ticket = database
        .create_purchased_tickets(&user2, ticket_type, 5)
        .remove(0);

    let mut path = Path::<PathParameters>::extract(&request.request).unwrap();
    path.id = ticket.id;
    let mut path2 = Path::<PathParameters>::extract(&request.request).unwrap();
    path2.id = ticket.id;

    //First try when Redeem code is wrong
    let request_data = TicketRedeemRequest {
        redeem_key: "WrongKey".to_string(),
    };

    let response: HttpResponse = tickets::redeem((
        database.connection.clone().into(),
        path,
        Json(request_data),
        auth_user.clone(),
        request.extract_state(),
    )).into();

    #[derive(Deserialize)]

    struct R {
        success: bool,
        //message: Option<String>,
    }

    if should_test_succeed {
        let body = support::unwrap_body_to_string(&response).unwrap();
        let ticket_response: R = serde_json::from_str(&body).unwrap();
        assert_eq!(ticket_response.success, false);
        //Now try with redeem code being correct
        let request_data = TicketRedeemRequest {
            redeem_key: ticket.redeem_key.unwrap(),
        };

        let response: HttpResponse = tickets::redeem((
            database.connection.clone().into(),
            path2,
            Json(request_data),
            auth_user,
            request.extract_state(),
        )).into();
        let body = support::unwrap_body_to_string(&response).unwrap();
        let ticket_response: R = serde_json::from_str(&body).unwrap();
        assert_eq!(ticket_response.success, true);
    } else {
        support::expects_unauthorized(&response);
    }
}

pub fn show_redeemable_ticket(role: Roles, should_test_succeed: bool) {
    let database = TestDatabase::new();
    let user = database.create_user().finish();
    let request = TestRequest::create();
    let organization = database.create_organization().finish();
    let auth_user =
        support::create_auth_user_from_user(&user, role, Some(&organization), &database);
    let venue = database.create_venue().finish();
    let event = database
        .create_event()
        .with_organization(&organization)
        .with_ticket_pricing()
        .with_venue(&venue)
        .finish();
    let user2 = database.create_user().finish();
    let conn = &database.connection;
    let mut cart = Order::find_or_create_cart(&user2, conn).unwrap();
    let ticket_type = &event.ticket_types(conn).unwrap()[0];
    cart.update_quantities(
        &[UpdateOrderItem {
            ticket_type_id: ticket_type.id,
            quantity: 1,
            redemption_code: None,
        }],
        conn,
    ).unwrap();
    let total = cart.calculate_total(conn).unwrap();
    cart.add_external_payment("test".to_string(), user2.id, total, conn)
        .unwrap();

    let ticket = TicketInstance::find_for_user(user2.id, conn)
        .unwrap()
        .remove(0);

    let mut path = Path::<PathParameters>::extract(&request.request).unwrap();
    path.id = ticket.id;

    let response: HttpResponse = tickets::show_redeemable_ticket((
        database.connection.clone().into(),
        path,
        auth_user.clone(),
    )).into();

    if should_test_succeed {
        let body = support::unwrap_body_to_string(&response).unwrap();
        let ticket_response: RedeemableTicket = serde_json::from_str(&body).unwrap();
        assert!(ticket_response.redeem_key.is_some());
    } else {
        support::expects_unauthorized(&response);
    }
}
