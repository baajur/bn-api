use std::collections::HashMap;

use chrono::{Duration, NaiveDateTime, Utc};
use diesel;
use diesel::prelude::*;
use uuid::Uuid;
use validator::Validate;

use bigneon_db::dev::TestProject;
use bigneon_db::prelude::*;
use bigneon_db::schema::{orders, user_genres};
use bigneon_db::utils::dates;
use bigneon_db::utils::errors;
use bigneon_db::utils::errors::ErrorCode;
use bigneon_db::utils::errors::ErrorCode::ValidationError;

#[test]
fn country() {
    let project = TestProject::new();
    let connection = project.get_connection();
    let country = "ZA".to_string();
    let venue = project
        .create_venue()
        .with_country(country.clone())
        .finish();
    let event = project
        .create_event()
        .with_venue(&venue)
        .with_tickets()
        .with_ticket_pricing()
        .finish();

    // No link to a country
    let user = project.create_user().finish();
    assert!(user.country(connection).unwrap().is_none());

    // With event interest
    project
        .create_event_interest()
        .with_event(&event)
        .with_user(&user)
        .finish();
    assert_eq!(user.country(connection).unwrap(), Some(country.clone()));

    // With order
    let user = project.create_user().finish();
    let order = project
        .create_order()
        .for_user(&user)
        .for_event(&event)
        .quantity(2)
        .is_paid()
        .finish();
    assert_eq!(user.country(connection).unwrap(), Some(country.clone()));

    // With transfer
    let user2 = project.create_user().finish();
    let ticket_type = &event.ticket_types(true, None, connection).unwrap()[0];
    let ticket = &order.tickets(ticket_type.id, connection).unwrap()[0];
    TicketInstance::direct_transfer(
        user.id,
        &vec![ticket.id],
        "nowhere",
        TransferMessageType::Email,
        user2.id,
        connection,
    )
    .unwrap();
    assert_eq!(user2.country(connection).unwrap(), Some(country.clone()));
}

#[test]
fn timezone() {
    let project = TestProject::new();
    let connection = project.get_connection();
    let timezone = "Africa/Johannesburg".to_string();
    let venue = project
        .create_venue()
        .with_timezone(timezone.clone())
        .finish();
    let event = project
        .create_event()
        .with_venue(&venue)
        .with_tickets()
        .with_ticket_pricing()
        .finish();

    // No link to a timezone
    let user = project.create_user().finish();
    assert!(user.timezone(connection).unwrap().is_none());

    // With event interest
    project
        .create_event_interest()
        .with_event(&event)
        .with_user(&user)
        .finish();
    assert_eq!(user.timezone(connection).unwrap(), Some(timezone.clone()));

    // With order
    let user = project.create_user().finish();
    let order = project
        .create_order()
        .for_user(&user)
        .for_event(&event)
        .quantity(2)
        .is_paid()
        .finish();
    assert_eq!(user.timezone(connection).unwrap(), Some(timezone.clone()));

    // With transfer
    let user2 = project.create_user().finish();
    let ticket_type = &event.ticket_types(true, None, connection).unwrap()[0];
    let ticket = &order.tickets(ticket_type.id, connection).unwrap()[0];
    TicketInstance::direct_transfer(
        user.id,
        &vec![ticket.id],
        "nowhere",
        TransferMessageType::Email,
        user2.id,
        connection,
    )
    .unwrap();
    assert_eq!(user2.timezone(connection).unwrap(), Some(timezone.clone()));
}

#[test]
fn last_associated_venue() {
    let project = TestProject::new();
    let connection = project.get_connection();
    let timezone = "Africa/Johannesburg".to_string();
    let venue = project
        .create_venue()
        .with_timezone(timezone.clone())
        .finish();
    let event = project
        .create_event()
        .with_venue(&venue)
        .with_tickets()
        .with_ticket_pricing()
        .finish();

    // No link to an event
    let user = project.create_user().finish();
    assert!(user.last_associated_venue(connection).unwrap().is_none());

    // With event interest
    project
        .create_event_interest()
        .with_event(&event)
        .with_user(&user)
        .finish();
    assert_eq!(
        user.last_associated_venue(connection).unwrap(),
        Some(venue.clone())
    );

    // With order
    let user = project.create_user().finish();
    let order = project
        .create_order()
        .for_user(&user)
        .for_event(&event)
        .quantity(2)
        .is_paid()
        .finish();
    assert_eq!(
        user.last_associated_venue(connection).unwrap(),
        Some(venue.clone())
    );

    // With transfer
    let user2 = project.create_user().finish();
    let ticket_type = &event.ticket_types(true, None, connection).unwrap()[0];
    let ticket = &order.tickets(ticket_type.id, connection).unwrap()[0];
    TicketInstance::direct_transfer(
        user.id,
        &vec![ticket.id],
        "nowhere",
        TransferMessageType::Email,
        user2.id,
        connection,
    )
    .unwrap();
    assert_eq!(
        user2.last_associated_venue(connection).unwrap(),
        Some(venue.clone())
    );
}

#[test]
fn update_genre_info() {
    let project = TestProject::new();
    let connection = project.get_connection();
    let organization = project.create_organization().with_fees().finish();
    let artist = project.create_artist().finish();
    let event = project
        .create_event()
        .with_organization(&organization)
        .with_tickets()
        .with_ticket_pricing()
        .finish();
    project
        .create_event_artist()
        .with_event(&event)
        .with_artist(&artist)
        .finish();
    artist
        .set_genres(
            &vec!["emo".to_string(), "hard-rock".to_string()],
            None,
            connection,
        )
        .unwrap();
    event.update_genres(None, connection).unwrap();
    let user = project.create_user().finish();

    project
        .create_order()
        .for_event(&event)
        .for_user(&user)
        .is_paid()
        .quantity(1)
        .finish();
    // Clearing all genres
    diesel::delete(user_genres::table.filter(user_genres::user_id.eq(user.id)))
        .execute(connection)
        .unwrap();

    assert!(user.genres(connection).unwrap().is_empty());

    assert!(user.update_genre_info(connection).is_ok());
    assert_eq!(
        event.genres(connection).unwrap(),
        vec!["emo".to_string(), "hard-rock".to_string()]
    );
    assert_eq!(
        user.genres(connection).unwrap(),
        vec!["emo".to_string(), "hard-rock".to_string()]
    );
}

#[test]
fn activity() {
    let project = TestProject::new();
    let connection = project.get_connection();
    let user = project.create_user().finish();
    let user2 = project.create_user().finish();
    let user3 = project.create_user().finish();
    let organization = project
        .create_organization()
        .with_event_fee()
        .with_fees()
        .finish();
    let organization2 = project
        .create_organization()
        .with_event_fee()
        .with_fees()
        .finish();
    let event = project
        .create_event()
        .with_organization(&organization)
        .with_ticket_type_count(1)
        .with_tickets()
        .with_ticket_pricing()
        .finish();
    let event2 = project
        .create_event()
        .with_organization(&organization2)
        .with_ticket_pricing()
        .finish();
    let hold = project
        .create_hold()
        .with_hold_type(HoldTypes::Discount)
        .with_quantity(10)
        .with_ticket_type_id(event.ticket_types(true, None, connection).unwrap()[0].id)
        .finish();
    let code = project
        .create_code()
        .with_event(&event2)
        .with_code_type(CodeTypes::Discount)
        .for_ticket_type(&event2.ticket_types(true, None, connection).unwrap()[0])
        .with_discount_in_cents(Some(10))
        .finish();
    project
        .create_order()
        .for_event(&event)
        .on_behalf_of_user(&user)
        .for_user(&user3)
        .quantity(2)
        .with_redemption_code(hold.redemption_code.clone().unwrap())
        .is_paid()
        .finish();
    project
        .create_order()
        .for_event(&event2)
        .for_user(&user)
        .quantity(3)
        .is_paid()
        .finish();
    project
        .create_order()
        .for_event(&event2)
        .for_user(&user)
        .quantity(3)
        .with_redemption_code(code.redemption_code.clone())
        .is_paid()
        .finish();

    assert_eq!(
        vec![ActivitySummary {
            activity_items: ActivityItem::load_for_event(event.id, user.id, None, connection)
                .unwrap(),
            event: event.for_display(connection).unwrap(),
        }],
        user.activity(
            &organization,
            0,
            100,
            SortingDir::Asc,
            PastOrUpcoming::Upcoming,
            None,
            connection
        )
        .unwrap()
        .data
    );
    assert_eq!(
        vec![ActivitySummary {
            activity_items: ActivityItem::load_for_event(event2.id, user.id, None, connection)
                .unwrap(),
            event: event2.for_display(connection).unwrap(),
        }],
        user.activity(
            &organization2,
            0,
            100,
            SortingDir::Asc,
            PastOrUpcoming::Upcoming,
            None,
            connection
        )
        .unwrap()
        .data
    );

    assert!(user2
        .activity(
            &organization,
            0,
            100,
            SortingDir::Asc,
            PastOrUpcoming::Upcoming,
            None,
            connection
        )
        .unwrap()
        .data
        .is_empty());
    assert!(user2
        .activity(
            &organization2,
            0,
            100,
            SortingDir::Asc,
            PastOrUpcoming::Upcoming,
            None,
            connection
        )
        .unwrap()
        .data
        .is_empty());

    assert_eq!(
        vec![ActivitySummary {
            activity_items: ActivityItem::load_for_event(event.id, user3.id, None, connection)
                .unwrap(),
            event: event.for_display(connection).unwrap(),
        }],
        user3
            .activity(
                &organization,
                0,
                100,
                SortingDir::Asc,
                PastOrUpcoming::Upcoming,
                None,
                connection
            )
            .unwrap()
            .data
    );
    assert!(user3
        .activity(
            &organization2,
            0,
            100,
            SortingDir::Asc,
            PastOrUpcoming::Upcoming,
            None,
            connection
        )
        .unwrap()
        .data
        .is_empty());

    // Event is now in the past
    let event = event
        .update(
            None,
            EventEditableAttributes {
                event_start: Some(dates::now().add_days(-2).finish()),
                event_end: Some(dates::now().add_days(-1).finish()),
                ..Default::default()
            },
            connection,
        )
        .unwrap();

    // Is not found via upcoming filter
    assert!(user3
        .activity(
            &organization,
            0,
            100,
            SortingDir::Asc,
            PastOrUpcoming::Upcoming,
            None,
            connection
        )
        .unwrap()
        .data
        .is_empty());

    // Is found via past filter
    assert_eq!(
        vec![ActivitySummary {
            activity_items: ActivityItem::load_for_event(event.id, user3.id, None, connection)
                .unwrap(),
            event: event.for_display(connection).unwrap(),
        }],
        user3
            .activity(
                &organization,
                0,
                100,
                SortingDir::Asc,
                PastOrUpcoming::Past,
                None,
                connection
            )
            .unwrap()
            .data
    );
}

#[test]
fn find_by_ids() {
    let project = TestProject::new();
    let connection = project.get_connection();
    let user = project.create_user().finish();
    let user2 = project.create_user().finish();
    let mut user_ids = vec![user.id, user2.id];
    user_ids.sort();

    let found_users = User::find_by_ids(&user_ids, connection).unwrap();
    let mut found_user_ids: Vec<Uuid> = found_users.into_iter().map(|u| u.id).collect();
    found_user_ids.sort();
    assert_eq!(found_user_ids, user_ids);
}

#[test]
fn genres() {
    let project = TestProject::new();
    let connection = project.get_connection();
    let organization = project.create_organization().with_fees().finish();
    let artist = project.create_artist().finish();
    let event = project
        .create_event()
        .with_organization(&organization)
        .with_tickets()
        .with_ticket_pricing()
        .finish();
    project
        .create_event_artist()
        .with_event(&event)
        .with_artist(&artist)
        .finish();
    artist
        .set_genres(
            &vec!["emo".to_string(), "hard-rock".to_string()],
            None,
            connection,
        )
        .unwrap();
    event.update_genres(None, connection).unwrap();

    // No genres as no purchases yet
    let user = project.create_user().finish();
    assert!(user.genres(connection).unwrap().is_empty());

    project
        .create_order()
        .for_event(&event)
        .for_user(&user)
        .is_paid()
        .quantity(1)
        .finish();

    assert_eq!(
        event.genres(connection).unwrap(),
        vec!["emo".to_string(), "hard-rock".to_string()]
    );
    assert_eq!(
        user.genres(connection).unwrap(),
        vec!["emo".to_string(), "hard-rock".to_string()]
    );
}

#[test]
fn commit() {
    let project = TestProject::new();
    let first_name = Some("Jeff".to_string());
    let last_name = Some("Wilco".to_string());
    let email = Some("jeff@tari.com".to_string());
    let phone_number = Some("555-555-5555".to_string());
    let password = "examplePassword";
    let user = User::create(
        first_name.clone(),
        last_name.clone(),
        email.clone(),
        phone_number.clone(),
        password,
    )
    .commit(None, project.get_connection())
    .unwrap();

    assert_eq!(user.first_name, first_name);
    assert_eq!(user.last_name, last_name);
    assert_eq!(user.email, email);
    assert_eq!(user.phone, phone_number);
    assert_ne!(user.hashed_pw, password);
    assert_eq!(user.hashed_pw.is_empty(), false);
    assert_eq!(user.id.to_string().is_empty(), false);

    let wallets = user.wallets(project.get_connection()).unwrap();
    assert_eq!(wallets.len(), 1);
}

#[test]
fn commit_duplicate_email() {
    let project = TestProject::new();
    let user1 = project.create_user().finish();
    let first_name = Some("Jeff".to_string());
    let last_name = Some("Wilco".to_string());
    let email = user1.email;
    let phone_number = Some("555-555-5555".to_string());
    let password = "examplePassword";
    let result = User::create(first_name, last_name, email, phone_number, password)
        .commit(None, project.get_connection());

    assert_eq!(result.is_err(), true);
    assert_eq!(
        result.err().unwrap().code,
        errors::get_error_message(&ErrorCode::DuplicateKeyError).0
    );
}

#[test]
fn find_external_login() {
    let project = TestProject::new();
    let connection = project.get_connection();
    let user = project.create_user().finish();

    // No external login for facebook, returns None
    assert_eq!(
        None,
        user.find_external_login(FACEBOOK_SITE, connection)
            .optional()
            .unwrap()
    );

    // With external login present
    let external_login = user
        .add_external_login(
            None,
            "abc".to_string(),
            FACEBOOK_SITE.to_string(),
            "123".to_string(),
            vec!["email".to_string()],
            connection,
        )
        .unwrap();
    assert_eq!(
        Some(external_login),
        user.find_external_login(FACEBOOK_SITE, connection)
            .optional()
            .unwrap()
    );
}

#[test]
fn get_profile_for_organization() {
    let project = TestProject::new();
    let connection = project.get_connection();
    let user = project.create_user().finish();
    let user2 = project.create_user().finish();
    let user3 = project.create_user().finish();
    let organization = project.create_organization().with_fees().finish();

    let event = project
        .create_event()
        .with_event_start(NaiveDateTime::from(
            Utc::now().naive_utc() + Duration::days(1),
        ))
        .with_organization(&organization)
        .with_tickets()
        .with_ticket_pricing()
        .finish();
    let event2 = project
        .create_event()
        .with_event_start(NaiveDateTime::from(
            Utc::now().naive_utc() + Duration::days(2),
        ))
        .with_organization(&organization)
        .with_tickets()
        .with_ticket_pricing()
        .finish();
    let event3 = project
        .create_event()
        .with_event_start(NaiveDateTime::from(
            Utc::now().naive_utc() + Duration::days(3),
        ))
        .with_organization(&organization)
        .with_tickets()
        .with_ticket_pricing()
        .finish();
    let ticket_type = &event.ticket_types(true, None, connection).unwrap()[0];
    let ticket_type2 = &event2.ticket_types(true, None, connection).unwrap()[0];
    let ticket_type3 = &event3.ticket_types(true, None, connection).unwrap()[0];

    // No purchases / no organization link
    assert_eq!(
        user.get_profile_for_organization(&organization, connection),
        Err(DatabaseError {
            code: 2000,
            message: "No results".into(),
            cause: Some("Could not load profile for organization fan, NotFound".into()),
            error_code: ErrorCode::NoResults,
        })
    );

    // Add event interest giving access without orders
    project
        .create_event_interest()
        .with_event(&event)
        .with_user(&user)
        .finish();

    assert_eq!(
        user.get_profile_for_organization(&organization, connection)
            .unwrap(),
        FanProfile {
            first_name: user.first_name.clone(),
            last_name: user.last_name.clone(),
            email: user.email.clone(),
            facebook_linked: false,
            event_count: 1,
            revenue_in_cents: 0,
            ticket_sales: 0,
            tickets_owned: 0,
            profile_pic_url: user.profile_pic_url.clone(),
            thumb_profile_pic_url: user.thumb_profile_pic_url.clone(),
            cover_photo_url: user.cover_photo_url.clone(),
            created_at: user.created_at,
            attendance_information: Vec::new(),
        }
    );

    // Add facebook login
    user.add_external_login(
        None,
        "abc".to_string(),
        FACEBOOK_SITE.to_string(),
        "123".to_string(),
        vec!["email".to_string()],
        connection,
    )
    .unwrap();
    assert_eq!(
        user.get_profile_for_organization(&organization, connection)
            .unwrap(),
        FanProfile {
            first_name: user.first_name.clone(),
            last_name: user.last_name.clone(),
            email: user.email.clone(),
            facebook_linked: true,
            event_count: 1,
            revenue_in_cents: 0,
            ticket_sales: 0,
            tickets_owned: 0,
            profile_pic_url: user.profile_pic_url.clone(),
            thumb_profile_pic_url: user.thumb_profile_pic_url.clone(),
            cover_photo_url: user.cover_photo_url.clone(),
            created_at: user.created_at,
            attendance_information: Vec::new(),
        }
    );

    // Add order but do not checkout
    let mut cart = Order::find_or_create_cart(&user, connection).unwrap();
    cart.update_quantities(
        user.id,
        &[UpdateOrderItem {
            ticket_type_id: ticket_type.id,
            quantity: 10,
            redemption_code: None,
        }],
        false,
        false,
        connection,
    )
    .unwrap();
    assert_eq!(
        user.get_profile_for_organization(&organization, connection)
            .unwrap(),
        FanProfile {
            first_name: user.first_name.clone(),
            last_name: user.last_name.clone(),
            email: user.email.clone(),
            facebook_linked: true,
            event_count: 1,
            revenue_in_cents: 0,
            ticket_sales: 0,
            tickets_owned: 0,
            profile_pic_url: user.profile_pic_url.clone(),
            thumb_profile_pic_url: user.thumb_profile_pic_url.clone(),
            cover_photo_url: user.cover_photo_url.clone(),
            created_at: user.created_at,
            attendance_information: Vec::new(),
        }
    );

    // Checkout which changes sales data
    assert_eq!(cart.calculate_total(connection).unwrap(), 1700);
    cart.add_external_payment(
        Some("test".to_string()),
        ExternalPaymentType::CreditCard,
        user.id,
        1700,
        connection,
    )
    .unwrap();
    assert_eq!(cart.status, OrderStatus::Paid);
    assert_eq!(
        user.get_profile_for_organization(&organization, connection)
            .unwrap(),
        FanProfile {
            first_name: user.first_name.clone(),
            last_name: user.last_name.clone(),
            email: user.email.clone(),
            facebook_linked: true,
            event_count: 1,
            revenue_in_cents: 1700,
            ticket_sales: 10,
            tickets_owned: 10,
            profile_pic_url: user.profile_pic_url.clone(),
            thumb_profile_pic_url: user.thumb_profile_pic_url.clone(),
            cover_photo_url: user.cover_photo_url.clone(),
            created_at: user.created_at,
            attendance_information: Vec::new(),
        }
    );

    // Redeem tickets from order
    let items = cart.items(&connection).unwrap();
    let order_item = items
        .iter()
        .find(|i| i.ticket_type_id == Some(ticket_type.id))
        .unwrap();
    let tickets = TicketInstance::find_for_order_item(order_item.id, connection).unwrap();
    let ticket = &tickets[0];
    let ticket2 = &tickets[1];
    TicketInstance::redeem_ticket(
        ticket.id,
        ticket.redeem_key.clone().unwrap(),
        user.id,
        connection,
    )
    .unwrap();
    TicketInstance::redeem_ticket(
        ticket2.id,
        ticket2.redeem_key.clone().unwrap(),
        user.id,
        connection,
    )
    .unwrap();
    assert_eq!(
        user.get_profile_for_organization(&organization, connection)
            .unwrap(),
        FanProfile {
            first_name: user.first_name.clone(),
            last_name: user.last_name.clone(),
            email: user.email.clone(),
            facebook_linked: true,
            event_count: 1,
            revenue_in_cents: 1700,
            ticket_sales: 10,
            tickets_owned: 10,
            profile_pic_url: user.profile_pic_url.clone(),
            thumb_profile_pic_url: user.thumb_profile_pic_url.clone(),
            cover_photo_url: user.cover_photo_url.clone(),
            created_at: user.created_at,
            attendance_information: vec![AttendanceInformation {
                event_name: event.name.clone(),
                event_id: event.id,
                event_start: event.event_start
            }],
        }
    );

    // Checkout with a second order same event
    let order = project
        .create_order()
        .for_user(&user)
        .for_event(&event)
        .quantity(1)
        .is_paid()
        .finish();

    // Redeem a ticket from new order
    let items = order.items(&connection).unwrap();
    let order_item = items
        .iter()
        .find(|i| i.ticket_type_id == Some(ticket_type.id))
        .unwrap();
    let tickets = TicketInstance::find_for_order_item(order_item.id, connection).unwrap();
    let ticket = &tickets[0];
    TicketInstance::redeem_ticket(
        ticket.id,
        ticket.redeem_key.clone().unwrap(),
        user.id,
        connection,
    )
    .unwrap();

    assert_eq!(
        user.get_profile_for_organization(&organization, connection)
            .unwrap(),
        FanProfile {
            first_name: user.first_name.clone(),
            last_name: user.last_name.clone(),
            email: user.email.clone(),
            facebook_linked: true,
            event_count: 1,
            revenue_in_cents: 1870,
            ticket_sales: 11,
            tickets_owned: 11,
            profile_pic_url: user.profile_pic_url.clone(),
            thumb_profile_pic_url: user.thumb_profile_pic_url.clone(),
            cover_photo_url: user.cover_photo_url.clone(),
            created_at: user.created_at,
            attendance_information: vec![AttendanceInformation {
                event_name: event.name.clone(),
                event_id: event.id,
                event_start: event.event_start
            }],
        }
    );

    // Checkout with new event increasing event count as well
    let order = project
        .create_order()
        .for_user(&user)
        .for_event(&event2)
        .quantity(1)
        .is_paid()
        .finish();
    assert_eq!(
        user.get_profile_for_organization(&organization, connection)
            .unwrap(),
        FanProfile {
            first_name: user.first_name.clone(),
            last_name: user.last_name.clone(),
            email: user.email.clone(),
            facebook_linked: true,
            event_count: 2,
            revenue_in_cents: 2040,
            ticket_sales: 12,
            tickets_owned: 12,
            profile_pic_url: user.profile_pic_url.clone(),
            thumb_profile_pic_url: user.thumb_profile_pic_url.clone(),
            cover_photo_url: user.cover_photo_url.clone(),
            created_at: user.created_at,
            attendance_information: vec![AttendanceInformation {
                event_name: event.name.clone(),
                event_id: event.id,
                event_start: event.event_start
            }],
        }
    );

    // Redeem ticket from new event
    let items = order.items(&connection).unwrap();
    let order_item = items
        .iter()
        .find(|i| i.ticket_type_id == Some(ticket_type2.id))
        .unwrap();
    let tickets = TicketInstance::find_for_order_item(order_item.id, connection).unwrap();
    let ticket = &tickets[0];

    // Transfer ticket to different user removing it from attendance information and moving it to theirs
    TicketInstance::direct_transfer(
        user.id,
        &vec![ticket.id],
        "example@tari.com",
        TransferMessageType::Email,
        user2.id,
        connection,
    )
    .unwrap();
    TicketInstance::redeem_ticket(
        ticket.id,
        ticket.redeem_key.clone().unwrap(),
        user.id,
        connection,
    )
    .unwrap();
    assert_eq!(
        user.get_profile_for_organization(&organization, connection)
            .unwrap(),
        FanProfile {
            first_name: user.first_name.clone(),
            last_name: user.last_name.clone(),
            email: user.email.clone(),
            facebook_linked: true,
            event_count: 2,
            revenue_in_cents: 2040,
            ticket_sales: 12,
            tickets_owned: 11,
            profile_pic_url: user.profile_pic_url.clone(),
            thumb_profile_pic_url: user.thumb_profile_pic_url.clone(),
            cover_photo_url: user.cover_photo_url.clone(),
            created_at: user.created_at,
            attendance_information: vec![AttendanceInformation {
                event_name: event.name.clone(),
                event_id: event.id,
                event_start: event.event_start
            }],
        }
    );
    assert_eq!(
        user2
            .get_profile_for_organization(&organization, connection)
            .unwrap(),
        FanProfile {
            first_name: user2.first_name.clone(),
            last_name: user2.last_name.clone(),
            email: user2.email.clone(),
            facebook_linked: false,
            event_count: 1,
            revenue_in_cents: 0,
            ticket_sales: 0,
            tickets_owned: 1,
            profile_pic_url: user2.profile_pic_url.clone(),
            thumb_profile_pic_url: user2.thumb_profile_pic_url.clone(),
            cover_photo_url: user2.cover_photo_url.clone(),
            created_at: user2.created_at,
            attendance_information: vec![AttendanceInformation {
                event_name: event2.name.clone(),
                event_id: event2.id,
                event_start: event2.event_start
            }],
        }
    );

    // Purchase and redeem from other event without transferring
    let order = project
        .create_order()
        .for_user(&user)
        .for_event(&event2)
        .quantity(1)
        .is_paid()
        .finish();
    let items = order.items(&connection).unwrap();
    let order_item = items
        .iter()
        .find(|i| i.ticket_type_id == Some(ticket_type2.id))
        .unwrap();
    let tickets = TicketInstance::find_for_order_item(order_item.id, connection).unwrap();
    let ticket = &tickets[0];
    TicketInstance::redeem_ticket(
        ticket.id,
        ticket.redeem_key.clone().unwrap(),
        user.id,
        connection,
    )
    .unwrap();

    assert_eq!(
        user.get_profile_for_organization(&organization, connection)
            .unwrap(),
        FanProfile {
            first_name: user.first_name.clone(),
            last_name: user.last_name.clone(),
            email: user.email.clone(),
            facebook_linked: true,
            event_count: 2,
            revenue_in_cents: 2210,
            ticket_sales: 13,
            tickets_owned: 12,
            profile_pic_url: user.profile_pic_url.clone(),
            thumb_profile_pic_url: user.thumb_profile_pic_url.clone(),
            cover_photo_url: user.cover_photo_url.clone(),
            created_at: user.created_at,
            attendance_information: vec![
                AttendanceInformation {
                    event_name: event.name.clone(),
                    event_id: event.id,
                    event_start: event.event_start
                },
                AttendanceInformation {
                    event_name: event2.name.clone(),
                    event_id: event2.id,
                    event_start: event2.event_start
                }
            ],
        }
    );

    // Purchased by other user and transferred
    let order = project
        .create_order()
        .for_user(&user2)
        .for_event(&event3)
        .quantity(1)
        .is_paid()
        .finish();

    // Redeem ticket from new event
    let items = order.items(&connection).unwrap();
    let order_item = items
        .iter()
        .find(|i| i.ticket_type_id == Some(ticket_type3.id))
        .unwrap();
    let tickets = TicketInstance::find_for_order_item(order_item.id, connection).unwrap();
    let ticket = &tickets[0];
    TicketInstance::direct_transfer(
        user2.id,
        &vec![ticket.id],
        "example@tari.com",
        TransferMessageType::Email,
        user.id,
        connection,
    )
    .unwrap();

    TicketInstance::redeem_ticket(
        ticket.id,
        ticket.redeem_key.clone().unwrap(),
        user.id,
        connection,
    )
    .unwrap();
    assert_eq!(
        user.get_profile_for_organization(&organization, connection)
            .unwrap(),
        FanProfile {
            first_name: user.first_name.clone(),
            last_name: user.last_name.clone(),
            email: user.email.clone(),
            facebook_linked: true,
            event_count: 3,
            revenue_in_cents: 2210,
            ticket_sales: 13,
            tickets_owned: 13,
            profile_pic_url: user.profile_pic_url.clone(),
            thumb_profile_pic_url: user.thumb_profile_pic_url.clone(),
            cover_photo_url: user.cover_photo_url.clone(),
            created_at: user.created_at,
            attendance_information: vec![
                AttendanceInformation {
                    event_name: event.name.clone(),
                    event_id: event.id,
                    event_start: event.event_start
                },
                AttendanceInformation {
                    event_name: event2.name.clone(),
                    event_id: event2.id,
                    event_start: event2.event_start
                },
                AttendanceInformation {
                    event_name: event3.name.clone(),
                    event_id: event3.id,
                    event_start: event3.event_start
                }
            ],
        }
    );

    // Box office purchase shows up on new user's profile but not on box office user's
    project
        .create_order()
        .for_user(&user)
        .on_behalf_of_user(&user3)
        .for_event(&event)
        .quantity(1)
        .is_paid()
        .finish();
    assert_eq!(
        user.get_profile_for_organization(&organization, connection)
            .unwrap(),
        FanProfile {
            first_name: user.first_name.clone(),
            last_name: user.last_name.clone(),
            email: user.email.clone(),
            facebook_linked: true,
            event_count: 3,
            revenue_in_cents: 2210,
            ticket_sales: 13,
            tickets_owned: 13,
            profile_pic_url: user.profile_pic_url.clone(),
            thumb_profile_pic_url: user.thumb_profile_pic_url.clone(),
            cover_photo_url: user.cover_photo_url.clone(),
            created_at: user.created_at,
            attendance_information: vec![
                AttendanceInformation {
                    event_name: event.name.clone(),
                    event_id: event.id,
                    event_start: event.event_start
                },
                AttendanceInformation {
                    event_name: event2.name.clone(),
                    event_id: event2.id,
                    event_start: event2.event_start
                },
                AttendanceInformation {
                    event_name: event3.name.clone(),
                    event_id: event3.id,
                    event_start: event3.event_start
                }
            ],
        }
    );
    assert_eq!(
        user3
            .get_profile_for_organization(&organization, connection)
            .unwrap(),
        FanProfile {
            first_name: user3.first_name.clone(),
            last_name: user3.last_name.clone(),
            email: user3.email.clone(),
            facebook_linked: false,
            event_count: 1,
            revenue_in_cents: 150,
            ticket_sales: 1,
            tickets_owned: 1,
            profile_pic_url: user3.profile_pic_url.clone(),
            thumb_profile_pic_url: user3.thumb_profile_pic_url.clone(),
            cover_photo_url: user3.cover_photo_url.clone(),
            created_at: user3.created_at,
            attendance_information: Vec::new(),
        }
    );
}

#[test]
fn get_history_for_organization() {
    let project = TestProject::new();
    let connection = project.get_connection();
    let user = project.create_user().finish();
    let organization = project.create_organization().with_fees().finish();
    let event = project
        .create_event()
        .with_organization(&organization)
        .with_tickets()
        .with_ticket_pricing()
        .finish();
    let ticket_type = &event.ticket_types(true, None, connection).unwrap()[0];

    // No history to date
    assert!(user
        .get_history_for_organization(&organization, 0, 100, SortingDir::Desc, connection)
        .unwrap()
        .is_empty());

    // User adds item to cart but does not checkout so no history
    let mut cart = Order::find_or_create_cart(&user, connection).unwrap();
    cart.update_quantities(
        user.id,
        &[UpdateOrderItem {
            ticket_type_id: ticket_type.id,
            quantity: 10,
            redemption_code: None,
        }],
        false,
        false,
        connection,
    )
    .unwrap();
    assert!(user
        .get_history_for_organization(&organization, 0, 100, SortingDir::Desc, connection)
        .unwrap()
        .is_empty());

    // User checks out so has a paid order so history exists
    assert_eq!(cart.calculate_total(connection).unwrap(), 1700);
    cart.add_external_payment(
        Some("test".to_string()),
        ExternalPaymentType::CreditCard,
        user.id,
        1700,
        connection,
    )
    .unwrap();
    assert_eq!(cart.status, OrderStatus::Paid);

    let mut paging = Paging::new(0, 100);
    paging.dir = SortingDir::Desc;
    let mut payload = Payload::new(
        vec![HistoryItem::Purchase {
            order_id: cart.id,
            order_date: cart.order_date,
            event_name: event.name.clone(),
            ticket_sales: 10,
            revenue_in_cents: 1700,
        }],
        paging,
    );
    payload.paging.total = 1;
    assert_eq!(
        user.get_history_for_organization(&organization, 0, 100, SortingDir::Desc, connection)
            .unwrap(),
        payload
    );

    // User makes a second order
    let mut cart2 = Order::find_or_create_cart(&user, connection).unwrap();
    cart2
        .update_quantities(
            user.id,
            &[UpdateOrderItem {
                ticket_type_id: ticket_type.id,
                quantity: 1,
                redemption_code: None,
            }],
            false,
            false,
            connection,
        )
        .unwrap();

    // Update cart2 to a future date to avoid test timing errors
    let mut cart2 = diesel::update(orders::table.filter(orders::id.eq(cart2.id)))
        .set(orders::order_date.eq(Utc::now().naive_utc() + Duration::seconds(1)))
        .get_result::<Order>(connection)
        .unwrap();

    assert_eq!(cart2.calculate_total(connection).unwrap(), 170);
    cart2
        .add_external_payment(
            Some("test".to_string()),
            ExternalPaymentType::CreditCard,
            user.id,
            170,
            connection,
        )
        .unwrap();
    assert_eq!(cart2.status, OrderStatus::Paid);

    let mut paging = Paging::new(0, 100);
    paging.dir = SortingDir::Desc;
    let mut payload = Payload::new(
        vec![
            HistoryItem::Purchase {
                order_id: cart2.id,
                order_date: cart2.order_date,
                event_name: event.name.clone(),
                ticket_sales: 1,
                revenue_in_cents: 170,
            },
            HistoryItem::Purchase {
                order_id: cart.id,
                order_date: cart.order_date,
                event_name: event.name.clone(),
                ticket_sales: 10,
                revenue_in_cents: 1700,
            },
        ],
        paging,
    );
    payload.paging.total = 2;
    assert_eq!(
        user.get_history_for_organization(&organization, 0, 100, SortingDir::Desc, connection)
            .unwrap(),
        payload
    );
}

#[test]
fn find() {
    let project = TestProject::new();
    let user = project.create_user().finish();

    let found_user = User::find(user.id, project.get_connection()).expect("User was not found");
    assert_eq!(found_user.id, user.id);
    assert_eq!(found_user.email, user.email);

    assert!(
        match User::find(Uuid::new_v4(), project.get_connection()) {
            Ok(_user) => false,
            Err(_e) => true,
        },
        "User incorrectly returned when id invalid"
    );
}

#[test]
fn event_users() {
    let project = TestProject::new();
    let connection = project.get_connection();
    let user = project.create_user().finish();
    let event = project.create_event().finish();

    let event_user = EventUser::create(user.id, event.id, Roles::PromoterReadOnly)
        .commit(connection)
        .unwrap();
    assert_eq!(user.event_users(connection).unwrap(), vec![event_user]);
}

#[test]
fn get_event_ids_by_organization() {
    let project = TestProject::new();
    let connection = project.get_connection();
    let user = project.create_user().finish();

    // No results
    assert_eq!(
        (HashMap::new(), HashMap::new()),
        user.get_event_ids_by_organization(connection).unwrap()
    );

    let organization = project
        .create_organization()
        .with_name("Organization1".into())
        .finish();
    let organization2 = project
        .create_organization()
        .with_name("Organization2".into())
        .finish();
    let organization3 = project
        .create_organization()
        .with_name("Organization3".into())
        .with_member(&user, Roles::OrgAdmin)
        .finish();

    let event = project
        .create_event()
        .with_organization(&organization)
        .finish();
    let event2 = project
        .create_event()
        .with_organization(&organization2)
        .finish();
    let event3 = project
        .create_event()
        .with_organization(&organization2)
        .finish();

    organization
        .add_user(
            user.id,
            vec![Roles::PromoterReadOnly],
            vec![event.id],
            connection,
        )
        .unwrap();
    organization2
        .add_user(
            user.id,
            vec![Roles::Promoter],
            vec![event2.id, event3.id],
            connection,
        )
        .unwrap();

    let (events_by_organization, readonly_events_by_organization) =
        user.get_event_ids_by_organization(connection).unwrap();
    assert!(events_by_organization
        .get(&organization.id)
        .unwrap()
        .is_empty());
    assert!(readonly_events_by_organization
        .get(&organization2.id)
        .unwrap()
        .is_empty());
    let organization_results = readonly_events_by_organization
        .get(&organization.id)
        .unwrap();
    assert_eq!(&vec![event.id], organization_results);
    let mut organization2_results = events_by_organization
        .get(&organization2.id)
        .unwrap()
        .clone();
    organization2_results.sort();
    let mut expected_organization2 = vec![event2.id, event3.id];
    expected_organization2.sort();
    assert_eq!(&expected_organization2, &organization2_results);

    // get_event_ids_for_organization
    assert_eq!(
        vec![event.id],
        user.get_event_ids_for_organization(organization.id, connection)
            .unwrap()
    );
    let mut organization2_results = user
        .get_event_ids_for_organization(organization2.id, connection)
        .unwrap();
    organization2_results.sort();
    assert_eq!(&expected_organization2, &organization2_results);

    assert!(user
        .get_event_ids_for_organization(organization3.id, connection)
        .unwrap()
        .is_empty());
}

#[test]
fn payment_method() {
    let project = TestProject::new();
    let user = project.create_user().finish();
    assert!(user
        .payment_method(PaymentProviders::External, project.get_connection())
        .is_err());

    let payment_method = project
        .create_payment_method()
        .with_name(PaymentProviders::External)
        .with_user(&user)
        .finish();
    assert_eq!(
        payment_method,
        user.payment_method(payment_method.name.clone(), project.get_connection())
            .unwrap(),
    );
}

#[test]
fn default_payment_method() {
    let project = TestProject::new();
    let connection = project.get_connection();
    let user = project.create_user().finish();

    // No payment methods set
    assert!(user.default_payment_method(connection).is_err());

    // Payment method exists but not default
    project
        .create_payment_method()
        .with_name(PaymentProviders::External)
        .with_user(&user)
        .finish();
    assert!(user.default_payment_method(connection).is_err());

    // Default set
    let payment_method2 = project
        .create_payment_method()
        .with_name(PaymentProviders::Stripe)
        .with_user(&user)
        .make_default()
        .finish();
    let default_payment_method = user.default_payment_method(connection).unwrap();
    assert_eq!(payment_method2, default_payment_method);
}

#[test]
fn payment_methods() {
    let project = TestProject::new();
    let connection = project.get_connection();
    let user = project.create_user().finish();
    assert!(user.payment_methods(connection).unwrap().is_empty());

    let payment_method = project
        .create_payment_method()
        .with_name(PaymentProviders::External)
        .with_user(&user)
        .finish();
    assert_eq!(
        vec![payment_method.clone()],
        user.payment_methods(connection).unwrap(),
    );

    let payment_method2 = project
        .create_payment_method()
        .with_name(PaymentProviders::Stripe)
        .with_user(&user)
        .finish();
    assert_eq!(
        vec![payment_method, payment_method2],
        user.payment_methods(connection).unwrap(),
    );
}

#[test]
fn full_name() {
    let project = TestProject::new();

    let first_name = "Bob".to_string();
    let last_name = "Jones".to_string();

    let user = project
        .create_user()
        .with_first_name(&first_name)
        .with_last_name(&last_name)
        .finish();
    assert_eq!(user.full_name(), format!("{} {}", first_name, last_name));
}

#[test]
fn find_by_email() {
    let project = TestProject::new();
    let user = project.create_user().finish();

    let found_user = User::find_by_email(&user.email.clone().unwrap(), project.get_connection())
        .expect("User was not found");
    assert_eq!(found_user, user);

    let not_found = User::find_by_email("not@real.com", project.get_connection());
    let error = not_found.unwrap_err();
    assert_eq!(
        error.to_string(),
        "[2000] No results\nCaused by: Error loading user, NotFound"
    );
}

#[test]
fn update() {
    let project = TestProject::new();
    let connection = project.get_connection();
    let user = project.create_user().finish();
    let mut attributes: UserEditableAttributes = Default::default();
    let email = "new_email@tari.com";
    attributes.email = Some(email.to_string());

    let updated_user = user.update(attributes.into(), None, connection).unwrap();
    assert_eq!(updated_user.email, Some(email.into()));
}

#[test]
fn update_with_validation_errors() {
    let project = TestProject::new();
    let connection = project.get_connection();
    let user = project.create_user().finish();
    let user2 = project.create_user().finish();

    let mut attributes: UserEditableAttributes = Default::default();
    let email = user2.email.clone();
    attributes.email = email;

    let result = user.update(attributes.into(), None, connection);
    match result {
        Ok(_) => {
            panic!("Expected validation error");
        }
        Err(error) => match &error.error_code {
            ValidationError { errors } => {
                assert!(errors.contains_key("email"));
                assert_eq!(errors["email"].len(), 1);
                assert_eq!(errors["email"][0].code, "uniqueness");
                assert_eq!(
                    &errors["email"][0].message.clone().unwrap().into_owned(),
                    "Email is already in use"
                );
            }
            _ => panic!("Expected validation error"),
        },
    }

    // Ignores case
    let mut attributes: UserEditableAttributes = Default::default();
    let email = user2.email.clone().map(|e| e.to_uppercase());
    attributes.email = email;

    let result = user.update(attributes.into(), None, connection);
    match result {
        Ok(_) => {
            panic!("Expected validation error");
        }
        Err(error) => match &error.error_code {
            ValidationError { errors } => {
                assert!(errors.contains_key("email"));
                assert_eq!(errors["email"].len(), 1);
                assert_eq!(errors["email"][0].code, "uniqueness");
                assert_eq!(
                    &errors["email"][0].message.clone().unwrap().into_owned(),
                    "Email is already in use"
                );
            }
            _ => panic!("Expected validation error"),
        },
    }
}

#[test]
fn new_user_validate() {
    let email = "abc";
    let user = User::create(
        Some("First".to_string()),
        Some("Last".to_string()),
        Some(email.to_string()),
        Some("123".to_string()),
        &"Password",
    );
    let result = user.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err().field_errors();

    assert!(errors.contains_key("email"));
    assert_eq!(errors["email"].len(), 1);
    assert_eq!(errors["email"][0].code, "email");
    assert_eq!(
        &errors["email"][0].message.clone().unwrap().into_owned(),
        "Email is invalid"
    );
}

#[test]
fn user_editable_attributes_validate() {
    let mut user_parameters: UserEditableAttributes = Default::default();
    user_parameters.email = Some("abc".into());

    let result = user_parameters.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err().field_errors();

    assert!(errors.contains_key("email"));
    assert_eq!(errors["email"].len(), 1);
    assert_eq!(errors["email"][0].code, "email");
    assert_eq!(
        &errors["email"][0].message.clone().unwrap().into_owned(),
        "Email is invalid"
    );
}

#[test]
fn create_from_external_login() {
    let project = TestProject::new();
    let external_id = "123";
    let first_name = "Dennis";
    let last_name = "Miguel";
    let email = "dennis@tari.com";
    let site = "facebook.com";
    let access_token = "abc-123";

    let user = User::create_from_external_login(
        external_id.to_string(),
        first_name.to_string(),
        last_name.to_string(),
        Some(email.to_string()),
        site.to_string(),
        access_token.to_string(),
        vec!["email".to_string()],
        None,
        project.get_connection(),
    )
    .unwrap();

    let external_login = ExternalLogin::find_user(external_id, site, project.get_connection())
        .unwrap()
        .unwrap();

    assert_eq!(user.id, external_login.user_id);
    assert_eq!(access_token, external_login.access_token);
    assert_eq!(site, external_login.site);
    assert_eq!(external_id, external_login.external_user_id);

    assert_eq!(Some(email.to_string()), user.email);
    assert_eq!(first_name, user.first_name.unwrap_or("".to_string()));
    assert_eq!(last_name, user.last_name.unwrap_or("".to_string()));
}

#[test]
fn for_display() {
    let project = TestProject::new();
    let user = project.create_user().finish();
    let user_id = user.id.clone();
    let display_user = user.for_display().unwrap();

    assert_eq!(display_user.id, user_id);
}

#[test]
fn organizations() {
    let project = TestProject::new();
    let connection = project.get_connection();
    let user = project.create_user().finish();

    let organization = project
        .create_organization()
        .with_name("Organization1".into())
        .with_member(&user, Roles::OrgOwner)
        .finish();
    let organization2 = project
        .create_organization()
        .with_name("Organization2".into())
        .with_member(&user, Roles::OrgMember)
        .finish();
    let _organization3 = project
        .create_organization()
        .with_name("Organization3".into())
        .finish();

    assert_eq!(
        vec![organization, organization2],
        user.organizations(connection).unwrap()
    );
}

#[test]
fn find_events_with_access_to_scan() {
    //create event
    let project = TestProject::new();
    let connection = project.get_connection();
    let venue = project.create_venue().finish();

    let owner = project.create_user().finish();
    let scanner = project.create_user().finish();
    let _normal_user = project.create_user().finish();
    let organization = project
        .create_organization()
        .with_member(&owner, Roles::OrgOwner)
        .with_member(&scanner, Roles::OrgMember)
        .finish();
    let _draft_event = project
        .create_event()
        .with_status(EventStatus::Draft)
        .with_event_start(Utc::now().naive_utc())
        .with_name("DraftEvent".into())
        .with_organization(&organization)
        .with_venue(&venue)
        .finish();
    let published_event = project
        .create_event()
        .with_status(EventStatus::Published)
        .with_event_start(Utc::now().naive_utc())
        .with_name("PublishedEvent".into())
        .with_organization(&organization)
        .with_venue(&venue)
        .finish();
    let _published_external_event = project
        .create_event()
        .with_status(EventStatus::Published)
        .external()
        .with_event_start(Utc::now().naive_utc())
        .with_name("PublishedExternalEvent".into())
        .with_organization(&organization)
        .with_venue(&venue)
        .finish();

    let owner_events = owner.find_events_with_access_to_scan(connection).unwrap();
    let scanner_events = scanner.find_events_with_access_to_scan(connection).unwrap();
    let normal_user_events = _normal_user
        .find_events_with_access_to_scan(connection)
        .unwrap();

    assert_eq!(owner_events, vec![published_event.clone()]);
    assert_eq!(scanner_events, vec![published_event]);
    assert!(normal_user_events.is_empty());
}

#[test]
fn get_roles_by_organization() {
    let project = TestProject::new();
    let connection = project.get_connection();
    let user = project.create_user().finish();

    let organization = project
        .create_organization()
        .with_name("Organization1".into())
        .with_member(&user, Roles::OrgOwner)
        .finish();
    let organization2 = project
        .create_organization()
        .with_name("Organization2".into())
        .with_member(&user, Roles::OrgMember)
        .finish();
    let _organization3 = project
        .create_organization()
        .with_name("Organization3".into())
        .finish();

    let mut expected_results = HashMap::new();
    expected_results.insert(organization.id.clone(), vec![Roles::OrgOwner]);
    expected_results.insert(organization2.id.clone(), vec![Roles::OrgMember]);

    assert_eq!(
        user.get_roles_by_organization(connection).unwrap(),
        expected_results
    );
}

#[test]
fn get_scopes_by_organization() {
    let project = TestProject::new();
    let connection = project.get_connection();
    let user = project.create_user().finish();

    let organization = project
        .create_organization()
        .with_name("Organization1".into())
        .with_member(&user, Roles::OrgOwner)
        .finish();
    let organization2 = project
        .create_organization()
        .with_name("Organization2".into())
        .with_member(&user, Roles::OrgMember)
        .finish();
    let _organization3 = project
        .create_organization()
        .with_name("Organization3".into())
        .finish();

    let mut expected_results = HashMap::new();
    expected_results.insert(
        organization.id,
        vec![
            Scopes::ArtistWrite,
            Scopes::BoxOfficeTicketRead,
            Scopes::BoxOfficeTicketWrite,
            Scopes::CodeRead,
            Scopes::CodeWrite,
            Scopes::CompRead,
            Scopes::CompWrite,
            Scopes::DashboardRead,
            Scopes::EventBroadcast,
            Scopes::EventCancel,
            Scopes::EventDelete,
            Scopes::EventFinancialReports,
            Scopes::EventInterest,
            Scopes::EventReports,
            Scopes::EventScan,
            Scopes::EventViewGuests,
            Scopes::EventWrite,
            Scopes::HoldRead,
            Scopes::HoldWrite,
            Scopes::NoteDelete,
            Scopes::NoteRead,
            Scopes::NoteWrite,
            Scopes::OrderMakeExternalPayment,
            Scopes::OrderRead,
            Scopes::OrderReadOwn,
            Scopes::OrderRefund,
            Scopes::OrderResendConfirmation,
            Scopes::OrgAdminUsers,
            Scopes::OrgFans,
            Scopes::OrgRead,
            Scopes::OrgReadEvents,
            Scopes::OrgReports,
            Scopes::OrgUsers,
            Scopes::OrgWrite,
            Scopes::TransferCancel,
            Scopes::TransferCancelOwn,
            Scopes::TransferRead,
            Scopes::TransferReadOwn,
            Scopes::RedeemTicket,
            Scopes::TicketAdmin,
            Scopes::TicketRead,
            Scopes::TicketWrite,
            Scopes::TicketWriteOwn,
            Scopes::TicketTransfer,
            Scopes::TicketTypeRead,
            Scopes::TicketTypeWrite,
            Scopes::UserRead,
            Scopes::VenueWrite,
        ],
    );
    expected_results.insert(
        organization2.id,
        vec![
            Scopes::ArtistWrite,
            Scopes::BoxOfficeTicketRead,
            Scopes::BoxOfficeTicketWrite,
            Scopes::CodeRead,
            Scopes::CodeWrite,
            Scopes::CompRead,
            Scopes::CompWrite,
            Scopes::DashboardRead,
            Scopes::EventCancel,
            Scopes::EventDelete,
            Scopes::EventInterest,
            Scopes::EventScan,
            Scopes::EventViewGuests,
            Scopes::EventWrite,
            Scopes::HoldRead,
            Scopes::HoldWrite,
            Scopes::NoteRead,
            Scopes::NoteWrite,
            Scopes::OrderRead,
            Scopes::OrderReadOwn,
            Scopes::OrderRefund,
            Scopes::OrderResendConfirmation,
            Scopes::OrgFans,
            Scopes::OrgRead,
            Scopes::OrgReadEvents,
            Scopes::TransferCancel,
            Scopes::TransferCancelOwn,
            Scopes::TransferRead,
            Scopes::TransferReadOwn,
            Scopes::RedeemTicket,
            Scopes::TicketAdmin,
            Scopes::TicketRead,
            Scopes::TicketWriteOwn,
            Scopes::TicketTransfer,
            Scopes::TicketTypeRead,
            Scopes::TicketTypeWrite,
            Scopes::VenueWrite,
        ],
    );

    assert_eq!(
        user.get_scopes_by_organization(connection).unwrap(),
        expected_results
    );
}

#[test]
fn get_global_scopes() {
    let project = TestProject::new();
    let connection = project.get_connection();
    let user = project.create_user().finish();
    let user2 = project.create_user().finish();
    let mut user3 = project.create_user().finish();
    let _organization = project
        .create_organization()
        .with_member(&user, Roles::OrgOwner)
        .with_member(&user2, Roles::OrgMember)
        .finish();
    user3 = user3.add_role(Roles::Admin, connection).unwrap();

    assert_eq!(
        user.get_global_scopes()
            .into_iter()
            .map(|scope| scope.to_string())
            .collect::<Vec<String>>(),
        vec![
            "event:interest",
            "order:read-own",
            "transfer:cancel-own",
            "transfer:read-own",
            "ticket:write-own",
            "ticket:transfer"
        ]
    );
    assert_eq!(
        user2
            .get_global_scopes()
            .into_iter()
            .map(|scope| scope.to_string())
            .collect::<Vec<String>>(),
        vec![
            "event:interest",
            "order:read-own",
            "transfer:cancel-own",
            "transfer:read-own",
            "ticket:write-own",
            "ticket:transfer"
        ]
    );
    assert_equiv!(
        user3
            .get_global_scopes()
            .into_iter()
            .map(|scope| scope.to_string())
            .collect::<Vec<String>>(),
        vec![
            "artist:write",
            "box-office-ticket:read",
            "box-office-ticket:write",
            "code:read",
            "code:write",
            "comp:read",
            "comp:write",
            "dashboard:read",
            "event:broadcast",
            "event:cancel",
            "event:delete",
            "event:financial-reports",
            "event:interest",
            "event:reports",
            "event:scan",
            "event:view-guests",
            "event:write",
            "hold:read",
            "hold:write",
            "note:delete",
            "note:read",
            "note:write",
            "order:make-external-payment",
            "order:read",
            "order:read-own",
            "order:refund",
            "order:resend-confirmation",
            "org:admin",
            "org:admin-users",
            "org:fans",
            "org:financial-reports",
            "org:read",
            "org:read-events",
            "org:reports",
            "org:users",
            "org:write",
            "redeem:ticket",
            "region:write",
            "ticket:admin",
            "ticket:read",
            "ticket:transfer",
            "ticket:write",
            "ticket:write-own",
            "ticket-type:read",
            "ticket-type:write",
            "transfer:cancel-own",
            "transfer:cancel",
            "transfer:read-own",
            "transfer:read",
            "user:read",
            "venue:write"
        ]
    );
}

#[test]
fn add_role() {
    let project = TestProject::new();
    let user = project.create_user().finish();

    user.add_role(Roles::Admin, project.get_connection())
        .unwrap();
    //Try adding a duplicate role to check that it isnt duplicated.
    user.add_role(Roles::Admin, project.get_connection())
        .unwrap();

    let user2 = User::find(user.id, project.get_connection()).unwrap();
    assert_eq!(user2.role, vec![Roles::User, Roles::Admin]);
}
