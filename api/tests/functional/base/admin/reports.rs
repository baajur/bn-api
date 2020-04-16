use crate::support;
use crate::support::database::TestDatabase;
use crate::support::test_request::TestRequest;
use actix_web::{http::StatusCode, web::Query, FromRequest};
use api::controllers::admin::reports::{self, *};
use chrono::prelude::*;
use chrono::Duration;
use db::models::*;
use std::collections::HashMap;

pub async fn sales_summary_report(role: Roles, should_succeed: bool) {
    let database = TestDatabase::new();
    let connection = database.connection.get();
    let organization = database
        .create_organization()
        .with_event_fee()
        .with_fees()
        .with_cc_fee(1.1)
        .finish();
    let event = database
        .create_event()
        .with_organization(&organization)
        .with_name("Event1".to_string())
        .with_tickets()
        .with_ticket_pricing()
        .with_event_start(Utc::now().naive_utc() + Duration::days(20))
        .finish();
    let ticket_type = event.ticket_types(true, None, connection).unwrap().remove(0);
    let fee_schedule = FeeSchedule::find(organization.fee_schedule_id, connection).unwrap();
    let ticket_pricing = ticket_type.current_ticket_pricing(false, connection).unwrap();
    let fee_schedule_range = fee_schedule
        .get_range(ticket_pricing.price_in_cents, connection)
        .unwrap();
    database.create_order().quantity(1).for_event(&event).is_paid().finish();
    let auth_db_user = database.create_user().finish();
    let auth_user = support::create_auth_user_from_user(&auth_db_user, role, Some(&organization), &database);

    let test_request = TestRequest::create_with_uri(&format!("/reports?name=sales_summary&event_id={}", event.id));
    let query = Query::<ReportQueryParameters>::extract(&test_request.request)
        .await
        .unwrap();
    let response =
        reports::sales_summary_report((database.connection.clone().into(), query, organization.id, auth_user));
    let wrapped_payload = Payload {
        data: vec![
            SalesSummaryReportRow {
                total: 2,
                event_name: event.name.clone(),
                event_date: event.event_start,
                ticket_name: ticket_type.name.clone(),
                face_value_in_cents: ticket_pricing.price_in_cents,
                online_sale_count: 1,
                total_online_client_fees_in_cents: fee_schedule_range.client_fee_in_cents,
                box_office_sale_count: 0,
                comp_sale_count: 0,
            },
            SalesSummaryReportRow {
                total: 2,
                event_name: event.name.clone(),
                event_date: event.event_start,
                ticket_name: "Per Order Fee".to_string(),
                face_value_in_cents: 0,
                online_sale_count: 0,
                total_online_client_fees_in_cents: organization.client_event_fee_in_cents,
                box_office_sale_count: 0,
                comp_sale_count: 0,
            },
        ],
        paging: Paging {
            page: 0,
            limit: 100,
            sort: "".to_string(),
            dir: SortingDir::Asc,
            total: 2 as u64,
            tags: HashMap::new(),
        },
    };

    if should_succeed {
        let response = response.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(wrapped_payload, *response.payload());
    } else {
        assert_eq!(
            response.err().unwrap().to_string(),
            "User does not have the required permissions"
        );
    }
}
