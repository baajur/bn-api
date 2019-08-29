use diesel::prelude::*;
use models::*;
use uuid::Uuid;

pub struct VenueBuilder<'a> {
    name: String,
    region_id: Option<Uuid>,
    organization_id: Option<Uuid>,
    is_private: bool,
    timezone: String,
    country: String,
    connection: &'a PgConnection,
}

impl<'a> VenueBuilder<'a> {
    pub fn new(connection: &PgConnection) -> VenueBuilder {
        VenueBuilder {
            connection,
            name: "Name".into(),
            region_id: None,
            is_private: false,
            organization_id: None,
            timezone: "America/Los_Angeles".into(),
            country: "US".into(),
        }
    }

    pub fn with_name(mut self, name: String) -> Self {
        self.name = name;
        self
    }

    pub fn with_country(mut self, country: String) -> Self {
        self.country = country;
        self
    }

    pub fn with_region(mut self, region: &Region) -> Self {
        self.region_id = Some(region.id.clone());
        self
    }

    pub fn make_private(mut self) -> Self {
        self.is_private = true;
        self
    }

    pub fn with_organization(mut self, organization: &Organization) -> Self {
        self.organization_id = Some(organization.id.clone());
        self
    }

    pub fn with_timezone(mut self, timezone: String) -> Self {
        self.timezone = timezone;
        self
    }

    pub fn finish(self) -> Venue {
        let mut venue = Venue::create(
            &self.name,
            self.region_id,
            self.organization_id,
            self.timezone,
        );
        venue.country = self.country;

        let venue = venue.commit(self.connection).unwrap();
        venue.set_privacy(self.is_private, self.connection).unwrap()
    }
}
