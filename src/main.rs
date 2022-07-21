use ansi_term::Colour::{Blue, Green, Red};
use anyhow::{anyhow, Context, Result};
use clap::CommandFactory;
use clap::Parser;
use indicatif::ProgressIterator;
use responses::{Album, Artist};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, read_dir},
    path::PathBuf,
    str::FromStr,
};

pub mod responses;

#[derive(Default, Debug, Serialize, Deserialize)]
struct Config {
    artist_names: Vec<String>,
    artist_full: Vec<Artist>,
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
        let a = Artist::new(&client, i)?;
        c.artist_full.push(a);
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
    for a in c.artist_full.into_iter().progress() {
        let mut albums = a.get_albums_basic_filtered(&client)?;
        all_albums.append(&mut albums);
    }

    todo!("do filtering");
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
                .map(String::from)
        })
        .collect::<Vec<String>>();

    entries.sort();

    let c = Config {artist_names: entries, ..Default::default()};
    c.write()?;
    Ok(())
}

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Get the artists from a file
    #[clap(short, long, value_parser =valid_dir, value_name = "DIR")]
    artists: Option<PathBuf>,

    /// get artists ids
    #[clap(short, long)]
    ids: bool,

    /// find new albums
    #[clap(short, long)]
    new: bool,
}

fn valid_dir(s: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from_str(s).map_err(|_| "Not a valid directory description".to_string())?;
    if !p.exists() {
        Err("Directory does not exist".to_string())
    } else if p.is_dir() {
        Err("Not a directory".to_string())
    } else {
        Ok(p)
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    if let Some(path) = args.artists {
        println!("Getting artists");
        get_artists(path)?;
    } else if args.ids {
        get_artist_ids()?;
    } else if args.new {
        grab_new_releases()?;
    } else {
        let mut cmd = Args::command();
        cmd.print_help()?;
    }

    Ok(())
}
