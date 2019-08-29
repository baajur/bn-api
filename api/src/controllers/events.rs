use actix_web::{http::StatusCode, HttpResponse, Path, Query, State};
use auth::user::User as AuthUser;
use bigneon_db::dev::times;
use bigneon_db::prelude::*;
use chrono::prelude::*;
use chrono::Duration;
use controllers::organizations::DisplayOrganizationUser;
use db::Connection;
use db::ReadonlyConnection;
use diesel::PgConnection;
use domain_events::executors::UpdateGenresPayload;
use errors::*;
use extractors::*;
use helpers::application;
use models::{
    EventShowResult, PathParameters, RedeemTicketPathParameters, ShortOrganization,
    UserDisplayTicketType, WebPayload,
};
use serde_json::Value;
use serde_with::{self, CommaSeparator};
use server::AppState;
use std::collections::HashMap;
use utils::{marketing_contacts, ServiceLocator};
use uuid::Uuid;

#[derive(Deserialize, Clone)]
pub struct SearchParameters {
    #[serde(default, deserialize_with = "deserialize_unless_blank")]
    query: Option<String>,
    region_id: Option<Uuid>,
    organization_id: Option<Uuid>,
    venue_id: Option<Uuid>,
    #[serde(
        default,
        with = "serde_with::rust::StringWithSeparator::<CommaSeparator>"
    )]
    genres: Vec<String>,
    #[serde(
        default,
        with = "serde_with::rust::StringWithSeparator::<CommaSeparator>"
    )]
    status: Vec<EventStatus>,
    start_utc: Option<NaiveDateTime>,
    end_utc: Option<NaiveDateTime>,
    page: Option<u32>,
    limit: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_unless_blank")]
    sort: Option<String>,
    dir: Option<SortingDir>,
    past_or_upcoming: Option<String>,
    updated_at: Option<String>,
    #[serde(default, deserialize_with = "deserialize_unless_blank")]
    category: Option<EventTypes>,
}

#[derive(Serialize)]
struct EventVenueEntry {
    id: Uuid,
    name: String,
    organization_id: Uuid,
    venue_id: Option<Uuid>,
    created_at: NaiveDateTime,
    event_start: Option<NaiveDateTime>,
    door_time: Option<NaiveDateTime>,
    status: EventStatus,
    publish_date: Option<NaiveDateTime>,
    promo_image_url: Option<String>,
    additional_info: Option<String>,
    top_line_info: Option<String>,
    age_limit: Option<String>,
    cancelled_at: Option<NaiveDateTime>,
    venue: Option<Venue>,
    artists: Option<Vec<DisplayEventArtist>>,
    min_ticket_price: Option<i64>,
    max_ticket_price: Option<i64>,
    is_external: bool,
    external_url: Option<String>,
    user_is_interested: bool,
    localized_times: EventLocalizedTimeStrings,
    tracking_keys: TrackingKeys,
    event_type: EventTypes,
    updated_at: NaiveDateTime,
    slug: String,
    url: String,
    event_end: Option<NaiveDateTime>,
}

impl From<SearchParameters> for Paging {
    fn from(s: SearchParameters) -> Paging {
        let mut default_tags: HashMap<String, Value> = HashMap::new();
        if let Some(ref i) = s.query {
            default_tags.insert("query".to_owned(), json!(i.clone()));
        }
        if let Some(ref i) = s.region_id {
            default_tags.insert("region_id".to_owned(), json!(i));
        }
        if let Some(ref i) = s.organization_id {
            default_tags.insert("organization_id".to_owned(), json!(i));
        }
        for event_status in s.status.clone().into_iter() {
            default_tags.insert("status".to_owned(), json!(event_status));
        }

        if let Some(ref i) = s.start_utc {
            default_tags.insert("start_utc".to_owned(), json!(i));
        }
        if let Some(ref i) = s.end_utc {
            default_tags.insert("end_utc".to_owned(), json!(i));
        }

        PagingParameters {
            page: s.page,
            limit: s.limit,
            sort: s.sort,
            dir: s.dir,
            tags: default_tags,
        }
        .into()
    }
}

/**
 * What events does this user have authority to check in
**/
pub fn checkins(
    (conn, query, auth_user, state): (
        Connection,
        Query<SearchParameters>,
        AuthUser,
        State<AppState>,
    ),
) -> Result<HttpResponse, BigNeonError> {
    let events = auth_user.user.find_events_with_access_to_scan(conn.get())?;
    let mut payload = Payload::new(
        event_venues_from_events(events, Some(auth_user.user), &state, conn.get())?,
        query.into_inner().into(),
    );
    payload.paging.total = payload.data.len() as u64;
    payload.paging.limit = 100;
    Ok(HttpResponse::Ok().json(&payload))
}

pub fn index(
    (state, connection, query, auth_user): (
        State<AppState>,
        ReadonlyConnection,
        Query<SearchParameters>,
        OptionalUser,
    ),
) -> Result<HttpResponse, BigNeonError> {
    let connection = connection.get();
    let query = query.into_inner();
    let paging = query.clone().into();
    let user = auth_user
        .into_inner()
        .and_then(|auth_user| Some(auth_user.user));

    let past_or_upcoming = match query
        .past_or_upcoming
        .clone()
        .unwrap_or("upcoming".to_string())
        .as_str()
    {
        "past" => PastOrUpcoming::Past,
        _ => PastOrUpcoming::Upcoming,
    };

    let sort_field = match query
        .sort
        .clone()
        .unwrap_or("event_start".to_string())
        .as_str()
    {
        "event_start" => EventSearchSortField::EventStart,
        "name" => EventSearchSortField::Name,
        _ => EventSearchSortField::EventStart,
    };

    let events_count = Event::search(
        query.query.clone(),
        query.region_id,
        query.organization_id,
        query.venue_id,
        if query.genres.is_empty() {
            None
        } else {
            Some(query.genres.clone())
        },
        query.start_utc,
        query.end_utc,
        if query.status.is_empty() {
            None
        } else {
            Some(query.status.clone())
        },
        sort_field,
        query.dir.clone().unwrap_or(SortingDir::Asc),
        user.clone(),
        past_or_upcoming,
        query.category.clone(),
        &paging,
        state.service_locator.country_lookup_service(),
        connection,
    )?;
    let (events, count) = events_count;

    let mut payload = Payload::new(
        event_venues_from_events(events, user, &state, connection)?,
        query.into(),
    );
    payload.paging.total = count as u64;
    payload.paging.limit = paging.limit;

    Ok(HttpResponse::Ok().json(&payload))
}

#[derive(Deserialize)]
pub struct EventParameters {
    pub box_office_pricing: Option<bool>,
    pub redemption_code: Option<String>,
    pub private_access_code: Option<String>,
}

#[derive(Deserialize)]
pub struct StringPathParameters {
    pub id: String,
}

pub fn show(
    (state, connection, parameters, query, user): (
        State<AppState>,
        ReadonlyConnection,
        Path<StringPathParameters>,
        Query<EventParameters>,
        OptionalUser,
    ),
) -> Result<HttpResponse, BigNeonError> {
    let connection = connection.get();
    let user = user.into_inner();

    let (event, organization, venue, fee_schedule) = match parameters.id.parse() {
        Ok(i) => Event::find_incl_org_venue_fees(i, connection)?,
        Err(_) => Event::find_by_slug(&parameters.id, connection)?,
    };

    if event.private_access_code.is_some()
        && !(query.private_access_code.is_some()
            && event.private_access_code.clone().unwrap()
                == query.private_access_code.clone().unwrap().to_lowercase())
    {
        match user {
            Some(ref user) => user.requires_scope_for_organization(
                Scopes::OrgReadEvents,
                &organization,
                connection,
            )?,
            None => {
                return application::unauthorized_with_message(
                    "Unauthorized access of private event",
                    None,
                    None,
                );
            }
        }
    };

    let is_user_admin = match user {
        Some(ref user) => user.has_scope_for_organization_event(
            Scopes::EventWrite,
            &organization,
            event.id,
            connection,
        )?,
        None => false,
    };

    if (!is_user_admin && event.publish_date.unwrap_or(times::infinity()) > dates::now().finish())
        || event.deleted_at.is_some()
    {
        return application::not_found();
    }

    let localized_times = event.get_all_localized_time_strings(venue.as_ref());
    let event_artists = EventArtist::find_all_from_event(event.id, connection)?;
    let total_interest = EventInterest::total_interest(event.id, connection)?;
    let user_interest = match user {
        Some(ref u) => EventInterest::user_interest(event.id, u.id(), connection)?,
        None => false,
    };

    let box_office_pricing = query.box_office_pricing.unwrap_or(false);
    if box_office_pricing {
        match user {
            Some(ref user) => user.requires_scope_for_organization(
                Scopes::BoxOfficeTicketRead,
                &organization,
                connection,
            )?,
            None => {
                return application::unauthorized_with_message(
                    "Cannot access box office pricing",
                    None,
                    None,
                );
            }
        }
    }

    let ticket_types =
        TicketType::find_by_event_id(event.id, true, query.redemption_code.clone(), connection)?;
    let mut display_ticket_types = Vec::new();
    let mut sales_start_date = Some(times::infinity());
    let mut limited_tickets_remaining: Vec<TicketsRemaining> = Vec::new();
    for ticket_type in ticket_types {
        if ticket_type.status != TicketTypeStatus::Cancelled {
            let display_ticket_type = UserDisplayTicketType::from_ticket_type(
                &ticket_type,
                &fee_schedule,
                box_office_pricing,
                query.redemption_code.clone(),
                connection,
            )?;

            // Only show private ticket types via holds
            if ticket_type.visibility == TicketTypeVisibility::Hidden
                && display_ticket_type.redemption_code.is_none()
            {
                continue;
            }

            if sales_start_date.unwrap()
                > ticket_type.start_date.clone().unwrap_or(times::infinity())
            {
                sales_start_date = ticket_type.start_date.clone();
            }

            // If the ticket type is sold out, hide it if necessary
            if display_ticket_type.status != TicketTypeStatus::Published
                && ticket_type.visibility == TicketTypeVisibility::WhenAvailable
            {
                continue;
            };

            display_ticket_types.push(display_ticket_type);
        }
    }

    if let Some(ref u) = user {
        let tickets_bought = Order::quantity_for_user_for_event(u.id(), event.id, connection)?;
        for (tt_id, num) in tickets_bought {
            let limit = TicketType::find(tt_id, connection)?.limit_per_person;
            if limit > 0 {
                limited_tickets_remaining.push(TicketsRemaining {
                    ticket_type_id: tt_id,
                    tickets_remaining: limit - num,
                });
            }
        }
    }

    let mut tracking_keys = Organization::tracking_keys_for_ids(
        vec![organization.id],
        &state.config.api_keys_encryption_key,
        connection,
    )?
    .get(&organization.id)
    .unwrap_or(&TrackingKeys {
        ..Default::default()
    })
    .clone();

    if let Some(ref pixel) = event.facebook_pixel_key {
        tracking_keys.facebook_pixel_key = Some(pixel.to_string());
    }

    let (min_ticket_price, max_ticket_price) =
        if event.publish_date.unwrap_or(times::infinity()) < dates::now().finish() || is_user_admin
        {
            event.current_ticket_pricing_range(box_office_pricing, connection)?
        } else {
            (None, None)
        };
    // Show private access code to any admin with write access
    let show_private_access_code = if let Some(user) = user {
        user.has_scope_for_organization_event(
            Scopes::EventWrite,
            &organization,
            event.id,
            connection,
        )?
    } else {
        false
    };

    let fee_in_cents = event
        .client_fee_in_cents
        .unwrap_or(organization.client_event_fee_in_cents)
        + event
            .company_fee_in_cents
            .unwrap_or(organization.company_event_fee_in_cents);
    let payload = &EventShowResult {
        id: event.id,
        private_access_code: if show_private_access_code {
            Some(event.private_access_code)
        } else {
            None
        },
        name: event.name,
        organization_id: event.organization_id,
        venue_id: event.venue_id,
        created_at: event.created_at,
        event_start: event.event_start,
        door_time: event.door_time,
        event_end: event.event_end,
        cancelled_at: event.cancelled_at,
        fee_in_cents,
        status: event.status,
        publish_date: event.publish_date,
        promo_image_url: event.promo_image_url,
        cover_image_url: event.cover_image_url,
        additional_info: event.additional_info,
        top_line_info: event.top_line_info,
        age_limit: event.age_limit,
        video_url: event.video_url,
        organization: ShortOrganization {
            id: organization.id,
            name: organization.name,
        },
        venue,
        artists: event_artists,
        ticket_types: display_ticket_types,
        total_interest,
        user_is_interested: user_interest,
        min_ticket_price,
        max_ticket_price,
        is_external: event.is_external,
        external_url: event.external_url,
        override_status: event.override_status,
        limited_tickets_remaining,
        localized_times,
        tracking_keys,
        event_type: event.event_type,
        sales_start_date,
        url: format!("{}/events/{}", &state.config.front_end_url, &event.slug),
        slug: event.slug.clone(),
        facebook_pixel_key: event.facebook_pixel_key,
        extra_admin_data: event.extra_admin_data.and_then(|data| {
            if is_user_admin {
                Some(data)
            } else {
                None
            }
        }),
    };

    Ok(HttpResponse::Ok().json(&payload))
}

pub fn publish(
    (connection, path, user): (Connection, Path<PathParameters>, AuthUser),
) -> Result<HttpResponse, BigNeonError> {
    let conn = connection.get();
    let event = Event::find(path.id, conn)?;
    user.requires_scope_for_organization_event(
        Scopes::EventWrite,
        &event.organization(conn)?,
        &event,
        conn,
    )?;
    event.publish(Some(user.id()), conn)?;

    // TODO: Remove domain action and replace with domain event EventPublished
    //       once domain events are ready #DomainEvents
    let _ = marketing_contacts::CreateEventMarketingListAction::new(event.id).enqueue(conn)?;

    Ok(HttpResponse::Ok().finish())
}

pub fn unpublish(
    (connection, path, user): (Connection, Path<PathParameters>, AuthUser),
) -> Result<HttpResponse, BigNeonError> {
    let conn = connection.get();
    let event = Event::find(path.id, conn)?;
    user.requires_scope_for_organization_event(
        Scopes::EventWrite,
        &event.organization(conn)?,
        &event,
        conn,
    )?;
    event.unpublish(Some(user.id()), conn)?;
    Ok(HttpResponse::Ok().finish())
}

#[derive(Deserialize, Serialize, Debug)]
pub struct TicketRedeemRequest {
    pub redeem_key: String,
}

pub fn redeem_ticket(
    (connection, parameters, redeem_parameters, auth_user, state): (
        Connection,
        Path<RedeemTicketPathParameters>,
        Json<TicketRedeemRequest>,
        AuthUser,
        State<AppState>,
    ),
) -> Result<HttpResponse, BigNeonError> {
    let connection = connection.get();
    let ticket = TicketInstance::find_for_processing(
        parameters.ticket_instance_id,
        parameters.id,
        connection,
    )?;
    let db_event = Event::find(ticket.event_id, connection)?;
    let organization = db_event.organization(connection)?;
    auth_user.requires_scope_for_organization_event(
        Scopes::RedeemTicket,
        &organization,
        &db_event,
        connection,
    )?;
    let redeemable =
        TicketInstance::show_redeemable_ticket(parameters.ticket_instance_id, connection)?;

    let result = TicketInstance::redeem_ticket(
        ticket.id,
        redeem_parameters.redeem_key.clone(),
        auth_user.id(),
        connection,
    )?;

    match result {
        RedeemResults::TicketRedeemSuccess => {
            //Redeem ticket on chain
            let asset = Asset::find(ticket.asset_id, connection)?;
            match asset.blockchain_asset_id {
                Some(a) => {
                    let wallet = Wallet::find(ticket.wallet_id, connection)?;
                    state.config.tari_client.modify_asset_redeem_token(&wallet.secret_key, &wallet.public_key,
                                                                       &a,
                                                                       vec![ticket.token_id as u64],
                    )?;

                    //Fetch the redeemable again to include the redeemed_by and redeemed_at fields
                    let redeemable = TicketInstance::show_redeemable_ticket(parameters.ticket_instance_id, connection)?;

                    Ok(HttpResponse::Ok().json(redeemable))
                }
                None => Ok(HttpResponse::BadRequest().json(json!({ "error": "Could not complete this checkout because the asset has not been assigned on the blockchain.".to_string()}))),
            }
        }
        RedeemResults::TicketAlreadyRedeemed => Ok(HttpResponse::Conflict().json(json!({
        "error": "Ticket has already been redeemed.".to_string(),
        "redeemed_by": redeemable.redeemed_by,
        "redeemed_at": redeemable.redeemed_at
        }))),
        RedeemResults::TicketInvalid => {
            Ok(HttpResponse::BadRequest().json(json!({"error": "Ticket is invalid.".to_string()})))
        }
    }
}

pub fn show_from_organizations(
    (connection, path, paging, user): (
        Connection,
        Path<PathParameters>,
        Query<PagingParameters>,
        AuthUser,
    ),
) -> Result<WebPayload<EventSummaryResult>, BigNeonError> {
    let conn = connection.get();
    let org = Organization::find(path.id, conn)?;
    user.requires_scope_for_organization(Scopes::OrgReadEvents, &org, conn)?;

    let user_roles = org.get_roles_for_user(&user.user, conn)?;
    let mut events = Event::find_all_events_for_organization(
        path.id,
        paging
            .get_tag("past_or_upcoming")
            .unwrap_or_else(|| "Upcoming".to_string())
            .parse()?,
        if Roles::get_event_limited_roles()
            .iter()
            .find(|r| user_roles.contains(&r))
            .is_some()
        {
            Some(user.user.get_event_ids_for_organization(org.id, conn)?)
        } else {
            None
        },
        paging.page(),
        paging.limit(),
        conn,
    )?;
    for event in events.data.iter_mut() {
        if !user.has_scope_for_organization_event(Scopes::EventDelete, &org, event.id, conn)? {
            event.eligible_for_deletion = None;
        }

        if !user.has_scope_for_organization_event(Scopes::DashboardRead, &org, event.id, conn)? {
            event.sales_total_in_cents = None;
            event.sold_held = None;
            event.sold_unreserved = None;
            for tt in event.ticket_types.iter_mut() {
                tt.sold_unreserved = None;
                tt.sold_held = None;
                tt.sales_total_in_cents = None;
            }
        }
    }
    Ok(WebPayload::new(StatusCode::OK, events))
}

#[derive(Deserialize)]
pub struct DashboardParameters {
    start_utc: Option<NaiveDate>,
    // Defaults to 29 days ago if not provided
    end_utc: Option<NaiveDate>, // Defaults to today if not provided
}

#[derive(Deserialize, Serialize)]
pub struct DashboardResult {
    pub event: EventSummaryResult,
    pub day_stats: Vec<DayStats>,
}

pub fn dashboard(
    (connection, path, query, user): (
        Connection,
        Path<PathParameters>,
        Query<DashboardParameters>,
        AuthUser,
    ),
) -> Result<HttpResponse, BigNeonError> {
    let conn = connection.get();
    let event = Event::find(path.id, conn)?;
    user.requires_scope_for_organization_event(
        Scopes::DashboardRead,
        &event.organization(conn)?,
        &event,
        conn,
    )?;
    let summary = event.summary(conn)?;
    let end_utc = match query.end_utc {
        Some(end_utc) => end_utc,
        None => std::cmp::min(
            Utc::now().naive_utc().date(),
            event.event_end.unwrap_or(Utc::now().naive_utc()).date(),
        ),
    };

    let start_utc = query.start_utc.unwrap_or(end_utc - Duration::days(29));

    let day_stats = event.get_sales_by_date_range(start_utc, end_utc, conn)?;

    Ok(HttpResponse::Ok().json(DashboardResult {
        event: summary,
        day_stats,
    }))
}

#[derive(Deserialize, Debug)]
pub struct AddArtistRequest {
    pub artist_id: Uuid,
    pub rank: i32,
    pub set_time: Option<NaiveDateTime>,
    pub importance: i32,
    pub stage_id: Option<Uuid>,
}

pub fn create(
    (connection, new_event, user): (Connection, Json<NewEvent>, AuthUser),
) -> Result<HttpResponse, BigNeonError> {
    let connection = connection.get();
    let organization = Organization::find(new_event.organization_id, connection)?;
    user.requires_scope_for_organization(Scopes::EventWrite, &organization, connection)?;

    let new_event = new_event.into_inner();

    let event = new_event.commit(Some(user.id()), connection)?;

    Ok(HttpResponse::Created().json(event))
}

pub fn update(
    (connection, parameters, event_parameters, user): (
        Connection,
        Path<PathParameters>,
        Json<EventEditableAttributes>,
        AuthUser,
    ),
) -> Result<HttpResponse, BigNeonError> {
    let connection = connection.get();
    let event = Event::find(parameters.id, connection)?;
    let organization = event.organization(connection)?;
    user.requires_scope_for_organization_event(
        Scopes::EventWrite,
        &organization,
        &event,
        connection,
    )?;

    let event_parameters = event_parameters.into_inner();
    // Not sure about this at this time, we don't want to have 404's on the old URL
    //       if let Some(ref name) = event_parameters.name {
    //        event_parameters.slug = Some(event_parameters.slug.unwrap_or(create_slug(name)));
    //    }

    let updated_event = event.update(Some(user.id()), event_parameters, connection)?;
    Ok(HttpResponse::Ok().json(&updated_event))
}

pub fn delete(
    (connection, parameters, user): (Connection, Path<PathParameters>, AuthUser),
) -> Result<HttpResponse, BigNeonError> {
    let connection = connection.get();
    let event = Event::find(parameters.id, connection)?;
    let organization = event.organization(connection)?;
    user.requires_scope_for_organization_event(
        Scopes::EventDelete,
        &organization,
        &event,
        connection,
    )?;

    event.delete(user.id(), connection)?;
    Ok(HttpResponse::Ok().json({}))
}

pub fn cancel(
    (connection, parameters, user): (Connection, Path<PathParameters>, AuthUser),
) -> Result<HttpResponse, BigNeonError> {
    let connection = connection.get();
    let event = Event::find(parameters.id, connection)?;
    let organization = event.organization(connection)?;
    user.requires_scope_for_organization_event(
        Scopes::EventCancel,
        &organization,
        &event,
        connection,
    )?;

    //Doing this in the DB layer so it can use the DB time as now.
    let updated_event = event.cancel(Some(user.id()), connection)?;

    Ok(HttpResponse::Ok().json(&updated_event))
}

pub fn list_interested_users(
    (connection, path_parameters, query, user): (
        Connection,
        Path<PathParameters>,
        Query<PagingParameters>,
        AuthUser,
    ),
) -> Result<HttpResponse, BigNeonError> {
    user.requires_scope(Scopes::EventInterest)?;

    let connection = connection.get();
    let paging: Paging = query.clone().into();

    let payload = EventInterest::list_interested_users(
        path_parameters.id,
        user.id(),
        paging.page * paging.limit,
        (paging.page * paging.limit) + paging.limit,
        connection,
    )?;
    Ok(HttpResponse::Ok().json(&payload))
}

pub fn add_interest(
    (connection, parameters, user): (Connection, Path<PathParameters>, AuthUser),
) -> Result<HttpResponse, BigNeonError> {
    user.requires_scope(Scopes::EventInterest)?;

    let connection = connection.get();
    let event_interest = EventInterest::create(parameters.id, user.id()).commit(connection)?;
    Ok(HttpResponse::Created().json(&event_interest))
}

pub fn remove_interest(
    (connection, parameters, user): (Connection, Path<PathParameters>, AuthUser),
) -> Result<HttpResponse, BigNeonError> {
    user.requires_scope(Scopes::EventInterest)?;

    let connection = connection.get();
    let event_interest = EventInterest::remove(parameters.id, user.id(), connection)?;
    Ok(HttpResponse::Ok().json(&event_interest))
}

pub fn add_artist(
    (connection, parameters, event_artist, user): (
        Connection,
        Path<PathParameters>,
        Json<AddArtistRequest>,
        AuthUser,
    ),
) -> Result<HttpResponse, BigNeonError> {
    let connection = connection.get();
    let event = Event::find(parameters.id, connection)?;
    let organization = event.organization(connection)?;
    user.requires_scope_for_organization_event(
        Scopes::EventWrite,
        &organization,
        &event,
        connection,
    )?;

    let event_artist = EventArtist::create(
        parameters.id,
        event_artist.artist_id,
        event_artist.rank,
        event_artist.set_time,
        event_artist.importance,
        event_artist.stage_id,
    )
    .commit(Some(user.id()), connection)?;

    // Trigger update for event and associated users in background
    let action = DomainAction::create(
        None,
        DomainActionTypes::UpdateGenres,
        None,
        json!(UpdateGenresPayload { user_id: user.id() }),
        Some(Tables::Events.to_string()),
        Some(event.id),
    );
    action.commit(connection)?;

    Ok(HttpResponse::Created().json(&event_artist))
}

#[derive(Deserialize, Debug, Default)]
pub struct UpdateArtistsRequest {
    pub artist_id: Uuid,
    pub set_time: Option<NaiveDateTime>,
    pub importance: i32,
    pub stage_id: Option<Uuid>,
}

#[derive(Deserialize, Debug, Default)]
pub struct UpdateArtistsRequestList {
    pub artists: Vec<UpdateArtistsRequest>,
}

pub fn update_artists(
    (connection, parameters, artists, user): (
        Connection,
        Path<PathParameters>,
        Json<UpdateArtistsRequestList>,
        AuthUser,
    ),
) -> Result<HttpResponse, BigNeonError> {
    let connection = connection.get();
    let event = Event::find(parameters.id, connection)?;
    let organization = event.organization(connection)?;
    user.requires_scope_for_organization_event(
        Scopes::EventWrite,
        &organization,
        &event,
        connection,
    )?;

    EventArtist::clear_all_from_event(parameters.id, connection)?;

    let mut rank = 0;
    let mut added_artists: Vec<EventArtist> = Vec::new();

    for a in &artists.into_inner().artists {
        added_artists.push(
            EventArtist::create(
                parameters.id,
                a.artist_id,
                rank,
                a.set_time,
                a.importance,
                a.stage_id,
            )
            .commit(Some(user.id()), connection)?,
        );
        rank += 1;
    }

    let action = DomainAction::create(
        None,
        DomainActionTypes::UpdateGenres,
        None,
        json!(UpdateGenresPayload { user_id: user.id() }),
        Some(Tables::Events.to_string()),
        Some(event.id),
    );
    action.commit(connection)?;

    Ok(HttpResponse::Ok().json(&added_artists))
}

#[derive(Deserialize, Clone)]
pub struct GuestListQueryParameters {
    pub changes_since: Option<NaiveDateTime>,
    #[serde(default, deserialize_with = "deserialize_unless_blank")]
    pub query: Option<String>,
    pub page: Option<u32>,
    pub limit: Option<i32>,
}

impl From<GuestListQueryParameters> for Paging {
    fn from(s: GuestListQueryParameters) -> Paging {
        let mut default_tags: HashMap<String, Value> = HashMap::new();
        default_tags.insert("query".to_owned(), json!(s.query.clone()));
        default_tags.insert("changes_since".to_owned(), json!(s.changes_since.clone()));

        //TODO Replace u32::MAX with our default of 100
        let limit: u32 = match s.limit {
            Some(limit) => {
                if limit > 0 {
                    limit as u32
                } else {
                    std::u32::MAX
                }
            }
            None => std::u32::MAX,
        };

        PagingParameters {
            page: s.page,
            limit: Some(limit),
            sort: None,
            dir: None,
            tags: default_tags,
        }
        .into()
    }
}

pub fn guest_list(
    (connection, query, path, user): (
        Connection,
        Query<GuestListQueryParameters>,
        Path<PathParameters>,
        AuthUser,
    ),
) -> Result<HttpResponse, BigNeonError> {
    //TODO refactor GuestListQueryParameters to PagingParameters
    let conn = connection.get();
    let event = Event::find(path.id, conn)?;
    user.requires_scope_for_organization_event(
        Scopes::EventViewGuests,
        &event.organization(conn)?,
        &event,
        conn,
    )?;

    let query_string = query.clone().query;
    let changes_since = query.clone().changes_since;
    let paging = query.clone().into();
    let tickets_and_total = event.guest_list(query_string, &changes_since, Some(&paging), conn)?;
    let (tickets, total) = tickets_and_total;

    #[derive(Serialize)]
    struct TicketRefundable {
        #[serde(flatten)]
        ticket: RedeemableTicket,
        #[serde(flatten)]
        pending_transfer: PendingTransfer,
        refund_supported: bool,
    }

    let mut tickets_refund: Vec<TicketRefundable> = Vec::new();

    for t in tickets {
        let mut refundable = t.providers.len() != 0;
        for p in t.providers {
            if !ServiceLocator::is_refund_supported(p) {
                refundable = false;
            }
        }

        tickets_refund.push(TicketRefundable {
            ticket: t.ticket.clone(),
            pending_transfer: t.pending_transfer.clone().unwrap_or(PendingTransfer {
                ..Default::default()
            }),
            refund_supported: refundable,
        });
    }

    let mut payload = Payload::new(tickets_refund, query.into_inner().into());
    payload.paging.total = total as u64;
    payload.paging.limit = paging.limit;
    Ok(HttpResponse::Ok().json(payload))
}

pub fn codes(
    (conn, query, path, user): (
        Connection,
        Query<PagingParameters>,
        Path<PathParameters>,
        AuthUser,
    ),
) -> Result<HttpResponse, BigNeonError> {
    let conn = conn.get();
    let event = Event::find(path.id, conn)?;
    user.requires_scope_for_organization_event(
        Scopes::CodeRead,
        &event.organization(conn)?,
        &event,
        conn,
    )?;

    let mut code_type: Option<CodeTypes> = None;
    if let Some(value) = query.tags.get("type") {
        code_type = serde_json::from_value(value.clone())?;
    }

    //TODO: remap query to use paging info
    let codes = Code::find_for_event(path.id, code_type, conn)?;
    let mut payload = Payload::from_data(codes, query.page(), query.limit());
    payload.paging.tags = query.tags.clone();

    Ok(HttpResponse::Ok().json(payload))
}

pub fn holds(
    (conn, query, path, user): (
        Connection,
        Query<PagingParameters>,
        Path<PathParameters>,
        AuthUser,
    ),
) -> Result<HttpResponse, BigNeonError> {
    let conn = conn.get();
    let event = Event::find(path.id, conn)?;
    let organization = &event.organization(conn)?;
    user.requires_scope_for_organization_event(Scopes::HoldRead, &organization, &event, conn)?;
    let holds = Hold::find_for_event(path.id, false, conn)?;
    let mut ticket_type_ids: Vec<Uuid> = holds.iter().map(|h| h.ticket_type_id).collect();
    ticket_type_ids.sort();
    ticket_type_ids.dedup();
    let ticket_types = TicketType::find_by_ids(&ticket_type_ids, conn)?;
    let mut ticket_types_map = HashMap::new();
    for ticket_type in ticket_types {
        ticket_types_map.insert(
            ticket_type.id,
            (
                ticket_type.clone(),
                ticket_type.current_ticket_pricing(false, conn).optional()?,
            ),
        );
    }

    #[derive(Serialize)]
    struct R {
        pub id: Uuid,
        pub name: String,
        pub event_id: Uuid,
        pub redemption_code: Option<String>,
        pub discount_in_cents: Option<i64>,
        pub end_at: Option<NaiveDateTime>,
        pub max_per_user: Option<i64>,
        pub hold_type: HoldTypes,
        pub ticket_type_id: Uuid,
        pub ticket_type_name: String,
        pub price_in_cents: Option<u32>,
        pub available: u32,
        pub quantity: u32,
        pub children_available: u32,
        pub children_quantity: u32,
        pub parent_hold_id: Option<Uuid>,
    }

    let mut list = Vec::<R>::new();
    for hold in holds {
        let (quantity, available) = hold.quantity(conn)?;
        let (children_quantity, children_available) = hold.children_quantity(conn)?;
        let (ticket_type, current_ticket_pricing) = ticket_types_map
            .get(&hold.ticket_type_id)
            .ok_or_else(|| ApplicationError::new("Failed to load hold ticket type".to_string()))?;
        let r = R {
            id: hold.id,
            name: hold.name,
            event_id: hold.event_id,
            redemption_code: hold.redemption_code,
            discount_in_cents: hold.discount_in_cents,
            end_at: hold.end_at,
            max_per_user: hold.max_per_user,
            hold_type: hold.hold_type,
            ticket_type_id: hold.ticket_type_id,
            ticket_type_name: ticket_type.name.clone(),
            price_in_cents: current_ticket_pricing
                .clone()
                .map(|tp| tp.price_in_cents as u32),
            available,
            quantity,
            children_available,
            children_quantity,
            parent_hold_id: hold.parent_hold_id,
        };

        list.push(r);
    }

    Ok(HttpResponse::Ok().json(Payload::from_data(list, query.page(), query.limit())))
}

pub fn fans_index(
    (connection, query, path, user): (
        Connection,
        Query<PagingParameters>,
        Path<PathParameters>,
        AuthUser,
    ),
) -> Result<WebPayload<DisplayFan>, BigNeonError> {
    let connection = connection.get();
    let event = Event::find(path.id, connection)?;
    let org = event.organization(connection)?;
    user.requires_scope_for_organization_event(Scopes::OrgFans, &org, &event, &connection)?;
    let payload = event.search_fans(
        query.get_tag("query"),
        query.page(),
        query.limit(),
        query
            .sort
            .as_ref()
            .map(|s| s.parse().unwrap_or(FanSortField::LastOrder))
            .unwrap_or(FanSortField::UserCreated),
        query.dir.unwrap_or(SortingDir::Desc),
        connection,
    )?;
    Ok(WebPayload::new(StatusCode::OK, payload))
}

pub fn users(
    (connection, path_parameters, query_parameters, user): (
        Connection,
        Path<PathParameters>,
        Query<PagingParameters>,
        AuthUser,
    ),
) -> Result<WebPayload<DisplayOrganizationUser>, BigNeonError> {
    let connection = connection.get();
    let event = Event::find(path_parameters.id, connection)?;
    let organization = event.organization(connection)?;
    user.requires_scope_for_organization_event(Scopes::OrgRead, &organization, &event, connection)?;

    let mut members: Vec<DisplayOrganizationUser> = organization
        .users(Some(event.id), connection)?
        .into_iter()
        .map(|u| DisplayOrganizationUser {
            user_id: Some(u.1.id),
            first_name: u.1.first_name,
            last_name: u.1.last_name,
            email: u.1.email,
            roles: u.0.role,
            invite_or_member: "member".to_string(),
            invite_id: None,
        })
        .collect();

    for inv in organization.pending_invites(Some(event.id), connection)? {
        members.push(DisplayOrganizationUser {
            user_id: inv.user_id,
            first_name: None,
            last_name: None,
            email: Some(inv.user_email),
            roles: inv.roles,
            invite_or_member: "invite".to_string(),
            invite_id: Some(inv.id),
        });
    }

    let payload = Payload::from_data(members, query_parameters.page(), query_parameters.limit());
    Ok(WebPayload::new(StatusCode::OK, payload))
}

#[derive(Deserialize)]
pub struct EventUserPathParams {
    id: Uuid,
    user_id: Uuid,
}

pub fn remove_user(
    (connection, path, user): (Connection, Path<EventUserPathParams>, AuthUser),
) -> Result<HttpResponse, BigNeonError> {
    let connection = connection.get();
    let event = Event::find(path.id, connection)?;
    let organization = event.organization(connection)?;
    user.requires_scope_for_organization_event(
        Scopes::OrgUsers,
        &organization,
        &event,
        connection,
    )?;

    let event_user =
        EventUser::find_by_event_id_user_id(event.id, path.user_id, connection).optional()?;
    match event_user {
        Some(event_user) => {
            event_user.destroy(connection)?;
            Ok(HttpResponse::Ok().json(&organization))
        }
        None => Ok(HttpResponse::Ok().finish()),
    }
}

fn event_venues_from_events(
    events: Vec<Event>,
    user: Option<User>,
    state: &State<AppState>,
    connection: &PgConnection,
) -> Result<Vec<EventVenueEntry>, DatabaseError> {
    let mut venue_ids: Vec<Uuid> = events
        .iter()
        .filter(|e| e.venue_id.is_some())
        .map(|e| e.venue_id.unwrap())
        .collect();
    venue_ids.sort();
    venue_ids.dedup();

    let event_ticket_range_mapping = Event::ticket_pricing_range_by_events(
        events.iter().map(|e| e.id).collect::<Vec<Uuid>>(),
        false,
        connection,
    )?;

    let venues = Venue::find_by_ids(venue_ids, connection)?;
    let venue_map = venues.into_iter().fold(HashMap::new(), |mut map, v| {
        map.insert(v.id, v.clone());
        map
    });

    let event_ids = events.iter().map(|e| e.id).collect();
    let mut artists_map = EventArtist::find_all_from_events(event_ids, connection)?;

    let mut organization_ids: Vec<Uuid> = events.iter().map(|e| e.organization_id).collect();
    organization_ids.sort();
    organization_ids.dedup();

    let tracking_keys_for_orgs = Organization::tracking_keys_for_ids(
        organization_ids,
        &state.config.api_keys_encryption_key,
        connection,
    )?;

    let event_interest = match user {
        Some(ref u) => EventInterest::find_interest_by_event_ids_for_user(
            events.iter().map(|e| e.id).collect::<Vec<Uuid>>(),
            u.id,
            connection,
        )?,
        None => HashMap::new(),
    };

    let mut results: Vec<EventVenueEntry> = Vec::new();

    for event in events.into_iter() {
        let venue = event.venue_id.and_then(|v| Some(venue_map[&v].clone()));
        let artists = artists_map.remove(&event.id).map_or(Vec::new(), |x| x);
        let mut min_ticket_price = None;
        let mut max_ticket_price = None;
        if let Some((min, max)) = event_ticket_range_mapping.get(&event.id) {
            min_ticket_price = Some(*min);
            max_ticket_price = Some(*max);
        }

        let localized_times = event.get_all_localized_time_strings(venue.as_ref());
        let organization_id = event.organization_id;
        let tracking_keys = tracking_keys_for_orgs
            .get(&organization_id)
            .unwrap_or(&TrackingKeys {
                ..Default::default()
            })
            .clone();

        results.push(EventVenueEntry {
            venue,
            artists: Some(artists),
            id: event.id,
            name: event.name,
            organization_id,
            venue_id: event.venue_id,
            created_at: event.created_at,
            updated_at: event.updated_at,
            slug: event.slug.clone(),
            event_start: event.event_start,
            door_time: event.door_time,
            status: event.status,
            publish_date: event.publish_date,
            promo_image_url: event.promo_image_url,
            additional_info: event.additional_info,
            top_line_info: event.top_line_info,
            age_limit: event.age_limit,
            cancelled_at: event.cancelled_at,
            min_ticket_price,
            max_ticket_price,
            is_external: event.is_external,
            external_url: event.external_url,
            user_is_interested: event_interest
                .get(&event.id)
                .map(|i| i.to_owned())
                .unwrap_or(false),
            localized_times,
            tracking_keys,
            event_type: event.event_type,
            url: format!("{}/events/{}", state.config.front_end_url, &event.slug),
            event_end: event.event_end,
        });
    }
    Ok(results)
}

#[derive(Deserialize)]
pub struct LinkQueryParameters {
    source: Option<String>,
    medium: Option<String>,
    campaign: Option<String>,
    term: Option<String>,
    content: Option<String>,
}

#[derive(Serialize)]
pub struct LinkResult {
    pub link: String,
}

pub fn create_link(
    (path, query, state, user, conn): (
        Path<PathParameters>,
        Json<LinkQueryParameters>,
        State<AppState>,
        AuthUser,
        Connection,
    ),
) -> Result<HttpResponse, BigNeonError> {
    let conn = conn.get();
    let event = Event::find(path.id, conn)?;

    user.requires_scope_for_organization_event(
        Scopes::EventWrite,
        &event.organization(conn)?,
        &event,
        conn,
    )?;

    let query = query.into_inner();
    let long_link = format!(
        "{}/events/{}/tickets?utm_source={}&utm_medium={}&utm_campaign={}&utm_term={}&utm_content={}",
        state.config.front_end_url,
        path.id,
        query.source.as_ref().unwrap_or(&"".to_string()),
        query.medium.as_ref().unwrap_or(&"".to_string()),
        query.campaign.as_ref().unwrap_or(&"".to_string()),
        query.term.as_ref().unwrap_or(&"".to_string()),
        query.content.as_ref().unwrap_or(&"".to_string())
    );
    let deep_linker = state.service_locator.create_deep_linker()?;
    let short_link = deep_linker.create_deep_link(&long_link)?;
    Ok(HttpResponse::Ok().json(LinkResult { link: short_link }))
}
