use std::{
    fs::{self, create_dir},
    path::PathBuf,
};

use anyhow::{anyhow, Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};

use crate::{Album, Artist};

pub(crate) const CHARS_TO_REMOVE: &[char; 5] = &['.', '&', '\'', '’', '/'];

/// The config struct
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Config {
    /// Artists names only, gotten from the directory
    pub(crate) artist_names: Vec<String>,
    /// Artists we currently check
    pub(crate) artist_full: Vec<Artist>,
    /// last time we checked for new
    pub(crate) last_checked_time: Date,
    /// paths that we ignore
    pub(crate) ignore_paths: Vec<String>,
    /// previous new albums,
    pub(crate) previous: Vec<Album>,
}

impl Default for Config {
    /// default empty config
    fn default() -> Self {
        Self {
            artist_full: vec![],
            artist_names: vec![],
            last_checked_time: OffsetDateTime::now_utc().date(),
            ignore_paths: vec![],
            previous: vec![],
        }
    }
}

impl Config {
    /// reads the config
    pub(crate) fn read() -> Result<Config> {
        if let Some(project_dirs) =
            ProjectDirs::from("io", "narfinger.github", "musicbrainz-release-grabber")
        {
            let mut dir = project_dirs.config_dir().to_path_buf();
            dir.push("config.json");
            let s = fs::read_to_string(dir).context("Reading config file")?;
            serde_json::from_str::<Config>(&s).context("Could not read config")
        } else {
            Err(anyhow!("Could not find project dir"))
        }
    }

    /// Writes a given config to file
    pub(crate) fn write(&self) -> Result<()> {
        if let Some(project_dirs) =
            ProjectDirs::from("io", "narfinger.github", "musicbrainz-release-grabber")
        {
            let mut dir = project_dirs.config_dir().to_path_buf();
            if !dir.exists() {
                create_dir(&dir)?;
            }
            dir.push("config.json");
            let str = serde_json::to_string_pretty(&self).context("JSON to string")?;
            fs::write(dir, str).context("Writing string")?;
            Ok(())
        } else {
            Err(anyhow!("Could not find project dir"))
        }
    }

    // writes the config with time today (minus one day for safety)
    pub(crate) fn now(&mut self) -> Result<()> {
        //remove one day just to be sure
        self.last_checked_time = OffsetDateTime::now_utc().date() - time::Duration::DAY;
        self.write()
    }

    pub(crate) fn add_ignore(&mut self, p: PathBuf) -> Result<()> {
        let s = p
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_lowercase()
            .to_string()
            .replace(CHARS_TO_REMOVE, "");
        if self.ignore_paths.contains(&s) {
            println!("Ignore already in place");
        }
        self.ignore_paths.push(s);
        self.write()
    }
}
