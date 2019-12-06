use cache::RedisCacheConnection;
use config::Config;
use db::{CacheDatabase, ConnectionType};
use db::{Connection, ReadonlyConnection};
use diesel::r2d2::{self, ConnectionManager};
use diesel::PgConnection;
use r2d2::Error as R2D2Error;

type R2D2Pool = r2d2::Pool<ConnectionManager<PgConnection>>;

pub struct Database {
    connection_pool: R2D2Pool,
    pub cache_database: CacheDatabase,
}

impl Database {
    pub fn from_config(config: &Config) -> Database {
        Database {
            connection_pool: create_connection_pool(&config, config.database_url.clone()),
            cache_database: CacheDatabase {
                inner: RedisCacheConnection::create_connection_pool(&config.redis_connection_string).ok(),
            },
        }
    }

    pub fn readonly_from_config(config: &Config) -> Database {
        Database {
            connection_pool: create_connection_pool(&config, config.readonly_database_url.clone()),
            cache_database: CacheDatabase {
                inner: RedisCacheConnection::create_connection_pool(&config.redis_connection_string).ok(),
            },
        }
    }

    pub fn get_connection(&self) -> Result<Connection, R2D2Error> {
        let conn = self.connection_pool.get()?;
        Ok(ConnectionType::R2D2(conn).into())
    }

    pub fn get_ro_connection(&self) -> Result<ReadonlyConnection, R2D2Error> {
        let conn = self.connection_pool.get()?;
        Ok(ConnectionType::R2D2(conn).into())
    }
}

impl Clone for Database {
    fn clone(&self) -> Self {
        Database {
            connection_pool: self.connection_pool.clone(),
            cache_database: CacheDatabase {
                inner: self.cache_database.inner.clone(),
            },
        }
    }
}

fn create_connection_pool(config: &Config, database_url: String) -> R2D2Pool {
    let r2d2_config = r2d2::Pool::builder()
        .min_idle(Some(config.connection_pool.min))
        .max_size(config.connection_pool.max);

    let connection_manager = ConnectionManager::new(database_url);

    r2d2_config
        .build(connection_manager)
        .expect("Failed to create connection pool.")
}
