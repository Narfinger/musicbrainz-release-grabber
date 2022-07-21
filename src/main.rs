use ansi_term::Colour::{Blue, Green, Red};
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use indicatif::ProgressIterator;
use responses::{Album, Artist};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::{
    fs::{self, read_dir},
    path::PathBuf,
    str::FromStr,
};

pub mod responses;

#[derive(Default, Debug, Serialize, Deserialize)]
struct Config {
    artist_names: Vec<String>,
    artist_full: Vec<(Uuid, String)>,
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

fn get_artist_ids() -> Result<()> {
    let client = reqwest::blocking::ClientBuilder::new()
        .user_agent("MusicbrainzReleaseGrabber")
        .build()?;
    let mut c = Config::read()?;
    c.artist_full.clear();
    for i in c.artist_names.iter().progress() {
        let a = Artist::new(&client, &i)?;
        c.artist_full.push((a.id,i.clone()));
    }
    c.write()?;
    Ok(())
}

fn grab_new_releases() -> Result<()> {
    let client = reqwest::blocking::ClientBuilder::new()
        .user_agent("MusicbrainzReleaseGrabber")
        .build()?;

    let c = Config::read()?;
    let mut all_albums: Vec<Album> = Vec::new();
    for (id,name) in c.artist_full.into_iter().progress() {
        let a = Artist { id, name};
        let mut albums = a.get_albums_basic_filtered(&client)?;
        all_albums.append(&mut albums);
    }

    ///do filter here

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

    /// get artists ids
    #[clap(short, long)]
    get_artists_ids: bool,

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
    } else if args.get_artists_ids {
        get_artist_ids()?;
    } else if args.new {
        grab_new_releases()?;
    } else {
        println!("Please use an argument");
    }

    Ok(())
}
