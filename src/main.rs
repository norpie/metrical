#![forbid(missing_docs)]
//! # Metrical
//! > A simple metrics database.
//!
//! Metrical is a simple metrics database that stores metrics on disk with rocksdb.
//!
//! ## Format
//!
//! The data stored in the database is as follows:
//! * `metric` - The name of the metric.
//! * `key` - The key of the metric. This is used to identify the metric.
//! * `timestamp` - The timestamp of the metric.
//! * `value` - A float value of the metric.
//!
//! ### Examples
//!
//! #### Variable Metrics
//!
//! ```json
//! {
//!    "metric": "cpu",
//!    "key": "backend-server1",
//!    "timestamp": 1234567890,
//!    "value": 0.532
//! }
//! ```
//!
//! #### Boolean Metrics
//!
//! ```json
//! {
//!    "metric": "connected_to_db",
//!    "key": "database-server1"
//!    "timestamp": 1234567890,
//!    "value": 1
//! }
//! ```
//!
//! ## Storage
//!
//! Each `metric` has its own `store` in the database. Each `key` has a "table" in the `store`.
//! This allows for easy querying of metrics.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Error, Result};
use clap::{command, Parser};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::interface::http;

extern crate rocksdb;

/// The mod that contains the interface for the database.
mod interface;

/// The global instance of the Metrical struct.
static INSTANCE: OnceCell<Arc<RwLock<Metrical>>> = OnceCell::new();

/// # Metrical
/// The main struct that is used to interact with the database.
#[derive(Debug)]
struct Metrical {
    db: rocksdb::DB,
}

impl Metrical {
    fn get_instance() -> &'static Arc<RwLock<Self>> {
        INSTANCE.get().expect("Metrical instance not initialized")
    }

    /// Create a new Metrical instance.
    fn new(db_path: PathBuf) -> Result<Self, Error> {
        let db = rocksdb::DB::open_default(db_path)?;
        Ok(Self { db })
    }

    fn add_metric(&mut self, metric: Metric) -> Result<(), Error> {
        let key = format!("{}:{}:{}", metric.name, metric.key, metric.timestamp);
        let value = metric.value.to_string();
        self.db
            .put(key.as_bytes(), value.as_bytes())
            .map_err(|e| e.into())
    }

    fn get_metrics(&mut self, name: &str, key: &str) -> Result<Vec<Metric>, Error> {
        let prefix = format!("{}:{}:", name, key);
        let iter = self.db.prefix_iterator(prefix.as_bytes());
        let mut metrics = Vec::new();
        for result in iter {
            let (key, value) = result?;
            let key = std::str::from_utf8(&key)?;
            let value = std::str::from_utf8(&value)?;
            let parts: Vec<&str> = key.split(':').collect();
            let timestamp = parts[2].parse::<u64>()?;
            let value = value.parse::<f64>()?;
            metrics.push(Metric {
                name: parts[0].to_string(),
                key: parts[1].to_string(),
                timestamp,
                value,
            });
        }
        Ok(metrics)
    }
}

/// # Metric
/// A metric is a single data point that is stored in the database.
#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Deserialize)]
struct Metric {
    name: String,
    key: String,
    timestamp: u64,
    value: f64,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The path to the database.
    #[clap(long, default_value = "/etc/metrical/default.db")]
    db_path: PathBuf,
}

fn create_db_dir(db: &Path) -> Result<(), Error> {
    let dir = db
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Failed to get the data directory"))?;
    let result = std::fs::create_dir_all(dir);
    if result.is_ok() {
        return Ok(());
    }
    let e = result.unwrap_err();
    let kind = e.kind();
    match kind {
        std::io::ErrorKind::AlreadyExists => Ok(()),
        std::io::ErrorKind::PermissionDenied => Err(anyhow::anyhow!(
            "Permission denied to create the data directory"
        )),
        _ => Err(anyhow::anyhow!(
            "Miscellaneous error creating the data directory: {:?}",
            e
        )),
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args = Args::parse();

    println!("Opening database at: {:?}", args.db_path);
    create_db_dir(&args.db_path)?;
    INSTANCE
        .set(Arc::new(RwLock::new(Metrical::new(args.db_path)?)))
        .map_err(|_| anyhow::anyhow!("Failed to set Metrical instance"))?;

    http::serve().await?;

    Ok(())
}
