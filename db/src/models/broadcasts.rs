use chrono::prelude::*;
use diesel;
use diesel::expression::dsl;
use diesel::prelude::*;
use models::*;
use schema::broadcasts;
use utils::errors::ConvertToDatabaseError;
use utils::errors::DatabaseError;
use utils::errors::ErrorCode;
use uuid::Uuid;
use validator::*;
use validators::{self, *};

#[derive(Default, Insertable, Serialize, Deserialize, PartialEq, Debug)]
#[table_name = "broadcasts"]
pub struct NewBroadcast {
    pub event_id: Uuid,
    pub notification_type: BroadcastType,
    pub channel: BroadcastChannel,
    pub name: String,
    pub message: Option<String>,
    pub send_at: Option<NaiveDateTime>,
    pub status: BroadcastStatus,
    pub progress: i32,
}

#[derive(Queryable, Identifiable, Insertable, Serialize, Deserialize, PartialEq, Debug)]
#[table_name = "broadcasts"]
pub struct Broadcast {
    pub id: Uuid,
    pub event_id: Uuid,
    pub notification_type: BroadcastType,
    pub channel: BroadcastChannel,
    pub name: String,
    pub message: Option<String>,
    pub send_at: Option<NaiveDateTime>,
    pub status: BroadcastStatus,
    pub progress: i32,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(AsChangeset, Default, Deserialize)]
#[table_name = "broadcasts"]
pub struct BroadcastEditableAttributes {
    #[serde(default, deserialize_with = "deserialize_unless_blank")]
    pub notification_type: Option<BroadcastType>,
    #[serde(default, deserialize_with = "deserialize_unless_blank")]
    pub channel: Option<BroadcastChannel>,
    #[serde(default, deserialize_with = "deserialize_unless_blank")]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option_deserialize_unless_blank")]
    pub message: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option_deserialize_unless_blank")]
    pub send_at: Option<Option<NaiveDateTime>>,
    #[serde(default, deserialize_with = "deserialize_unless_blank")]
    pub status: Option<BroadcastStatus>,
}

impl Broadcast {
    pub fn create(
        event_id: Uuid,
        notification_type: BroadcastType,
        channel: BroadcastChannel,
        name: String,
        message: Option<String>,
        send_at: Option<NaiveDateTime>,
        status: Option<BroadcastStatus>,
    ) -> NewBroadcast {
        NewBroadcast {
            event_id,
            notification_type,
            channel,
            name,
            message,
            send_at,
            status: status.unwrap_or(BroadcastStatus::Pending),
            progress: 0,
        }
    }

    pub fn find(id: Uuid, connection: &PgConnection) -> Result<Broadcast, DatabaseError> {
        broadcasts::table
            .filter(broadcasts::id.eq(id))
            .get_result(connection)
            .to_db_error(ErrorCode::QueryError, "Unable to load push notification")
    }

    pub fn find_by_event_id(
        event_id: Uuid,
        page: u32,
        limit: u32,
        connection: &PgConnection,
    ) -> Result<Payload<Broadcast>, DatabaseError> {
        let total: i64 = broadcasts::table
            .filter(broadcasts::event_id.eq(event_id))
            .count()
            .first(connection)
            .to_db_error(
                ErrorCode::QueryError,
                "Could not get total push notifications for event",
            )?;

        let notifications = broadcasts::table
            .filter(broadcasts::event_id.eq(event_id))
            .limit(limit as i64)
            .offset((limit * page) as i64)
            .select(broadcasts::all_columns)
            .order_by(broadcasts::send_at.asc())
            .load(connection)
            .to_db_error(
                ErrorCode::QueryError,
                "Unable to load push notification by event",
            )?;

        let mut paging = Paging::new(page, limit);
        paging.total = total as u64;
        Ok(Payload {
            paging,
            data: notifications,
        })
    }

    pub fn cancel(&self, connection: &PgConnection) -> Result<Broadcast, DatabaseError> {
        let attributes: BroadcastEditableAttributes = BroadcastEditableAttributes {
            notification_type: None,
            channel: None,
            name: None,
            message: None,
            send_at: None,
            status: Some(BroadcastStatus::Cancelled),
        };

        self.update(attributes, connection)
    }

    pub fn update(
        &self,
        attributes: BroadcastEditableAttributes,
        connection: &PgConnection,
    ) -> Result<Broadcast, DatabaseError> {
        match self.status {
            BroadcastStatus::Cancelled => Err(DatabaseError::new(
                ErrorCode::UpdateError,
                Some("This broadcast has been cancelled, it cannot be modified.".to_string()),
            )),
            _ => {
                self.validate_record(&attributes, connection)?;
                DatabaseError::wrap(
                    ErrorCode::UpdateError,
                    "Could not update broadcast",
                    diesel::update(self)
                        .set((attributes, broadcasts::updated_at.eq(dsl::now)))
                        .get_result(connection),
                )
            }
        }
    }

    pub fn set_in_progress(self, connection: &PgConnection) -> Result<Broadcast, DatabaseError> {
        let attributes = BroadcastEditableAttributes {
            status: Some(BroadcastStatus::InProgress),
            ..Default::default()
        };

        self.update(attributes, connection)
    }

    pub fn validate_record(
        &self,
        attributes: &BroadcastEditableAttributes,
        conn: &PgConnection,
    ) -> Result<(), DatabaseError> {
        let validation_errors = validators::append_validation_error(
            Ok(()),
            "message",
            Broadcast::custom_type_has_message(
                attributes
                    .notification_type
                    .clone()
                    .unwrap_or(self.notification_type.clone()),
                attributes.message.clone().unwrap_or(self.message.clone()),
                conn,
            )?,
        );
        Ok(validation_errors?)
    }

    fn custom_type_has_message(
        notification_type: BroadcastType,
        message: Option<String>,
        _connection: &PgConnection,
    ) -> Result<Result<(), ValidationError>, DatabaseError> {
        match notification_type {
            BroadcastType::LastCall => return Ok(Ok(())),
            BroadcastType::Custom => {
                if let Some(message) = message {
                    if !message.is_empty() {
                        return Ok(Ok(()));
                    }
                }
                let validation_error = create_validation_error(
                    "custom_message_empty",
                    "Custom messages cannot be blank",
                );
                return Ok(Err(validation_error));
            }
        }
    }
}

impl NewBroadcast {
    pub fn commit(&self, connection: &PgConnection) -> Result<Broadcast, DatabaseError> {
        self.validate_record(connection)?;
        let result: Broadcast = DatabaseError::wrap(
            ErrorCode::InsertError,
            "Could not create new push notification",
            diesel::insert_into(broadcasts::table)
                .values(self)
                .get_result(connection),
        )?;

        let mut action = DomainAction::create(
            None,
            DomainActionTypes::BroadcastPushNotification,
            None,
            json!(BroadcastPushNotificationAction {
                event_id: self.event_id,
            }),
            Some(Tables::Broadcasts.to_string()),
            Some(result.id),
        );
        if let Some(send_at) = self.send_at {
            action.schedule_at(send_at);
        }

        action.commit(connection)?;

        Ok(result)
    }

    pub fn validate_record(&self, conn: &PgConnection) -> Result<(), DatabaseError> {
        let validation_errors = validators::append_validation_error(
            Ok(()),
            "message",
            Broadcast::custom_type_has_message(
                self.notification_type.clone(),
                self.message.clone(),
                conn,
            )?,
        );
        Ok(validation_errors?)
    }
}

#[derive(Serialize, Deserialize)]
pub struct BroadcastPushNotificationAction {
    pub event_id: Uuid,
}
