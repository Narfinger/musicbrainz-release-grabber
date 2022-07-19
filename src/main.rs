use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, read_dir, File},
    io::{self, Write},
    path::PathBuf,
    str::FromStr,
};

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
        toml::from_str::<Config>(&s).context("Could not read config")
    }

    fn write(&self) -> Result<()> {
        let str = toml::to_string_pretty(&self)?;
        fs::write("config.toml", str)?;
        Ok(())
    }
}

fn grab_new_releases() {}

fn get_artists(base_dir: String) -> Result<()> {
    let dir = PathBuf::from_str(&base_dir)?;
    let mut entries = read_dir(&dir)?
        .filter_map(|res| res.map(|e| e.path()).ok())
        .filter_map(|p| {
            p.file_name()
                .and_then(|p| p.to_str())
                .map(|s| String::from(s))
        })
        .collect::<Vec<String>>();

    entries.sort();

    let mut c = Config::default();
    c.artist_names = entries;
    c.write()?;
    Ok(())
}

fn get_artist_ids() -> Result<()> {
    todo!()
}

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Get the artists from a file
    #[clap(short, long, value_parser)]
    get_artists: Option<String>,

    /// update ids
    #[clap(short, long, value_parser)]
    count: bool,

    /// find new albums
    #[clap(short, long, value_parser)]
    new: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if let Some(path) = args.get_artists {
        println!("Getting artists");
        get_artists(path)?;
    }

    Ok(())
}
