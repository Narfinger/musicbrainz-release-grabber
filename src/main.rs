use std::{path::PathBuf, fs::{read_dir, File, self}, str::FromStr, io::{self, Write}};
use anyhow::{Context, Result};
use serde::{Serialize, Deserialize};

pub mod responses;

#[derive(Default, Debug, Serialize, Deserialize)]
struct Config {
    artist_names: Vec<String>,
    artist_ids: Vec<usize>,
    last_checked_time: (),
}

impl Config {
    fn read() -> Result<Config> {
        let s = fs::read_to_string("config.toml")?;
        toml::from_str(&s)?
    }

    fn write(&self) -> Result<()> {
        let str = toml::to_string_pretty(&self)?;
        fs::write("config.toml", str)?;
        Ok(())
    }
}


fn grab_new_releases() {

}

fn get_artists(base_dir: String) -> Result<()> {
    let dir = PathBuf::from_str(&base_dir)?;
    let mut entries = read_dir(&dir)?
        .filter_map(|res| res.map(|e| e.path()).ok())
        .filter_map(|p| p.file_name())
        .filter_map(|os| os.to_str())
        .map(String::from)
        .collect::<Vec<String>>();

    entries.sort();


    let mut c = Config::default();
    c.artist_names = entries;
    c.write();
    Ok(())
}

fn get_artist_ids() -> Result<()> {
    todo!()
}

fn main() {
    println!("Hello, world!");
}
