use ansi_term::Colour::{Blue, Green, Red};
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use indicatif::ProgressIterator;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, read_dir},
    path::PathBuf,
    str::FromStr,
};
use responses::{Album, Artist};

pub mod responses;

#[derive(Default, Debug, Serialize, Deserialize)]
struct Config {
    artist_names: Vec<String>,
    artist_ids: Vec<usize>,
    last_checked_time: usize,
    discogs_key: String,
    discogs_secret: String,
}

impl Config {
    fn read() -> Result<Config> {
        let s = fs::read_to_string("config.toml").context("Reading config file")?;
        toml::from_str::<Config>(&s).context("Could not read config")
    }

    fn write(&self) -> Result<()> {
        let str = toml::to_string_pretty(&self).context("Toml to string")?;
        fs::write("config.toml", str).context("Writing string")?;
        Ok(())
    }
}

fn grab_new_releases() -> Result<()> {
    let client = reqwest::blocking::ClientBuilder::new().user_agent("MusicbrainzReleaseGrabber").build()?;
    let a = Artist::new(&client, "Blind Guardian")?;
    let albums = a.get_albums_basic_filtered(&client);
    println!("{:?}", albums);

    Ok(())
}

fn get_artists(dir: PathBuf) -> Result<()> {
    //let dir = PathBuf::from_str(&base_dir)?;
    println!("Counting artists");
    let dir_count = read_dir(&dir)?.count();
    let mut entries = read_dir(&dir)?
        .into_iter()
        .progress_count(dir_count as u64)
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

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Get the artists from a file
    #[clap(short, long, value_parser)]
    get_artists: Option<String>,

    /// find new albums
    #[clap(short, long)]
    new: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if let Some(path) = args.get_artists {
        println!("Getting artists");
        let p = PathBuf::from_str(&path)?;
        get_artists(p)?;
    } else if args.new {
        grab_new_releases()?;
    } else {
        println!("Please use an argument");
    }

    Ok(())
}
