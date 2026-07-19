use std::sync::Mutex;

use anyhow::{Result, anyhow};
use rusqlite::Connection;

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
