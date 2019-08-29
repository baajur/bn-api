use std::error::Error;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::Duration;
use std::{cmp, thread};

use log::Level::*;

use bigneon_db::prelude::*;
use config::Config;
use db::*;
use domain_events::errors::DomainActionError;
use domain_events::routing::{DomainActionExecutor, DomainActionRouter};
use logging::*;
use tokio::prelude::*;
use tokio::runtime::current_thread;
use tokio::runtime::Runtime;
use tokio::timer::Timeout;

pub struct DomainActionMonitor {
    config: Config,
    database: Database,
    worker_threads: Vec<(Sender<()>, JoinHandle<Result<(), DomainActionError>>)>,
    interval: u64,
}

impl DomainActionMonitor {
    pub fn new(conf: Config, database: Database, poll_period_in_secs: u64) -> DomainActionMonitor {
        DomainActionMonitor {
            config: conf,
            database,
            worker_threads: vec![],
            interval: poll_period_in_secs,
        }
    }

    pub fn run_til_empty(&self) -> Result<(), DomainActionError> {
        let router = DomainActionMonitor::create_router(&self.config);

        loop {
            let mut num_processed = 0;

            let futures = DomainActionMonitor::find_actions(
                &self.database,
                &router,
                cmp::max(1, self.config.connection_pool.max / 2) as usize,
            )?;

            let mut runtime = current_thread::Runtime::new().unwrap();

            for f in futures {
                let timeout = Timeout::new(f.0.execute(f.1, f.2), Duration::from_secs(55));

                runtime.block_on(timeout.or_else(|err| {
                    jlog! {Error,"bigneon::domain_actions", "Action: failed", {"error": err.to_string()}};
                    Err(())
                }))
                .unwrap();
                num_processed += 1;
            }

            if num_processed == 0 {
                break;
            }
        }
        Ok(())
    }

    fn find_and_publish_events(
        config: &Config,
        database: &Database,
    ) -> Result<usize, DomainActionError> {
        let connection = database.get_connection()?;
        let unpublished_domain_events_by_publisher =
            DomainEventPublisher::find_with_unpublished_domain_events(10, connection.get())?;

        let domain_events_to_publish = unpublished_domain_events_by_publisher
            .iter()
            .map(|(_, v)| v.len())
            .sum();
        if domain_events_to_publish > 0 {
            jlog!(
                Debug,
                "bigneon::domain_actions",
                "Found domain events to publish",
                { "count": unpublished_domain_events_by_publisher.len() }
            );
            for (publisher, domain_events) in unpublished_domain_events_by_publisher {
                for domain_event in domain_events {
                    publisher.publish(domain_event, &config.front_end_url, connection.get())?;
                }
            }
        }
        Ok(domain_events_to_publish)
    }

    pub fn publish_events_to_actions(
        config: Config,
        database: Database,
        interval: u64,
        rx: Receiver<()>,
    ) -> Result<(), DomainActionError> {
        loop {
            if rx.try_recv().is_ok() {
                jlog!(
                    Info,
                    "bigneon::domain_actions",
                    "Stopping events processor",
                    {}
                );
                break;
            }

            // Domain Monitor main loop
            DomainActionMonitor::find_and_publish_events(&config, &database)?;

            // Sleep regardless if we found results to reduce rate of webhook processing
            thread::sleep(Duration::from_secs(interval));
        }
        Ok(())
    }

    fn create_router(conf: &Config) -> DomainActionRouter {
        let mut router = DomainActionRouter::new();

        router.set_up_executors(conf);
        router
    }

    fn find_actions<'a>(
        database: &Database,
        router: &'a DomainActionRouter,
        limit: usize,
    ) -> Result<Vec<(&'a DomainActionExecutor, DomainAction, Connection)>, DomainActionError> {
        let connection = database.get_connection()?;

        let pending_actions = DomainAction::find_pending(None, connection.get())?;

        if pending_actions.len() == 0 {
            jlog!(
                Trace,
                "bigneon::domain_actions",
                "Found no actions to process",
                {}
            );
            return Ok(vec![]);
        }

        jlog!(
        Debug,
        "bigneon::domain_actions",
        "Found actions to process",
        { "action_count": pending_actions.len() }
        );

        let mut result = vec![];

        // //Process actions
        let len = pending_actions.len();
        for (index, action) in pending_actions.into_iter().enumerate() {
            if limit < index {
                break;
            }
            jlog! {Info, &format!("Pending Action: {}", action.domain_action_type), {"id":action.id, "domain_action_type": action.domain_action_type}};
            let connection = connection.get();
            let per_action_connection = match database.get_connection() {
                Ok(conn) => conn,
                Err(e) => {
                    // Assume connection pool is full
                    jlog!(
                    Info,
                    "bigneon::domain_actions",
                    "Hit connection pool maximum",
                    { "number_of_connections_used": index, "pending_actions": len, "connection_error": e.description() }
                    );

                    break;
                }
            };

            match action.set_busy(60, connection) {
                Ok(_) => {}
                Err(e) => match e.error_code {
                    ErrorCode::ConcurrencyError => {
                        jlog! {Debug, &format!("Action was already checked out to another process: {}", action.id)};
                        continue;
                    }
                    _ => return Err(e.into()),
                },
            };
            let command = router.get_executor_for(action.domain_action_type);
            if command.is_none() {
                action.set_errored(
                    "Not executor has been created for this action type",
                    &connection,
                )?;

                return Err(DomainActionError::Simple(format!(
                    "Could not find executor for this action type:{}",
                    action.domain_action_type
                )));
            }
            let command = command.unwrap();

            per_action_connection.begin_transaction()?;
            // let f = command.execute(action, per_action_connection);
            result.push((command, action, per_action_connection));
        }

        Ok(result)
    }

    #[allow(unreachable_code)]
    pub fn run_actions(
        conf: Config,
        database: Database,
        interval: u64,
        rx: Receiver<()>,
    ) -> Result<(), DomainActionError> {
        let router = DomainActionMonitor::create_router(&conf);

        let mut runtime = Runtime::new()?;

        loop {
            if rx.try_recv().is_ok() {
                jlog!(
                    Info,
                    "bigneon::domain_actions",
                    "Stopping actions processor",
                    {}
                );
                break;
            }
            //Check for actions that are due to be processed

            let actions = DomainActionMonitor::find_actions(
                &database,
                &router,
                cmp::max(1, conf.connection_pool.max / 2) as usize,
            )?;

            if actions.len() == 0 {
                thread::sleep(Duration::from_secs(interval));
            } else {
                for (command, action, connection) in actions {
                    let timeout =
                        Timeout::new(command.execute(action, connection), Duration::from_secs(55));

                    runtime.spawn(timeout.or_else(|err| {
                        jlog! {Error,"bigneon::domain_actions", "Action:  failed", {"error": err.to_string()}};
                        Err(())
                    }));
                }
            }
        }
        Ok(())
    }

    pub fn start(&mut self) {
        jlog!(
            Info,
            "bigneon::domain_actions",
            "Domain action monitor starting",
            {}
        );
        let config = self.config.clone();
        let database = self.database.clone();
        let interval = self.interval;

        let (tx, rx) = mpsc::channel::<()>();

        self.worker_threads.push((
            tx,
            thread::spawn(move || {
                match DomainActionMonitor::run_actions(config, database, interval, rx) {
                    Ok(_) => (),
                    Err(e) => jlog!(
                        Error,
                        "bigneon::domain_actions",
                        "Domain action monitor failed", {"error": e.description()}
                    ),
                };
                Ok(())
            }),
        ));

        let database = self.database.clone();
        let config = self.config.clone();
        let (tx, rx) = mpsc::channel::<()>();

        self.worker_threads.push((
            tx,
            thread::spawn(move || {
                DomainActionMonitor::publish_events_to_actions(config, database, interval, rx)
            }),
        ));
    }

    pub fn stop(&mut self) {
        for w in self.worker_threads.drain(..) {
            w.0.send(()).unwrap();
            w.1.join().unwrap().unwrap();
        }
    }
}
