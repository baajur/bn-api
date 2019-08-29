use chrono::prelude::*;
use chrono::Utc;
use chrono_tz::Tz;
use diesel;
use diesel::dsl::{exists, select};
use diesel::expression::dsl;
use diesel::expression::sql_literal::sql;
use diesel::pg::types::sql_types::Array;
use diesel::prelude::*;
use diesel::sql_types::{
    BigInt, Bool, Date, Integer, Jsonb, Nullable, Text, Timestamp, Uuid as dUuid,
};
use log::Level;
use models::*;
use regex::Regex;
use schema::{
    artists, assets, event_artists, event_genres, events, genres, order_items, orders,
    organization_users, organizations, payments, ticket_instances, ticket_types, transfer_tickets,
    transfers, venues,
};
use serde::Deserializer;
use serde_json::Value;
use serde_with::rust::double_option;
use services::*;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashMap;
use time::Duration;
use unidecode::unidecode;
use utils::errors::*;
use utils::pagination::*;
use utils::rand::random_alpha_string;
use utils::{regexes, text};
use uuid::Uuid;
use validator::{Validate, ValidationErrors};
use validators;
use validators::*;

#[derive(Associations, Identifiable, Queryable)]
#[belongs_to(Organization)]
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[belongs_to(Venue)]
#[table_name = "events"]
pub struct Event {
    pub id: Uuid,
    pub name: String,
    pub organization_id: Uuid,
    pub venue_id: Option<Uuid>,
    pub created_at: NaiveDateTime,
    pub event_start: Option<NaiveDateTime>,
    pub door_time: Option<NaiveDateTime>,
    pub status: EventStatus,
    pub publish_date: Option<NaiveDateTime>,
    pub redeem_date: Option<NaiveDateTime>,
    pub promo_image_url: Option<String>,
    pub additional_info: Option<String>,
    pub age_limit: Option<String>,
    pub top_line_info: Option<String>,
    pub cancelled_at: Option<NaiveDateTime>,
    pub updated_at: NaiveDateTime,
    pub video_url: Option<String>,
    pub is_external: bool,
    pub external_url: Option<String>,
    pub override_status: Option<EventOverrideStatus>,
    pub client_fee_in_cents: Option<i64>,
    pub company_fee_in_cents: Option<i64>,
    pub settlement_amount_in_cents: Option<i64>,
    pub event_end: Option<NaiveDateTime>,
    pub sendgrid_list_id: Option<i64>,
    pub event_type: EventTypes,
    pub cover_image_url: Option<String>,
    pub private_access_code: Option<String>,
    pub slug: String,
    pub facebook_pixel_key: Option<String>,
    pub deleted_at: Option<NaiveDateTime>,
    pub extra_admin_data: Option<Value>,
}

impl PartialOrd for Event {
    fn partial_cmp(&self, other: &Event) -> Option<Ordering> {
        Some(self.id.cmp(&other.id))
    }
}

pub fn from_str_or_num_to_str<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Value = Deserialize::deserialize(deserializer)?;

    if value.is_string() {
        Ok(Some(String::from(value.as_str().unwrap_or(""))))
    } else if value.is_number() {
        Ok(Some(String::from(
            value.as_f64().unwrap_or(0f64).to_string(),
        )))
    } else {
        Ok(None)
    }
}

#[derive(Default, Insertable, Serialize, Deserialize, Validate, Clone)]
#[table_name = "events"]
pub struct NewEvent {
    pub name: String,
    pub organization_id: Uuid,
    pub venue_id: Option<Uuid>,
    pub event_start: Option<NaiveDateTime>,
    pub door_time: Option<NaiveDateTime>,
    #[serde(default = "NewEvent::default_status", skip_deserializing)]
    pub status: EventStatus,
    pub publish_date: Option<NaiveDateTime>,
    pub redeem_date: Option<NaiveDateTime>,
    #[validate(url(message = "Promo image URL is invalid"))]
    #[serde(default, deserialize_with = "deserialize_unless_blank")]
    pub promo_image_url: Option<String>,
    #[validate(url(message = "Cover image URL is invalid"))]
    #[serde(default, deserialize_with = "deserialize_unless_blank")]
    pub cover_image_url: Option<String>,
    #[serde(default, deserialize_with = "deserialize_unless_blank")]
    pub additional_info: Option<String>,
    #[serde(default, deserialize_with = "from_str_or_num_to_str")]
    #[validate(length(
        max = "255",
        message = "Age limit must be less than 255 characters long"
    ))]
    pub age_limit: Option<String>,
    #[validate(length(
        max = "100",
        message = "Top line info must be at most 100 characters long"
    ))]
    #[serde(default, deserialize_with = "deserialize_unless_blank")]
    pub top_line_info: Option<String>,
    #[serde(default, deserialize_with = "deserialize_unless_blank")]
    #[validate(url(message = "Video URL is invalid"))]
    pub video_url: Option<String>,
    #[serde(default = "NewEvent::default_is_external")]
    pub is_external: bool,
    #[validate(url(message = "External URL is invalid"))]
    #[serde(default, deserialize_with = "deserialize_unless_blank")]
    pub external_url: Option<String>,
    #[serde(default, deserialize_with = "deserialize_unless_blank")]
    pub override_status: Option<EventOverrideStatus>,
    pub event_end: Option<NaiveDateTime>,
    pub event_type: EventTypes,
    #[serde(default, deserialize_with = "deserialize_unless_blank")]
    pub private_access_code: Option<String>,
    #[serde(default, deserialize_with = "deserialize_unless_blank")]
    pub slug: Option<String>,

    #[serde(default, deserialize_with = "deserialize_unless_blank")]
    pub facebook_pixel_key: Option<String>,
    pub extra_admin_data: Option<Value>,
}

impl NewEvent {
    pub fn commit(
        &self,
        current_user_id: Option<Uuid>,
        conn: &PgConnection,
    ) -> Result<Event, DatabaseError> {
        self.validate()?;
        let mut new_event = self.clone();

        match new_event.event_start {
            Some(event_start) => {
                if new_event.event_end.is_none() {
                    new_event.event_end =
                        Some(NaiveDateTime::from(event_start + Duration::days(1)));
                }
                if new_event.door_time.is_none() {
                    new_event.door_time =
                        Some(NaiveDateTime::from(event_start - Duration::hours(1)));
                }
            }
            None => (),
        }

        validators::append_validation_error(
            Ok(()),
            "event.event_end",
            validators::n_date_valid(
                new_event.event_start,
                new_event.event_end,
                "event_end_before_event_start",
                "Event End must be after Event Start",
                "event_start",
                "event_end",
            ),
        )?;
        if new_event.slug.is_none() {
            let slug = create_slug(&new_event.name);
            new_event.slug = Some(slug.clone());
            loop {
                let existing =
                    Event::find_by_slug(new_event.slug.as_ref().unwrap(), conn).optional()?;
                if existing.is_none() {
                    break;
                }
                new_event.slug = Some(format!("{}-{}", &slug, random_alpha_string(5)));
            }
        }

        let result: Event = diesel::insert_into(events::table)
            .values(&new_event)
            .get_result(conn)
            .to_db_error(ErrorCode::InsertError, "Could not create new event")?;

        DomainEvent::create(
            DomainEventTypes::EventCreated,
            format!("Event '{}' created", &self.name),
            Tables::Events,
            Some(result.id),
            current_user_id,
            Some(json!(&new_event)),
        )
        .commit(conn)?;

        Ok(result)
    }

    pub fn default_status() -> EventStatus {
        EventStatus::Draft
    }

    pub fn default_is_external() -> bool {
        false
    }
}

fn create_slug(name: &str) -> String {
    // Unwrap should be treated as a compile time error

    let only_characters = Regex::new(r#"[^a-zA-Z0-9]"#).unwrap();
    let duplicate_dashes = Regex::new(r#"-+"#).unwrap();

    let slug = unidecode(name);
    let slug = only_characters.replace_all(&slug, " ");
    let mut slug: String = duplicate_dashes
        .replace_all(&regexes::whitespace().replace_all(&slug.trim(), "-"), "-")
        .to_lowercase()
        .chars()
        .take(250)
        .collect();

    // If the slug is empty, generate a short random string
    if slug.len() == 0 {
        slug = random_alpha_string(5);
    }
    slug
}

#[derive(AsChangeset, Default, Deserialize, Validate, Serialize)]
#[table_name = "events"]
pub struct EventEditableAttributes {
    pub name: Option<String>,
    pub venue_id: Option<Uuid>,
    pub event_start: Option<NaiveDateTime>,
    pub door_time: Option<NaiveDateTime>,
    #[serde(default, deserialize_with = "double_option::deserialize")]
    pub publish_date: Option<Option<NaiveDateTime>>,
    pub redeem_date: Option<NaiveDateTime>,
    #[validate(url(message = "Promo image URL is invalid"))]
    #[serde(default, deserialize_with = "double_option_deserialize_unless_blank")]
    pub promo_image_url: Option<Option<String>>,
    #[validate(url(message = "Cover image URL is invalid"))]
    #[serde(default, deserialize_with = "double_option_deserialize_unless_blank")]
    pub cover_image_url: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option_deserialize_unless_blank")]
    pub additional_info: Option<Option<String>>,
    #[serde(default, deserialize_with = "from_str_or_num_to_str")]
    #[validate(length(
        max = "255",
        message = "Age limit must be less than 255 characters long"
    ))]
    pub age_limit: Option<String>,
    pub cancelled_at: Option<NaiveDateTime>,
    #[validate(length(
        max = "100",
        message = "Top line info must be at most 100 characters long"
    ))]
    #[serde(default, deserialize_with = "double_option_deserialize_unless_blank")]
    pub top_line_info: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option_deserialize_unless_blank")]
    #[validate(url(message = "Video URL is invalid"))]
    pub video_url: Option<Option<String>>,
    pub is_external: Option<bool>,
    #[validate(url(message = "External URL is invalid"))]
    #[serde(default, deserialize_with = "double_option_deserialize_unless_blank")]
    pub external_url: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option_deserialize_unless_blank")]
    pub override_status: Option<Option<EventOverrideStatus>>,
    pub event_end: Option<NaiveDateTime>,
    #[validate(length(max = "6", message = "Access code must be at most 6 characters long"))]
    #[serde(default, deserialize_with = "double_option_deserialize_unless_blank")]
    pub private_access_code: Option<Option<String>>,
    pub sendgrid_list_id: Option<i64>,
    pub event_type: Option<EventTypes>,
    pub slug: Option<String>,
    #[serde(default, deserialize_with = "double_option_deserialize_unless_blank")]
    pub facebook_pixel_key: Option<Option<String>>,
}

#[derive(Debug, Default, PartialEq, Serialize)]
pub struct EventLocalizedTimes {
    pub event_start: Option<DateTime<Tz>>,
    pub event_end: Option<DateTime<Tz>>,
    pub door_time: Option<DateTime<Tz>>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EventLocalizedTimeStrings {
    pub event_start: Option<String>,
    pub event_end: Option<String>,
    pub door_time: Option<String>,
}

impl Event {
    pub fn eligible_for_deletion(&self, conn: &PgConnection) -> Result<bool, DatabaseError> {
        // An event is ineligible for deletion if it is present in any cart
        Ok(self.deleted_at.is_none()
            && !select(exists(
                order_items::table.filter(order_items::event_id.eq(Some(self.id))),
            ))
            .get_result(conn)
            .to_db_error(
                ErrorCode::QueryError,
                "Could not check event for deletion eligibility",
            )?)
    }

    pub fn delete(self, user_id: Uuid, conn: &PgConnection) -> Result<(), DatabaseError> {
        if !&self.eligible_for_deletion(conn)? {
            return DatabaseError::business_process_error("Event is ineligible for deletion");
        }

        diesel::update(&self)
            .set((
                events::deleted_at.eq(dsl::now.nullable()),
                events::updated_at.eq(dsl::now),
            ))
            .execute(conn)
            .to_db_error(ErrorCode::UpdateError, "Could not delete event")?;

        DomainEvent::create(
            DomainEventTypes::EventDeleted,
            "Event deleted".to_string(),
            Tables::Events,
            Some(self.id),
            Some(user_id),
            None,
        )
        .commit(conn)?;
        Ok(())
    }

    pub fn genres(&self, conn: &PgConnection) -> Result<Vec<String>, DatabaseError> {
        genres::table
            .inner_join(event_genres::table.on(event_genres::genre_id.eq(genres::id)))
            .filter(event_genres::event_id.eq(self.id))
            .select(genres::name)
            .order_by(genres::name)
            .get_results(conn)
            .to_db_error(ErrorCode::QueryError, "Could not get genres for event")
    }

    pub fn update_genres(
        &self,
        user_id: Option<Uuid>,
        conn: &PgConnection,
    ) -> Result<(), DatabaseError> {
        let query = r#"
            INSERT INTO event_genres (event_id, genre_id)
            SELECT DISTINCT $1 as event_id, ag.genre_id
            FROM event_artists ea
            JOIN artist_genres ag ON ag.artist_id = ea.artist_id
            WHERE ea.event_id = $1
            AND ag.genre_id not in (select genre_id from event_genres where event_id = $1);
        "#;

        diesel::sql_query(query)
            .bind::<dUuid, _>(self.id)
            .execute(conn)
            .to_db_error(ErrorCode::QueryError, "Could not set genres on event")?;

        let query = r#"
            DELETE FROM event_genres
            WHERE NOT genre_id = ANY(
                SELECT ag.genre_id
                FROM event_artists ea
                JOIN artist_genres ag ON ag.artist_id = ea.artist_id
                WHERE ea.event_id = $1
            ) AND event_id = $1;
        "#;

        diesel::sql_query(query)
            .bind::<dUuid, _>(self.id)
            .execute(conn)
            .to_db_error(ErrorCode::QueryError, "Could not clear old genres on event")?;

        User::update_genre_info_for_associated_event_users(self.id, conn)?;

        DomainEvent::create(
            DomainEventTypes::GenresUpdated,
            "Event genres updated".to_string(),
            Tables::Events,
            Some(self.id),
            user_id,
            Some(json!({"genres": self.genres(conn)?})),
        )
        .commit(conn)?;

        Ok(())
    }

    pub fn create(
        name: &str,
        status: EventStatus,
        organization_id: Uuid,
        venue_id: Option<Uuid>,
        event_start: Option<NaiveDateTime>,
        door_time: Option<NaiveDateTime>,
        publish_date: Option<NaiveDateTime>,
        event_end: Option<NaiveDateTime>,
    ) -> NewEvent {
        NewEvent {
            name: name.into(),
            status,
            organization_id,
            venue_id,
            event_start,
            door_time,
            publish_date,
            event_end,
            ..Default::default()
        }
    }

    pub fn update(
        &self,
        current_user_id: Option<Uuid>,
        attributes: EventEditableAttributes,
        conn: &PgConnection,
    ) -> Result<Event, DatabaseError> {
        attributes.validate()?;
        let previous_start = self.event_start;
        let mut event = attributes;

        if event.private_access_code.is_some() {
            let inner_value = event.private_access_code.clone().unwrap();
            if inner_value.is_some() {
                event.private_access_code = Some(Some(inner_value.unwrap().to_lowercase()));
            }
        };

        if self.status == EventStatus::Published {
            if let Some(date) = event.publish_date {
                match date {
                    Some(_) => {}
                    None => event.publish_date = Some(Some(Utc::now().naive_utc())),
                }
            }
        }

        let event_start = match event.event_start {
            Some(e) => Some(e.clone()),
            None => self.event_start,
        };
        let event_end = match event.event_end {
            Some(e) => Some(e),
            None => self.event_end,
        };

        validators::append_validation_error(
            Ok(()),
            "event.event_end",
            validators::n_date_valid(
                event_start,
                event_end,
                "event_end_before_event_start",
                "Event End must be after Event Start",
                "event_start",
                "event_end",
            ),
        )?;

        let result: Event = DatabaseError::wrap(
            ErrorCode::UpdateError,
            "Could not update event",
            diesel::update(self)
                .set((&event, events::updated_at.eq(dsl::now)))
                .get_result(conn),
        )?;

        if previous_start != result.event_start && self.status == EventStatus::Published {
            result.regenerate_drip_actions(conn)?;
        }

        DomainEvent::create(
            DomainEventTypes::EventUpdated,
            format!("Event '{}' was updated", &self.name),
            Tables::Events,
            Some(self.id),
            current_user_id,
            Some(json!(&event)),
        );

        Ok(result)
    }

    pub fn regenerate_drip_actions(&self, conn: &PgConnection) -> Result<(), DatabaseError> {
        DomainAction::create(
            None,
            DomainActionTypes::RegenerateDripActions,
            None,
            json!({}),
            Some(Tables::Events.to_string()),
            Some(self.id),
        )
        .commit(conn)?;

        Ok(())
    }

    pub fn clear_pending_drip_actions(&self, conn: &PgConnection) -> Result<(), DatabaseError> {
        let drip_domain_actions = DomainAction::find_by_resource(
            Tables::Events.to_string(),
            self.id,
            DomainActionTypes::ProcessTransferDrip,
            DomainActionStatus::Pending,
            conn,
        )?;

        for drip_domain_action in drip_domain_actions {
            drip_domain_action.set_cancelled(conn)?;
        }

        Ok(())
    }

    pub fn ticket_pricing_range_by_events(
        event_ids: Vec<Uuid>,
        box_office_pricing: bool,
        conn: &PgConnection,
    ) -> Result<HashMap<Uuid, (i64, i64)>, DatabaseError> {
        #[derive(Debug, Queryable, QueryableByName)]
        struct R {
            #[sql_type = "dUuid"]
            event_id: Uuid,
            #[sql_type = "BigInt"]
            min_ticket_price: i64,
            #[sql_type = "BigInt"]
            max_ticket_price: i64,
        }

        let query = r#"
            SELECT
                tt.event_id,
                min(tp.price_in_cents) as min_ticket_price,
                max(tp.price_in_cents) as max_ticket_price
            FROM ticket_types tt
            JOIN ticket_pricing tp on tp.id = (
                select tp.id from ticket_pricing tp
                where tp.ticket_type_id = tt.id
                and tp.start_date < now()
                and tp.end_date > now()
                and tp.status in ('Default', 'Published')
                and tp.is_box_office_only = case when $2 = false then false else tp.is_box_office_only end
                order by tp.is_box_office_only desc, tp.status desc -- Box Office Pricing, Published, Default
                limit 1
            )
            where tt.event_id = ANY($1)
            and tt.visibility != 'Hidden'
            and tt.status = 'Published'
            and (tt.visibility != 'WhenAvailable'
                or exists (
                    select 8
                    from ticket_instances ti
                        inner join assets a
                            on a.id = ti.asset_id
                    where a.ticket_type_id = tt.id
                    and (ti.status = 'Available' or (ti.status = 'Reserved' and ti.reserved_until < now()))
                    and (ti.hold_id is null)
                ))
            GROUP BY tt.event_id;
        "#;

        let results: Vec<R> = diesel::sql_query(query)
            .bind::<Array<dUuid>, _>(event_ids)
            .bind::<Bool, _>(box_office_pricing)
            .get_results(conn)
            .to_db_error(
                ErrorCode::QueryError,
                "Could not load ticket pricing for events",
            )?;

        let mut result = HashMap::new();
        for r in results {
            result.insert(r.event_id, (r.min_ticket_price, r.max_ticket_price));
        }

        Ok(result)
    }

    pub fn current_ticket_pricing_range(
        &self,
        box_office_pricing: bool,
        conn: &PgConnection,
    ) -> Result<(Option<i64>, Option<i64>), DatabaseError> {
        if let Some((min_ticket_price, max_ticket_price)) =
            Event::ticket_pricing_range_by_events(vec![self.id], box_office_pricing, conn)?
                .get(&self.id)
        {
            Ok((Some(*min_ticket_price), Some(*max_ticket_price)))
        } else {
            Ok((None, None))
        }
    }

    pub fn pending_transfers(&self, conn: &PgConnection) -> Result<Vec<Transfer>, DatabaseError> {
        transfers::table
            .inner_join(transfer_tickets::table.on(transfers::id.eq(transfer_tickets::transfer_id)))
            .inner_join(
                ticket_instances::table
                    .on(ticket_instances::id.eq(transfer_tickets::ticket_instance_id)),
            )
            .inner_join(assets::table.on(assets::id.eq(ticket_instances::asset_id)))
            .inner_join(ticket_types::table.on(ticket_types::id.eq(assets::ticket_type_id)))
            .filter(ticket_types::event_id.eq(self.id))
            .filter(transfers::status.eq(TransferStatus::Pending))
            .select(transfers::all_columns)
            .distinct()
            .load(conn)
            .to_db_error(ErrorCode::QueryError, "Could not load event transfers")
    }

    pub fn create_next_transfer_drip_action(
        &self,
        environment: Environment,
        conn: &PgConnection,
    ) -> Result<(), DatabaseError> {
        if let Some(next_source_drip_day) = self.next_drip_date(environment) {
            let mut action = DomainAction::create(
                None,
                DomainActionTypes::ProcessTransferDrip,
                None,
                json!(ProcessTransferDripPayload {
                    event_id: self.id,
                    source_or_destination: SourceOrDestination::Destination,
                }),
                Some(Tables::Events.to_string()),
                Some(self.id),
            );
            action.schedule_at(next_source_drip_day);
            action.commit(conn)?;
        }
        Ok(())
    }

    pub fn days_until_event(&self) -> Option<i64> {
        if let Some(event_start) = self.event_start {
            let now = Utc::now().naive_utc();
            let hours_until_event = event_start.signed_duration_since(now).num_hours();
            // Full days away, with some wiggle room as these are triggered relative to the event_start
            let mut days_until_event = hours_until_event / 24;
            if days_until_event >= 0 && hours_until_event % 24 == 23 {
                days_until_event += 1;
            }

            return Some(days_until_event);
        }

        None
    }

    pub fn minutes_until_event(&self) -> Option<i64> {
        if let Some(event_start) = self.event_start {
            let now = Utc::now().naive_utc();
            let seconds_until_event = event_start.signed_duration_since(now).num_seconds();
            // Full minutes away, with some wiggle room as these are triggered relative to the event_start
            let mut minutes_until_event = seconds_until_event / 60;
            if minutes_until_event >= 0 && seconds_until_event % 60 >= 40 {
                minutes_until_event += 1;
            }

            return Some(minutes_until_event);
        }

        None
    }

    pub fn next_drip_date(&self, environment: Environment) -> Option<NaiveDateTime> {
        let now = Utc::now().naive_utc();
        if let Some(event_start) = self.event_start {
            if event_start < now {
                return None;
            }

            match environment {
                Environment::Staging => {
                    if let Some(minutes_until_event) = self.minutes_until_event() {
                        return TRANSFER_DRIP_NOTIFICATION_DAYS_PRIOR_TO_EVENT
                            .iter()
                            .find(|minutes| &minutes_until_event > minutes)
                            .map(|minutes| {
                                let duration = Duration::minutes(-*minutes);

                                event_start.checked_add_signed(duration).unwrap()
                            });
                    }
                }
                _ => {
                    if let Some(days_until_event) = self.days_until_event() {
                        return TRANSFER_DRIP_NOTIFICATION_DAYS_PRIOR_TO_EVENT
                            .iter()
                            .find(|days| &days_until_event > days)
                            .map(|days| {
                                let duration = if *days == 0 {
                                    Duration::hours(
                                        -TRANSFER_DRIP_NOTIFICATION_HOURS_PRIOR_TO_EVENT,
                                    )
                                } else {
                                    Duration::days(-*days)
                                };

                                event_start.checked_add_signed(duration).unwrap()
                            });
                    }
                }
            }
        }

        None
    }

    pub fn unpublish(
        &self,
        current_user_id: Option<Uuid>,
        conn: &PgConnection,
    ) -> Result<Event, DatabaseError> {
        let mut errors = ValidationErrors::new();
        if self.status != EventStatus::Published {
            let mut validation_error = create_validation_error(
                "event_must_be_published",
                "Event can't be un-published if it is not published",
            );
            validation_error.add_param(Cow::from("event_id"), &self.id);
            errors.add("status", validation_error);
        }

        if !errors.is_empty() {
            return Err(errors.into());
        }

        let update_fields = EventEditableAttributes {
            publish_date: Some(None),
            ..Default::default()
        };
        diesel::update(self)
            .set((
                update_fields,
                events::status.eq(EventStatus::Draft),
                events::updated_at.eq(dsl::now),
            ))
            .execute(conn)
            .to_db_error(ErrorCode::UpdateError, "Could not un-publish record")?;

        DomainEvent::create(
            DomainEventTypes::EventUnpublished,
            "Event was unpublished".to_string(),
            Tables::Events,
            Some(self.id),
            current_user_id,
            None,
        )
        .commit(conn)?;
        self.clear_pending_drip_actions(conn)?;

        Event::find(self.id, conn)
    }

    pub fn publish(
        &self,
        current_user_id: Option<Uuid>,
        conn: &PgConnection,
    ) -> Result<Event, DatabaseError> {
        if self.status == EventStatus::Published {
            return Event::find(self.id, conn);
        }

        let mut errors = ValidationErrors::new();
        if self.venue_id.is_none() {
            let mut validation_error =
                create_validation_error("required", "Event can't be published without a venue");
            validation_error.add_param(Cow::from("event_id"), &self.id);
            errors.add("venue_id", validation_error);
        }

        if self.promo_image_url.is_none() {
            let mut validation_error = create_validation_error(
                "required",
                "Event can't be published without a promo image",
            );
            validation_error.add_param(Cow::from("event_id"), &self.id);
            errors.add("promo_image_url", validation_error);
        }

        if !errors.is_empty() {
            return Err(errors.into());
        }

        match self.publish_date {
            Some(_) => diesel::update(self)
                .set((
                    events::status.eq(EventStatus::Published),
                    events::updated_at.eq(dsl::now),
                ))
                .execute(conn)
                .to_db_error(ErrorCode::UpdateError, "Could not publish record")?,
            None => diesel::update(self)
                .set((
                    events::status.eq(EventStatus::Published),
                    events::publish_date.eq(dsl::now.nullable()),
                    events::updated_at.eq(dsl::now),
                ))
                .execute(conn)
                .to_db_error(ErrorCode::UpdateError, "Could not publish record")?,
        };

        self.regenerate_drip_actions(conn)?;
        DomainEvent::create(
            DomainEventTypes::EventPublished,
            format!("Event {} published", self.name),
            Tables::Events,
            Some(self.id),
            current_user_id,
            Some(json!({"publish_date": self.publish_date})),
        )
        .commit(conn)?;
        Event::find(self.id, conn)
    }

    pub fn find_by_order_item_ids(
        order_item_ids: &Vec<Uuid>,
        conn: &PgConnection,
    ) -> Result<Vec<Event>, DatabaseError> {
        events::table
            .inner_join(organizations::table.on(events::organization_id.eq(organizations::id)))
            .inner_join(ticket_types::table.on(ticket_types::event_id.eq(events::id)))
            .inner_join(
                order_items::table.on(order_items::ticket_type_id.eq(ticket_types::id.nullable())),
            )
            .filter(order_items::id.eq_any(order_item_ids))
            .select(events::all_columns)
            .order_by(events::name.asc())
            .distinct()
            .load(conn)
            .to_db_error(ErrorCode::QueryError, "Error loading organizations")
    }

    pub fn find(id: Uuid, conn: &PgConnection) -> Result<Event, DatabaseError> {
        DatabaseError::wrap(
            ErrorCode::QueryError,
            "Error loading event",
            events::table.find(id).first::<Event>(conn),
        )
    }

    pub fn find_incl_org_venue_fees(
        id: Uuid,
        conn: &PgConnection,
    ) -> Result<(Event, Organization, Option<Venue>, FeeSchedule), DatabaseError> {
        use schema::*;
        let res: (Event, Organization, Option<Venue>, FeeSchedule) = events::table
            .inner_join(organizations::table.inner_join(fee_schedules::table))
            .left_join(venues::table)
            .filter(events::id.eq(id))
            .filter(events::deleted_at.is_null())
            .select((
                events::all_columns,
                organizations::all_columns,
                venues::all_columns.nullable(),
                fee_schedules::all_columns,
            ))
            .load(conn)
            .to_db_error(ErrorCode::QueryError, "Error loading event")
            .expect_single()?;
        Ok(res)
    }

    pub fn find_by_ids(ids: Vec<Uuid>, conn: &PgConnection) -> Result<Vec<Event>, DatabaseError> {
        events::table
            .filter(events::deleted_at.is_null())
            .filter(events::id.eq_any(ids))
            .order_by(events::name)
            .get_results(conn)
            .to_db_error(ErrorCode::QueryError, "Error loading events")
    }

    pub fn find_by_slug(
        slug: &str,
        conn: &PgConnection,
    ) -> Result<(Event, Organization, Option<Venue>, FeeSchedule), DatabaseError> {
        use schema::*;
        let res: (Event, Organization, Option<Venue>, FeeSchedule) = events::table
            .inner_join(organizations::table.inner_join(fee_schedules::table))
            .left_join(venues::table)
            .filter(events::deleted_at.is_null())
            .filter(events::slug.eq(slug))
            .select((
                events::all_columns,
                organizations::all_columns,
                venues::all_columns.nullable(),
                fee_schedules::all_columns,
            ))
            .load(conn)
            .to_db_error(ErrorCode::QueryError, "Error loading event")
            .expect_single()?;
        Ok(res)
    }

    pub fn cancel(
        self,
        current_user_id: Option<Uuid>,
        conn: &PgConnection,
    ) -> Result<Event, DatabaseError> {
        let event: Event = diesel::update(&self)
            .set(events::cancelled_at.eq(dsl::now.nullable()))
            .get_result(conn)
            .to_db_error(ErrorCode::UpdateError, "Could not update event")?;

        DomainEvent::create(
            DomainEventTypes::EventCancelled,
            format!("Event '{}' cancelled", &self.name),
            Tables::Events,
            Some(self.id),
            current_user_id,
            None,
        )
        .commit(conn)?;

        Ok(event)
    }

    pub fn is_published(&self) -> bool {
        match self.publish_date {
            None => false,
            Some(d) => d < Utc::now().naive_utc(),
        }
    }

    pub fn get_all_events_ending_between(
        organization_id: Uuid,
        start: NaiveDateTime,
        end: NaiveDateTime,
        status: EventStatus,
        conn: &PgConnection,
    ) -> Result<Vec<Event>, DatabaseError> {
        events::table
            .filter(events::deleted_at.is_null())
            .filter(events::organization_id.eq(organization_id))
            .filter(events::event_end.ge(start))
            .filter(events::event_end.le(end))
            .filter(events::status.eq(status))
            .filter(events::is_external.eq(false))
            .order_by(events::event_end.asc())
            .get_results(conn)
            .to_db_error(ErrorCode::QueryError, "Could not retrieve events")
    }

    pub fn count_report(
        self,
        start: Option<NaiveDateTime>,
        end: Option<NaiveDateTime>,
        group_by_ticket_type: bool,
        group_by_ticket_pricing: bool,
        group_by_hold: bool,
        group_by_event: bool,
        conn: &PgConnection,
    ) -> Result<TicketSalesAndCounts, DatabaseError> {
        Report::ticket_sales_and_counts(
            Some(self.id),
            Some(self.organization_id),
            start,
            end,
            group_by_ticket_type,
            group_by_ticket_pricing,
            group_by_hold,
            group_by_event,
            conn,
        )
    }
    /**
     * Returns the localized_times formatted to rfc2822
     */
    pub fn get_all_localized_time_strings(
        &self,
        venue: Option<&Venue>,
    ) -> EventLocalizedTimeStrings {
        let event_localized_times: EventLocalizedTimes = self.get_all_localized_times(venue);
        EventLocalizedTimeStrings {
            event_start: event_localized_times.event_start.map(|s| s.to_rfc2822()),
            event_end: event_localized_times.event_end.map(|s| s.to_rfc2822()),
            door_time: event_localized_times.door_time.map(|s| s.to_rfc2822()),
        }
    }

    pub fn get_all_localized_times(&self, venue: Option<&Venue>) -> EventLocalizedTimes {
        let event_localized_times: EventLocalizedTimes = EventLocalizedTimes {
            event_start: Event::localized_time_from_venue(self.event_start, venue),
            event_end: Event::localized_time_from_venue(self.event_end, venue),
            door_time: Event::localized_time_from_venue(self.door_time, venue),
        };

        event_localized_times
    }

    pub fn localized_time_from_venue(
        utc_datetime: Option<NaiveDateTime>,
        venue: Option<&Venue>,
    ) -> Option<chrono::DateTime<Tz>> {
        Event::localized_time(utc_datetime, venue.map(|v| v.timezone.as_str()))
    }

    pub fn localized_time(
        utc_datetime: Option<NaiveDateTime>,
        timezone_string: Option<&str>,
    ) -> Option<chrono::DateTime<Tz>> {
        if utc_datetime.is_none() || timezone_string.is_none() {
            return None;
        }

        let utc_datetime = utc_datetime.unwrap();
        if let Some(tz_string) = &timezone_string {
            let tz: Tz = match tz_string.parse() {
                Ok(t) => t,
                Err(e) => {
                    jlog!(Level::Error, &e);
                    return None;
                }
            };
            let utc = chrono_tz::UTC
                .ymd(
                    utc_datetime.year(),
                    utc_datetime.month(),
                    utc_datetime.day(),
                )
                .and_hms(
                    utc_datetime.hour(),
                    utc_datetime.minute(),
                    utc_datetime.second(),
                );
            let dt: chrono::DateTime<Tz> = utc.with_timezone(&tz);
            return Some(dt);
        }
        None
    }

    pub fn find_all_active_events_for_venue(
        venue_id: &Uuid,
        conn: &PgConnection,
    ) -> Result<Vec<Event>, DatabaseError> {
        DatabaseError::wrap(
            ErrorCode::QueryError,
            "Error loading event via venue",
            events::table
                .filter(events::venue_id.eq(venue_id))
                .filter(events::status.eq(EventStatus::Published))
                .filter(events::deleted_at.is_null())
                .filter(events::cancelled_at.is_null())
                .filter(events::private_access_code.is_null())
                .order_by(events::name)
                .load(conn),
        )
    }

    pub fn find_all_events_for_organization(
        organization_id: Uuid,
        past_or_upcoming: PastOrUpcoming,
        event_ids: Option<Vec<Uuid>>,
        page: u32,
        limit: u32,
        conn: &PgConnection,
    ) -> Result<paging::Payload<EventSummaryResult>, DatabaseError> {
        #[derive(QueryableByName)]
        struct Total {
            #[sql_type = "BigInt"]
            total: i64,
        };

        let mut total: Vec<Total> = diesel::sql_query(
            r#"
            SELECT CAST(count(*) as bigint) as total
            FROM events e
            WHERE e.deleted_at is null
            AND e.organization_id = $1
            AND CASE WHEN $2
                THEN
                    COALESCE(e.event_start, '31 Dec 9999') >= now()
                    OR COALESCE(e.event_end, '31 Dec 1999') > now()
                ELSE
                    COALESCE(e.event_end, '31 Dec 1999') <= now()
            END
            AND ($3 IS NULL OR e.id = ANY($3));
        "#,
        )
        .bind::<dUuid, _>(organization_id)
        .bind::<Bool, _>(past_or_upcoming == PastOrUpcoming::Upcoming)
        .bind::<Nullable<Array<dUuid>>, _>(event_ids.clone())
        .get_results(conn)
        .to_db_error(
            ErrorCode::QueryError,
            "Could not get total events for organization",
        )?;

        let mut paging = Paging::new(page, limit);
        paging.total = total.remove(0).total as u64;

        let results = Event::find_summary_data(
            organization_id,
            Some(past_or_upcoming),
            event_ids,
            page,
            limit,
            conn,
        )?;
        Ok(Payload {
            paging,
            data: results,
        })
    }

    pub fn summary(&self, conn: &PgConnection) -> Result<EventSummaryResult, DatabaseError> {
        let mut results = Event::find_summary_data(
            self.organization_id,
            None,
            Some(vec![self.id]),
            0,
            100,
            conn,
        )?;
        Ok(results.remove(0))
    }

    fn find_summary_data(
        organization_id: Uuid,
        past_or_upcoming: Option<PastOrUpcoming>,
        event_ids: Option<Vec<Uuid>>,
        page: u32,
        limit: u32,
        conn: &PgConnection,
    ) -> Result<Vec<EventSummaryResult>, DatabaseError> {
        #[derive(QueryableByName)]
        struct R {
            #[sql_type = "dUuid"]
            id: Uuid,
            #[sql_type = "Text"]
            name: String,
            #[sql_type = "dUuid"]
            organization_id: Uuid,
            #[sql_type = "Nullable<dUuid>"]
            venue_id: Option<Uuid>,
            #[sql_type = "Nullable<Text>"]
            venue_name: Option<String>,
            #[sql_type = "Nullable<Text>"]
            venue_timezone: Option<String>,
            #[sql_type = "Timestamp"]
            created_at: NaiveDateTime,
            #[sql_type = "Nullable<Timestamp>"]
            event_start: Option<NaiveDateTime>,
            #[sql_type = "Nullable<Timestamp>"]
            door_time: Option<NaiveDateTime>,
            #[sql_type = "Nullable<Timestamp>"]
            event_end: Option<NaiveDateTime>,
            #[sql_type = "Text"]
            status: EventStatus,
            #[sql_type = "Nullable<Text>"]
            promo_image_url: Option<String>,
            #[sql_type = "Nullable<Text>"]
            additional_info: Option<String>,
            #[sql_type = "Nullable<Text>"]
            top_line_info: Option<String>,
            #[sql_type = "Nullable<Text>"]
            age_limit: Option<String>,
            #[sql_type = "Nullable<Timestamp>"]
            cancelled_at: Option<NaiveDateTime>,
            #[sql_type = "Nullable<BigInt>"]
            min_price: Option<i64>,
            #[sql_type = "Nullable<BigInt>"]
            max_price: Option<i64>,
            #[sql_type = "Nullable<Timestamp>"]
            publish_date: Option<NaiveDateTime>,
            #[sql_type = "Nullable<Timestamp>"]
            on_sale: Option<NaiveDateTime>,
            #[sql_type = "Nullable<BigInt>"]
            sales_total_in_cents: Option<i64>,
            #[sql_type = "Bool"]
            is_external: bool,
            #[sql_type = "Nullable<Text>"]
            external_url: Option<String>,
            #[sql_type = "Nullable<Text>"]
            override_status: Option<EventOverrideStatus>,
            #[sql_type = "Text"]
            event_type: EventTypes,
            #[sql_type = "Text"]
            slug: String,
            #[sql_type = "Nullable<Jsonb>"]
            extra_admin_data: Option<Value>,
            #[sql_type = "Bool"]
            eligible_for_deletion: bool,
        }

        let query_events = include_str!("../queries/find_all_events_for_organization.sql");

        jlog!(Level::Debug, "Fetching summary data for event");
        let events: Vec<R> = diesel::sql_query(query_events)
            .bind::<dUuid, _>(organization_id)
            .bind::<Nullable<Bool>, _>(past_or_upcoming.map(|p| p == PastOrUpcoming::Upcoming))
            .bind::<BigInt, _>((page * limit) as i64)
            .bind::<BigInt, _>(limit as i64)
            .bind::<Nullable<Array<dUuid>>, _>(event_ids.clone())
            .get_results(conn)
            .to_db_error(
                ErrorCode::QueryError,
                "Could not load events for organization",
            )?;

        let query_ticket_types =
            include_str!("../queries/find_all_events_for_organization_ticket_type.sql");

        jlog!(Level::Debug, "Fetching summary data for ticket types");

        let ticket_types: Vec<EventSummaryResultTicketType> = diesel::sql_query(query_ticket_types)
            .bind::<dUuid, _>(organization_id)
            .bind::<Nullable<Bool>, _>(past_or_upcoming.map(|p| p == PastOrUpcoming::Upcoming))
            .bind::<Nullable<Array<dUuid>>, _>(event_ids)
            .get_results(conn)
            .to_db_error(
                ErrorCode::QueryError,
                "Could not load events' ticket types for organization",
            )?;

        let mut results: Vec<EventSummaryResult> = Vec::new();
        for r in events.into_iter() {
            let venue = if let (Some(venue_id), Some(venue_name), Some(venue_timezone)) = (
                r.venue_id.as_ref(),
                r.venue_name.as_ref(),
                r.venue_timezone.as_ref(),
            ) {
                Some(VenueInfo {
                    id: *venue_id,
                    name: venue_name.to_string(),
                    timezone: venue_timezone.to_string(),
                })
            } else {
                None
            };

            let event_id = r.id;
            let timezone = venue.as_ref().map(|v| v.timezone.as_ref());
            let localized_times: EventLocalizedTimeStrings = EventLocalizedTimeStrings {
                event_start: Event::localized_time(r.event_start, timezone).map(|s| s.to_rfc2822()),
                event_end: Event::localized_time(r.event_end, timezone).map(|s| s.to_rfc2822()),
                door_time: Event::localized_time(r.door_time, timezone).map(|s| s.to_rfc2822()),
            };

            let mut result = EventSummaryResult {
                id: r.id,
                name: r.name,
                organization_id: r.organization_id,
                venue: venue.clone(),
                created_at: r.created_at,
                event_start: r.event_start,
                door_time: r.door_time,
                status: r.status,
                promo_image_url: r.promo_image_url,
                additional_info: r.additional_info,
                top_line_info: r.top_line_info,
                age_limit: r.age_limit,
                cancelled_at: r.cancelled_at,
                max_ticket_price: r.max_price.map(|i| i as u32),
                min_ticket_price: r.min_price.map(|i| i as u32),
                publish_date: r.publish_date,
                on_sale: r.on_sale,
                total_tickets: 0,
                sold_unreserved: Some(0),
                sold_held: Some(0),
                tickets_open: 0,
                tickets_held: 0,
                tickets_redeemed: 0,
                sales_total_in_cents: Some(r.sales_total_in_cents.unwrap_or(0) as u32),
                ticket_types: vec![],
                is_external: r.is_external,
                external_url: r.external_url,
                override_status: r.override_status,
                localized_times,
                event_type: r.event_type,
                slug: r.slug,
                eligible_for_deletion: Some(r.eligible_for_deletion),
                extra_admin_data: r.extra_admin_data,
            };

            for ticket_type in ticket_types.iter().filter(|tt| {
                tt.event_id == event_id
                    && !(tt.status == TicketTypeStatus::Cancelled
                        && tt.sold_held.unwrap_or(0) + tt.sold_unreserved.unwrap_or(0) == 0)
            }) {
                let mut ticket_type = ticket_type.clone();
                ticket_type.sales_total_in_cents =
                    Some(ticket_type.sales_total_in_cents.unwrap_or(0));
                result.total_tickets += ticket_type.total as u32;
                result.sold_unreserved = Some(
                    result.sold_unreserved.unwrap_or(0)
                        + ticket_type.sold_unreserved.unwrap_or(0) as u32,
                );
                result.sold_held =
                    Some(result.sold_held.unwrap_or(0) + ticket_type.sold_held.unwrap_or(0) as u32);
                result.tickets_open += ticket_type.open as u32;
                result.tickets_held += ticket_type.held as u32;
                result.tickets_redeemed += ticket_type.redeemed as u32;
                result.ticket_types.push(ticket_type);
            }

            results.push(result);
        }

        Ok(results)
    }

    pub fn get_sales_by_date_range(
        &self,
        start_utc: NaiveDate,
        end_utc: NaiveDate,
        conn: &PgConnection,
    ) -> Result<Vec<DayStats>, DatabaseError> {
        jlog!(
            Level::Debug,
            &format!("Fetching sales data by dates {} and {}", start_utc, end_utc)
        );

        if start_utc > end_utc {
            return Err(DatabaseError::new(
                ErrorCode::InternalError,
                Some("Sales data start date must come before end date".to_string()),
            ));
        }

        //Gets the face value
        let query = r#"
            SELECT CAST(o.paid_at AT TIME ZONE 'utc' AT TIME ZONE COALESCE(v.timezone, o2.timezone, 'utc') AS DATE)                          AS date,
                   CAST(COALESCE(SUM(oi.unit_price_in_cents * (oi.quantity - oi.refunded_quantity)), 0) AS BIGINT)                           AS sales,
                   CAST(COALESCE(SUM(CASE WHEN oi.item_type = 'Tickets' THEN (oi.quantity - oi.refunded_quantity) ELSE 0 END), 0) AS BIGINT) AS ticket_count
            FROM order_items oi
                     INNER JOIN orders o ON oi.order_id = o.id
                     INNER JOIN events e ON oi.event_id = e.id
                     LEFT JOIN venues v ON e.venue_id = v.id
                     INNER JOIN organizations o2 ON e.organization_id = o2.id
            WHERE oi.event_id = $1
              AND oi.item_type = 'Tickets'
              AND o.status = 'Paid'
              AND o.paid_at AT TIME ZONE 'utc' AT TIME ZONE COALESCE(v.timezone, o2.timezone, 'utc') >= $2
              AND o.paid_at AT TIME ZONE 'utc' AT TIME ZONE COALESCE(v.timezone, o2.timezone, 'utc') <= $3
            GROUP BY CAST(o.paid_at AT TIME ZONE 'utc' AT TIME ZONE COALESCE(v.timezone, o2.timezone, 'utc') AS DATE)
            ORDER BY CAST(o.paid_at AT TIME ZONE 'utc' AT TIME ZONE COALESCE(v.timezone, o2.timezone, 'utc') AS DATE) DESC;
                "#;

        #[derive(QueryableByName)]
        struct R {
            #[sql_type = "Date"]
            date: NaiveDate,
            #[sql_type = "Nullable<BigInt>"]
            sales: Option<i64>,
            #[sql_type = "Nullable<BigInt>"]
            ticket_count: Option<i64>,
        }

        let summary: Vec<R> = diesel::sql_query(query)
            .bind::<dUuid, _>(self.id)
            .bind::<Timestamp, NaiveDateTime>(start_utc.and_hms(0, 0, 0))
            .bind::<Timestamp, NaiveDateTime>(end_utc.and_hms(23, 59, 59))
            .get_results(conn)
            .to_db_error(
                ErrorCode::QueryError,
                "Could not load calculate sales for event",
            )?;

        let mut map = HashMap::<NaiveDate, R>::new();
        for s in summary {
            map.insert(s.date, s);
        }

        let mut result = vec![];
        let n = end_utc.signed_duration_since(start_utc).num_days();
        for s in 0..=n {
            let date = start_utc + Duration::days(s);

            match map.get(&date) {
                Some(map_data) => result.push(DayStats {
                    date: map_data.date,
                    revenue_in_cents: map_data.sales.unwrap_or(0),
                    ticket_sales: map_data.ticket_count.unwrap_or(0),
                }),
                None => result.push(DayStats {
                    date,
                    revenue_in_cents: 0,
                    ticket_sales: 0,
                }),
            }
        }

        Ok(result)
    }

    pub fn guest_list_tickets(
        event_id: Option<Uuid>,
        ticket_id: Option<Uuid>,
        query_string: Option<String>,
        changes_since: &Option<NaiveDateTime>,
        paging: Option<&Paging>,
        conn: &PgConnection,
    ) -> Result<(Vec<RedeemableTicket>, i64), DatabaseError> {
        use schema::*;
        let paging = match paging {
            Some(p) => p.clone(),
            None => Paging {
                page: 0,
                limit: 99999999,
                sort: "".to_string(),
                dir: SortingDir::Asc,
                total: 0,
                tags: HashMap::new(),
            },
        };

        let mut query = ticket_instances::table
            .inner_join(assets::table.inner_join(ticket_types::table))
            .inner_join(order_items::table.inner_join(orders::table))
            .inner_join(
                wallets::table
                    .inner_join(users::table.on(wallets::user_id.eq(users::id.nullable())))
                    .on(wallets::id.eq(ticket_instances::wallet_id)),
            )
            .inner_join(events::table.on(events::id.eq(ticket_types::event_id)))
            .left_join(venues::table.on(venues::id.nullable().eq(events::venue_id.nullable())))
            .into_boxed();
        if let Some(event_id) = event_id {
            query = query.filter(ticket_types::event_id.nullable().eq(event_id))
        }
        if let Some(ticket_id) = ticket_id {
            query = query.filter(ticket_instances::id.nullable().eq(ticket_id))
        }
        if let Some(query_string) = query_string {
            let fuzzy_query_string: String = str::replace(&query_string.trim(), ",", "");
            let fuzzy_query_string = fuzzy_query_string
                .split_whitespace()
                .map(|w| w.split("").collect::<Vec<&str>>().join("%"))
                .collect::<Vec<String>>()
                .join("%");
            let id_query_string = format!("%{}%", query_string.to_lowercase());

            query = query

                .filter(sql("users.email ILIKE ").bind::<Text, _>(fuzzy_query_string.clone())
                    .or(sql("users.phone ILIKE ").bind::<Text, _>(fuzzy_query_string.clone()))
                    .or(sql("CONCAT(COALESCE(ticket_instances.first_name_override, users.first_name), ' ', COALESCE(ticket_instances.last_name_override, users.last_name)) ILIKE ").bind::<Text, _>(fuzzy_query_string.clone()))
                    .or(sql("CONCAT(COALESCE(ticket_instances.last_name_override, users.last_name), ' ', COALESCE(ticket_instances.first_name_override, users.first_name)) ILIKE ").bind::<Text, _>(fuzzy_query_string.clone()))
                    .or(sql("ticket_instances.id::TEXT LIKE ").bind::<Text, _>(id_query_string.clone()))
                    .or(sql("order_items.order_id::TEXT LIKE ").bind::<Text, _>(id_query_string.clone())));
        }

        if let Some(changes_since) = changes_since {
            query = query.filter(ticket_instances::updated_at.nullable().ge(changes_since))
        }

        let results = query.order_by(users::last_name.asc())
            .then_order_by(ticket_instances::id)
            .select((
                sql::<dUuid>("ticket_instances.id AS id")
                , sql::<Text>("ticket_types.name AS ticket_type")
                , sql::<Nullable<dUuid>>("users.id AS user_id")
                , sql::<dUuid>("order_items.order_id AS order_id")
                , sql::<dUuid>("order_items.id AS order_item_id")
                , sql::<BigInt>("cast(order_items.unit_price_in_cents + coalesce((SELECT SUM(unit_price_in_cents) FROM order_items WHERE parent_id = ticket_instances.order_item_id), 0) AS BIGINT) AS price_in_cents")
                , sql::<Nullable<Text>>("COALESCE(ticket_instances.first_name_override, users.first_name) AS first_name")
                , sql::<Nullable<Text>>("COALESCE(ticket_instances.last_name_override, users.last_name) AS last_name")
                , sql::<Nullable<Text>>("users.email AS email")
                , sql::<Nullable<Text>>("users.phone AS phone")
                , sql::<Nullable<Text>>("CASE WHEN events.redeem_date IS NULL OR NOW() >= events.redeem_date OR NOW() >= events.event_start - INTERVAL '1 day 1 minute' THEN ticket_instances.redeem_key ELSE NULL END AS redeem_key")
                , sql::<Nullable<Timestamp>>("events.redeem_date AS redeem_date")
                , sql::<Text>("ticket_instances.status AS status")
                , sql::<dUuid>("events.id AS event_id")
                , sql::<Text>("events.name AS event_name")
                , sql::<Nullable<Timestamp>>("events.door_time AS door_time")
                , sql::<Nullable<Timestamp>>("events.event_start AS event_start")
                , sql::<Nullable<dUuid>>("venues.id AS venue_id")
                , sql::<Nullable<Text>>("venues.name AS venue_name")
                , sql::<Timestamp>("ticket_instances.updated_at AS updated_at")
                , sql::<Nullable<Text>>("CASE WHEN ticket_instances.redeemed_by_user_id IS NOT NULL THEN (SELECT CONCAT(u2.first_name, ' ', u2.last_name) FROM users u2 WHERE u2.id = ticket_instances.redeemed_by_user_id) ELSE NULL END  AS redeemed_by")
                , sql::<Nullable<Timestamp>>("ticket_instances.redeemed_at AS redeemed_at")
            ))
            .paginate(paging.page as i64)
            .per_page(paging.limit as i64)
            .load_and_count_pages(conn);

        DatabaseError::wrap(
            ErrorCode::QueryError,
            "Unable to load all redeemable tickets",
            results,
        )
    }

    pub fn guest_list(
        &self,
        query: Option<String>,
        changes_since: &Option<NaiveDateTime>,
        paging: Option<&Paging>,
        conn: &PgConnection,
    ) -> Result<(Vec<GuestListItem>, i64), DatabaseError> {
        let tickets_and_counts =
            Event::guest_list_tickets(Some(self.id), None, query, changes_since, paging, conn)?;
        let (tickets, total) = tickets_and_counts;

        let mut guests: Vec<GuestListItem> = Vec::new();

        #[derive(Debug, QueryableByName, Queryable)]
        struct OrderPaymentProviders {
            #[sql_type = "Uuid"]
            id: Uuid,
            #[sql_type = "Nullable<Text>"]
            provider: Option<String>,
        }

        let order_payment_providers: Vec<OrderPaymentProviders> = orders::table
            .left_join(payments::table.on(orders::id.eq(payments::order_id)))
            .filter(orders::id.eq_any(tickets.iter().map(|t| t.order_id).collect::<Vec<Uuid>>()))
            .select((orders::id, payments::provider.nullable()))
            .load(conn)
            .to_db_error(ErrorCode::QueryError, "Could not load payment providers")?;

        let pending_transfers: Vec<PendingTransfer> = transfer_tickets::table
            .inner_join(transfers::table.on(transfers::id.eq(transfer_tickets::transfer_id)))
            .filter(
                transfers::status.eq(TransferStatus::Pending).and(
                    transfer_tickets::ticket_instance_id
                        .eq_any(tickets.iter().map(|t| t.id).collect::<Vec<Uuid>>()),
                ),
            )
            .select((
                transfer_tickets::ticket_instance_id.nullable(),
                transfer_tickets::transfer_id.nullable(),
                transfers::transfer_key.nullable(),
                transfers::status.nullable(),
                transfers::transfer_message_type,
                transfers::transfer_address,
            ))
            .load(conn)
            .to_db_error(ErrorCode::QueryError, "Could not load pending transfers")?;

        let mut pending_transfers_by_ticket: HashMap<Uuid, PendingTransfer> = HashMap::new();
        for pending_transfer in pending_transfers {
            if pending_transfer.ticket_instance_id.is_some() {
                pending_transfers_by_ticket.insert(
                    pending_transfer.ticket_instance_id.unwrap(),
                    pending_transfer,
                );
            }
        }

        for t in &tickets {
            let mut providers: Vec<String> = Vec::new();
            for order_payment_provider in &order_payment_providers {
                if order_payment_provider.id == t.order_id {
                    if let Some(ref p) = order_payment_provider.provider {
                        providers.push(p.clone());
                    }
                }
            }
            let pending_transfer = pending_transfers_by_ticket.get(&t.id).map(|x| x.clone());
            guests.push(GuestListItem {
                ticket: t.clone(),
                providers,
                pending_transfer,
            })
        }

        Ok((guests, total))
    }

    pub fn dates_by_past_or_upcoming(
        start_time: Option<NaiveDateTime>,
        end_time: Option<NaiveDateTime>,
        past_or_upcoming: PastOrUpcoming,
    ) -> (NaiveDateTime, NaiveDateTime) {
        let now = Utc::now().naive_utc();
        let beginning_of_time = NaiveDate::from_ymd(1900, 1, 1).and_hms(12, 0, 0);
        let end_of_time = NaiveDate::from_ymd(3100, 1, 1).and_hms(12, 0, 0);
        if past_or_upcoming == PastOrUpcoming::Upcoming {
            (
                NaiveDateTime::max(start_time.unwrap_or(now), now),
                NaiveDateTime::min(end_time.unwrap_or(end_of_time), end_of_time),
            )
        } else {
            (
                NaiveDateTime::max(start_time.unwrap_or(beginning_of_time), beginning_of_time),
                NaiveDateTime::min(end_time.unwrap_or(now), now),
            )
        }
    }

    pub fn search(
        query_filter: Option<String>,
        region_id: Option<Uuid>,
        organization_id: Option<Uuid>,
        venue_id: Option<Uuid>,
        genres: Option<Vec<String>>,
        start_time: Option<NaiveDateTime>,
        end_time: Option<NaiveDateTime>,
        status_filter: Option<Vec<EventStatus>>,
        sort_field: EventSearchSortField,
        sort_direction: SortingDir,
        user: Option<User>,
        past_or_upcoming: PastOrUpcoming,
        event_type: Option<EventTypes>,
        paging: &Paging,
        country_service: &CountryLookup,
        conn: &PgConnection,
    ) -> Result<(Vec<Event>, i64), DatabaseError> {
        let sort_column = match sort_field {
            EventSearchSortField::Name => "name",
            EventSearchSortField::EventStart => "event_start",
        };
        let (start_time, end_time) =
            Event::dates_by_past_or_upcoming(start_time, end_time, past_or_upcoming);

        let query_like = match query_filter.clone() {
            Some(n) => format!("%{}%", text::escape_control_chars(&n.trim())),
            None => "%".to_string(),
        };
        let query_escaped = query_filter.map(|q| {
            text::escape_control_chars(&q)
                .split(|c| c == ' ')
                .filter(|s| !s.is_empty())
                .collect::<Vec<&str>>()
                .join(" ")
        });

        let mut venue_location_searches: Vec<(Option<String>, Option<StateDatum>, CountryDatum)> =
            Vec::new();
        if let Some(query_escaped) = query_escaped.clone() {
            if let Ok(data) = country_service.parse_city_state_country(&query_escaped.clone()) {
                for (city, state, country) in data {
                    if let Some(country) = country {
                        venue_location_searches.push((city, state, country));
                    }
                }
            }
            let mut default_country: Option<CountryDatum> = None;
            if let Some(user) = &user {
                if let Some(country) = user.country(conn)?.clone() {
                    default_country = country_service.find(&country);
                }
            }
            // Default to US for country state search
            default_country = default_country.or_else(|| country_service.find("US"));
            if let Some(default_country) = default_country {
                if let Ok(data) = default_country.parse_city_state(&query_escaped.clone()) {
                    for (city, state) in data {
                        venue_location_searches.push((city, state, default_country.clone()));
                    }
                }
            }
            venue_location_searches.sort();
            venue_location_searches.dedup();
        }

        let mut query = events::table
            .left_join(venues::table.on(events::venue_id.eq(venues::id.nullable())))
            .inner_join(organizations::table.on(organizations::id.eq(events::organization_id)))
            .left_join(
                organization_users::table
                    .on(organization_users::organization_id.eq(organizations::id)),
            )
            .left_join(
                event_artists::table
                    .inner_join(
                        artists::table.on(event_artists::artist_id
                            .eq(artists::id)
                            .and(artists::name.ilike(query_like.clone()))),
                    )
                    .on(events::id.eq(event_artists::event_id)),
            )
            .filter(
                events::name
                    .ilike(query_like.clone())
                    .or(venues::name.ilike(query_like.clone()))
                    .or(venues::city.ilike(query_escaped.clone().unwrap_or("%".to_string())))
                    .or(venues::state.ilike(query_escaped.clone().unwrap_or("%".to_string())))
                    .or(venues::country.ilike(query_escaped.clone().unwrap_or("%".to_string())))
                    .or(artists::id.is_not_null()),
            )
            .into_boxed();

        if venue_location_searches.len() > 0 {
            for (city, state, country) in venue_location_searches {
                query =
                    query
                        .or_filter(
                            venues::city
                                .ilike(city.clone().unwrap_or("%".to_string()))
                                .and(venues::state.ilike(
                                    state.clone().map(|s| s.name).unwrap_or("%".to_string()),
                                ))
                                .and(venues::country.ilike(country.clone().name)),
                        )
                        .or_filter(
                            venues::city
                                .ilike(city.clone().unwrap_or("%".to_string()))
                                .and(
                                    venues::state.ilike(
                                        state
                                            .clone()
                                            .map(|s| s.code.unwrap_or("%".to_string()))
                                            .unwrap_or("%".to_string()),
                                    ),
                                )
                                .and(venues::country.ilike(country.clone().name)),
                        )
                        .or_filter(
                            venues::city
                                .ilike(city.clone().unwrap_or("%".to_string()))
                                .and(
                                    venues::state.ilike(
                                        state
                                            .clone()
                                            .map(|s| s.code.unwrap_or("%".to_string()))
                                            .unwrap_or("%".to_string()),
                                    ),
                                )
                                .and(venues::country.ilike(country.clone().code)),
                        )
                        .or_filter(
                            venues::city
                                .ilike(city.clone().unwrap_or("%".to_string()))
                                .and(venues::state.ilike(
                                    state.clone().map(|s| s.name).unwrap_or("%".to_string()),
                                ))
                                .and(venues::country.ilike(country.clone().code)),
                        );
            }
        }

        if let Some(event_type) = event_type {
            query = query.filter(events::event_type.eq(event_type))
        }

        match user {
            Some(user) => {
                // Admin results include all drafts across organizations
                if !user.get_global_scopes().contains(&Scopes::OrgAdmin) {
                    query = query
                        .filter(
                            events::status
                                .ne(EventStatus::Draft)
                                .or(organization_users::user_id.eq(user.id)),
                        )
                        .filter(
                            events::publish_date
                                .le(dsl::now.nullable())
                                .or(events::status.ne(EventStatus::Published))
                                .or(organization_users::user_id.eq(user.id)),
                        )
                }
            }
            None => {
                query = query.filter(events::status.ne(EventStatus::Draft)).filter(
                    events::publish_date
                        .le(dsl::now.nullable())
                        .or(events::status.ne(EventStatus::Published)),
                );
            }
        }

        query = query.filter(events::private_access_code.is_null()); //we dont ever want to show private events when searching

        if let Some(genres) = genres {
            let genres = Genre::format_names(&genres);
            query = query.filter(
                sql("(")
                    .bind::<Integer, _>(genres.len() as i32)
                    .sql(" = (select count(eg.genre_id) from event_genres eg join genres g on eg.genre_id = g.id where eg.event_id = events.id and g.name = ANY(")
                    .bind::<Array<Text>, _>(genres)
                    .sql(")))")
            );
        }

        if let Some(organization_id) = organization_id {
            query = query.filter(events::organization_id.eq(organization_id));
        }
        if let Some(venue_id) = venue_id {
            query = query.filter(events::venue_id.eq(venue_id));
        }

        if let Some(statuses) = status_filter {
            query = query.filter(events::status.eq_any(statuses));
        }

        if let Some(region_id) = region_id {
            query = query.filter(venues::region_id.eq(region_id));
        }

        let result = query
            .filter(events::event_end.ge(start_time))
            .filter(events::event_end.le(end_time))
            .filter(events::deleted_at.is_null())
            .select(events::all_columns)
            .distinct()
            .order_by(sql::<()>(&format!("{} {}", sort_column, sort_direction)))
            .then_order_by(events::name.asc())
            .paginate(paging.page as i64)
            .per_page(paging.limit as i64)
            .load_and_count_pages(conn);

        DatabaseError::wrap(ErrorCode::QueryError, "Unable to load all events", result)
    }

    pub fn add_artist(
        &self,
        current_user_id: Option<Uuid>,
        artist_id: Uuid,
        conn: &PgConnection,
    ) -> Result<(), DatabaseError> {
        EventArtist::create(self.id, artist_id, 0, None, 0, None)
            .commit(current_user_id, conn)
            .map(|_| ())
    }

    pub fn organization(&self, conn: &PgConnection) -> Result<Organization, DatabaseError> {
        Organization::find(self.organization_id, conn)
    }

    pub fn venue(&self, conn: &PgConnection) -> Result<Option<Venue>, DatabaseError> {
        match self.venue_id {
            Some(venue_id) => {
                let venue = Venue::find(venue_id, conn);
                match venue {
                    Ok(venue) => Ok(Some(venue)),
                    Err(e) => Err(e),
                }
            }
            None => Ok(None),
        }
    }
    pub fn checked_in_users(
        event_id: Uuid,
        conn: &PgConnection,
    ) -> Result<Vec<User>, DatabaseError> {
        use schema::*;
        ticket_instances::table
            .inner_join(assets::table.inner_join(ticket_types::table))
            .inner_join(
                wallets::table
                    .inner_join(users::table.on(wallets::user_id.eq(users::id.nullable())))
                    .on(wallets::id.eq(ticket_instances::wallet_id)),
            )
            .filter(ticket_instances::status.eq(TicketInstanceStatus::Redeemed))
            .filter(ticket_types::event_id.eq(event_id))
            .select(users::all_columns)
            .load(conn)
            .to_db_error(ErrorCode::QueryError, "Could not load checked in users")
    }

    pub fn add_ticket_type(
        &self,
        name: String,
        description: Option<String>,
        quantity: u32,
        start_date: Option<NaiveDateTime>,
        end_date: NaiveDateTime,
        wallet_id: Option<Uuid>,
        increment: Option<i32>,
        limit_per_person: i32,
        price_in_cents: i64,
        visibility: TicketTypeVisibility,
        parent_id: Option<Uuid>,
        additional_fee_in_cents: i64,
        current_user_id: Option<Uuid>,
        conn: &PgConnection,
    ) -> Result<TicketType, DatabaseError> {
        let asset_name = format!("{}.{}", self.name, &name);
        let ticket_type = TicketType::create(
            self.id,
            name,
            description,
            start_date,
            end_date,
            increment,
            limit_per_person,
            price_in_cents,
            visibility,
            parent_id,
            additional_fee_in_cents,
        )
        .commit(current_user_id, conn)?;
        let asset = Asset::create(ticket_type.id, asset_name).commit(conn)?;
        let wallet_id = match wallet_id {
            Some(w) => w,
            None => Wallet::find_default_for_organization(self.organization_id, conn)?.id,
        };

        TicketInstance::create_multiple(asset.id, 0, quantity, wallet_id, conn)?;
        Ok(ticket_type)
    }

    pub fn ticket_types(
        &self,
        filter_access_tokens: bool,
        redemption_code: Option<String>,
        conn: &PgConnection,
    ) -> Result<Vec<TicketType>, DatabaseError> {
        TicketType::find_by_event_id(self.id, filter_access_tokens, redemption_code, conn)
    }

    pub fn issuer_wallet(&self, conn: &PgConnection) -> Result<Wallet, DatabaseError> {
        Wallet::find_default_for_organization(self.organization_id, conn)
    }

    pub fn artists(&self, conn: &PgConnection) -> Result<Vec<DisplayEventArtist>, DatabaseError> {
        EventArtist::find_all_from_event(self.id, conn)
    }

    pub fn search_fans(
        &self,
        query: Option<String>,
        page: u32,
        limit: u32,
        sort_field: FanSortField,
        sort_direction: SortingDir,
        conn: &PgConnection,
    ) -> Result<Payload<DisplayFan>, DatabaseError> {
        let organization = self.organization(conn)?;
        organization.search_fans(
            Some(self.id),
            query,
            page,
            limit,
            sort_field,
            sort_direction,
            conn,
        )
    }

    pub fn activity_summary(
        &self,
        user_id: Uuid,
        activity_type: Option<ActivityType>,
        conn: &PgConnection,
    ) -> Result<ActivitySummary, DatabaseError> {
        Ok(ActivitySummary {
            activity_items: ActivityItem::load_for_event(self.id, user_id, activity_type, conn)?,
            event: self.for_display(conn)?,
        })
    }

    pub fn for_display(&self, conn: &PgConnection) -> Result<DisplayEvent, DatabaseError> {
        let venue = self.venue(conn)?;
        let display_venue: Option<DisplayVenue> =
            venue.clone().and_then(|venue| Some(venue.into()));
        let artists = self.artists(conn)?;
        let genres = self.genres(conn)?;

        let localized_times = self.get_all_localized_time_strings(venue.as_ref());
        let (min_ticket_price, max_ticket_price) =
            self.current_ticket_pricing_range(false, conn)?;
        Ok(DisplayEvent {
            id: self.id,
            name: self.name.clone(),
            event_start: self.event_start,
            door_time: self.door_time,
            promo_image_url: self.promo_image_url.clone(),
            cover_image_url: self.cover_image_url.clone(),
            additional_info: self.additional_info.clone(),
            top_line_info: self.top_line_info.clone(),
            artists,
            genres,
            venue: display_venue,
            max_ticket_price,
            min_ticket_price,
            video_url: self.video_url.clone(),
            is_external: self.is_external,
            external_url: self.external_url.clone(),
            override_status: self.override_status,
            localized_times,
            event_type: self.event_type,
            slug: self.slug.clone(),
        })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DisplayEvent {
    pub id: Uuid,
    pub name: String,
    pub event_start: Option<NaiveDateTime>,
    pub door_time: Option<NaiveDateTime>,
    pub promo_image_url: Option<String>,
    pub cover_image_url: Option<String>,
    pub additional_info: Option<String>,
    pub top_line_info: Option<String>,
    pub artists: Vec<DisplayEventArtist>,
    pub venue: Option<DisplayVenue>,
    pub min_ticket_price: Option<i64>,
    pub max_ticket_price: Option<i64>,
    pub video_url: Option<String>,
    pub is_external: bool,
    pub external_url: Option<String>,
    pub override_status: Option<EventOverrideStatus>,
    pub localized_times: EventLocalizedTimeStrings,
    pub event_type: EventTypes,
    pub genres: Vec<String>,
    pub slug: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct EventSummaryResult {
    pub id: Uuid,
    pub name: String,
    pub organization_id: Uuid,
    pub venue: Option<VenueInfo>,
    pub created_at: NaiveDateTime,
    pub event_start: Option<NaiveDateTime>,
    pub door_time: Option<NaiveDateTime>,
    pub status: EventStatus,
    pub promo_image_url: Option<String>,
    pub additional_info: Option<String>,
    pub top_line_info: Option<String>,
    pub age_limit: Option<String>,
    pub cancelled_at: Option<NaiveDateTime>,
    pub min_ticket_price: Option<u32>,
    pub max_ticket_price: Option<u32>,
    pub publish_date: Option<NaiveDateTime>,
    pub on_sale: Option<NaiveDateTime>,
    pub total_tickets: u32,
    pub sold_unreserved: Option<u32>,
    pub sold_held: Option<u32>,
    pub tickets_open: u32,
    pub tickets_held: u32,
    pub tickets_redeemed: u32,
    pub sales_total_in_cents: Option<u32>,
    pub ticket_types: Vec<EventSummaryResultTicketType>,
    pub is_external: bool,
    pub external_url: Option<String>,
    pub override_status: Option<EventOverrideStatus>,
    pub localized_times: EventLocalizedTimeStrings,
    pub event_type: EventTypes,
    pub slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eligible_for_deletion: Option<bool>,
    pub extra_admin_data: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, QueryableByName)]
pub struct EventSummaryResultTicketType {
    #[sql_type = "dUuid"]
    pub(crate) event_id: Uuid,
    #[sql_type = "Text"]
    pub name: String,
    #[sql_type = "Text"]
    pub status: TicketTypeStatus,
    #[sql_type = "BigInt"]
    pub min_price: i64,
    #[sql_type = "BigInt"]
    pub max_price: i64,
    #[sql_type = "BigInt"]
    pub total: i64,
    #[sql_type = "Nullable<BigInt>"]
    pub sold_unreserved: Option<i64>,
    #[sql_type = "Nullable<BigInt>"]
    pub sold_held: Option<i64>,
    #[sql_type = "BigInt"]
    pub open: i64,
    #[sql_type = "BigInt"]
    pub held: i64,
    #[sql_type = "BigInt"]
    pub redeemed: i64,
    #[sql_type = "Nullable<BigInt>"]
    pub sales_total_in_cents: Option<i64>,
}

#[derive(Debug, Deserialize, PartialEq, Serialize)]
pub struct DayStats {
    pub date: NaiveDate,
    pub revenue_in_cents: i64,
    pub ticket_sales: i64,
}

#[derive(Debug, QueryableByName, Queryable, Serialize, Clone, Default)]
pub struct PendingTransfer {
    #[sql_type = "Nullable<dUuid>"]
    pub ticket_instance_id: Option<Uuid>,
    #[sql_type = "Nullable<dUuid>"]
    pub transfer_id: Option<Uuid>,
    #[sql_type = "Nullable<dUuid>"]
    pub transfer_key: Option<Uuid>,
    #[sql_type = "Nullable<Text>"]
    pub transfer_status: Option<TransferStatus>,
    #[sql_type = "Nullable<Text>"]
    pub transfer_message_type: Option<TransferMessageType>,
    #[sql_type = "Nullable<Text>"]
    pub transfer_address: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct GuestListItem {
    pub ticket: RedeemableTicket,
    pub providers: Vec<String>,
    pub pending_transfer: Option<PendingTransfer>,
}
