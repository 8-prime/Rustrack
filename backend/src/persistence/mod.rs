use std::sync::Mutex;

use anyhow::{Result, anyhow};
use rusqlite::Connection;
use shared::lif::LifSummary;

use crate::configuration::configuration::Configuration;

pub struct Persistence {
    conn: Mutex<Connection>,
}

impl Persistence {
    pub fn new() -> Result<Persistence> {
        let path = "./runtimes_store.db";
        let conn = Connection::open(path)?;

        Ok(Persistence {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| anyhow!("persistence lock poisoned"))
    }

    pub fn init(&self) -> Result<()> {
        let table = "
            CREATE TABLE IF NOT EXISTS persisted_configuration (
                id                   TEXT    NOT NULL PRIMARY KEY,
                name                 TEXT    NOT NULL,
                mqtt_url             TEXT    NOT NULL,
                mqtt_port            INTEGER NOT NULL,
                mqtt_username        TEXT,
                mqtt_password        TEXT,
                tls_skip_verify      INTEGER NOT NULL,  -- boolean: 0 or 1
                vda5050_topic_prefix TEXT    NOT NULL,
                created_at           TEXT    NOT NULL
            );
           ";

        self.conn()?.execute(
            table,
            (), // empty list of parameters.
        )?;

        let lif_table = "
            CREATE TABLE IF NOT EXISTS persisted_lif_map (
                system_id              TEXT    NOT NULL PRIMARY KEY,
                raw_gzip               BLOB    NOT NULL,
                raw_bytes              INTEGER NOT NULL,  -- uncompressed size
                project_identification TEXT    NOT NULL,
                lif_version            TEXT    NOT NULL,
                layout_count           INTEGER NOT NULL,
                node_count             INTEGER NOT NULL,
                edge_count             INTEGER NOT NULL,
                station_count          INTEGER NOT NULL,
                uploaded_at            TEXT    NOT NULL
            );
           ";

        self.conn()?.execute(lif_table, ())?;

        Ok(())
    }

    /// Store (or replace) the layout for a system.
    pub fn upsert_lif_map(
        &self,
        system_id: &str,
        raw_gzip: &[u8],
        summary: &LifSummary,
    ) -> Result<()> {
        self.conn()?.execute(
            "INSERT INTO persisted_lif_map (system_id, raw_gzip, raw_bytes, project_identification, lif_version, layout_count, node_count, edge_count, station_count, uploaded_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) \
             ON CONFLICT(system_id) DO UPDATE SET \
               raw_gzip = ?2, raw_bytes = ?3, project_identification = ?4, lif_version = ?5, \
               layout_count = ?6, node_count = ?7, edge_count = ?8, station_count = ?9, uploaded_at = ?10",
            rusqlite::params![
                system_id,
                raw_gzip,
                summary.raw_bytes as i64,
                &summary.project_identification,
                &summary.lif_version,
                summary.layout_count as i64,
                summary.node_count as i64,
                summary.edge_count as i64,
                summary.station_count as i64,
                &summary.uploaded_at,
            ],
        )?;
        Ok(())
    }

    /// Read the compressed layout bytes for a system.
    ///
    /// Deliberately separate from [`Self::read_lif_summary`]: the connection is
    /// behind a single mutex, so pulling megabytes through it is something
    /// callers should have to ask for explicitly.
    pub fn read_lif_gzip(&self, system_id: &str) -> Result<Option<Vec<u8>>> {
        let conn = self.conn()?;
        let mut stmt =
            conn.prepare("SELECT raw_gzip FROM persisted_lif_map WHERE system_id = ?1")?;
        let mut rows = stmt.query([system_id])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get(0)?)),
            None => Ok(None),
        }
    }

    /// Read every system's layout summary. Never touches `raw_gzip`, so startup
    /// cost is independent of how large the stored layouts are.
    pub fn read_all_lif_summaries(&self) -> Result<Vec<(String, LifSummary)>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT system_id, project_identification, lif_version, layout_count, node_count, edge_count, station_count, raw_bytes, uploaded_at FROM persisted_lif_map",
        )?;
        let iter = stmt.query_map([], |row| {
            std::result::Result::Ok((
                row.get::<_, String>(0)?,
                LifSummary {
                    project_identification: row.get(1)?,
                    lif_version: row.get(2)?,
                    layout_count: row.get::<_, i64>(3)? as usize,
                    node_count: row.get::<_, i64>(4)? as usize,
                    edge_count: row.get::<_, i64>(5)? as usize,
                    station_count: row.get::<_, i64>(6)? as usize,
                    raw_bytes: row.get::<_, i64>(7)? as u64,
                    uploaded_at: row.get(8)?,
                },
            ))
        })?;

        Ok(iter.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn delete_lif_map(&self, system_id: &str) -> Result<()> {
        self.conn()?.execute(
            "DELETE FROM persisted_lif_map WHERE system_id = ?1",
            (system_id,),
        )?;
        Ok(())
    }

    pub fn read_configurations(&self) -> Result<Vec<Configuration>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT id, name, mqtt_url, mqtt_port, mqtt_username, mqtt_password, tls_skip_verify, vda5050_topic_prefix, created_at FROM persisted_configuration")?;
        let config_iter = stmt.query_map([], |row| {
            Ok(Configuration {
                id: row.get(0)?,
                name: row.get(1)?,
                mqtt_url: row.get(2)?,
                mqtt_port: row.get(3)?,
                mqtt_username: row.get(4)?,
                mqtt_password: row.get(5)?,
                tls_skip_verify: row.get(6)?,
                vda5050_topic_prefix: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?;

        Ok(config_iter.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn add_configuration(&self, cfg: Configuration) -> Result<()> {
        self.conn()?.execute(
            "INSERT INTO persisted_configuration (id, name, mqtt_url, mqtt_port, mqtt_username, mqtt_password, tls_skip_verify, vda5050_topic_prefix, created_at) values (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            (&cfg.id,&cfg.name, &cfg.mqtt_url,&cfg.mqtt_port,&cfg.mqtt_username,&cfg.mqtt_password,&cfg.tls_skip_verify,&cfg.vda5050_topic_prefix,&cfg.created_at)
        )?;
        Ok(())
    }

    pub fn update_configuration(&self, cfg: Configuration) -> Result<()> {
        self.conn()?.execute(
            "UPDATE persisted_configuration SET name = ?2, mqtt_url = ?3, mqtt_port = ?4, mqtt_username = ?5, mqtt_password = ?6, tls_skip_verify = ?7, vda5050_topic_prefix = ?8 WHERE id = ?1",
            (&cfg.id,&cfg.name, &cfg.mqtt_url,&cfg.mqtt_port,&cfg.mqtt_username,&cfg.mqtt_password,&cfg.tls_skip_verify,&cfg.vda5050_topic_prefix)
        )?;
        Ok(())
    }

    pub fn delete_configuration(&self, id: String) -> Result<()> {
        self.conn()?
            .execute("DELETE FROM persisted_configuration where id = ?1", (&id,))?;
        Ok(())
    }
}
