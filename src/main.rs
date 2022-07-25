use ansi_term::Colour::{Blue, Green, Red};
use anyhow::{anyhow, Context, Result};
use clap::CommandFactory;
use clap::Parser;
use indicatif::ProgressIterator;
use responses::{Album, Artist};
use serde::{Deserialize, Serialize};
use time::Date;
use time::OffsetDateTime;
use time::format_description;
use uuid::Uuid;
use std::{
    fs::{self, read_dir},
    path::PathBuf,
    str::FromStr,
};

pub mod responses;

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    artist_names: Vec<String>,
    artist_full: Vec<Artist>,
    last_checked_time: Date,
    ignore_ids: Vec<Uuid>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            artist_full: vec![],
            artist_names: vec![],
            last_checked_time: OffsetDateTime::now_utc().date(),
            ignore_ids: vec![],
        }
    }
}

impl Config {
    fn read() -> Result<Config> {
        let s = fs::read_to_string("config.toml").context("Reading config file")?;
        serde_json::from_str::<Config>(&s).context("Could not read config")
    }

    fn write(&self) -> Result<()> {
        let str = serde_json::to_string_pretty(&self).context("Toml to string")?;
        fs::write("config.toml", str).context("Writing string")?;
        Ok(())
    }

    fn now(&mut self) -> Result<()> {
        self.last_checked_time=  OffsetDateTime::now_utc().date();
        self.write()
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

    let mut c = Config::read()?;
    let mut all_albums: Vec<Album> = Vec::new();
    for a in c.artist_full.iter().progress() {
        let mut albums = a.get_albums_basic_filtered(&client)?;
        all_albums.append(&mut albums);
    }

    println!("Filtering results");
    let mut res = all_albums
        .iter()
        .filter(|a| a.date.is_none() || a.date.unwrap() >= c.last_checked_time)
        .collect::<Vec<&Album>>();
    res.sort_by_cached_key(|a| a.artist.clone());

    print_new_albums(&res)?;

    // updateing config
    c.now()?;
    Ok(())
}

fn print_new_albums(a: &[&Album]) -> Result<()> {
    println!("Found {} new albums", a.len());
    let format = format_description::parse("[year]-[month]-[day]")?;
    for i in a {
        let date:String = i.date.and_then(|d| d.format(&format).ok()).unwrap_or_else(|| "NONE".to_string());
        println!("{} - {} - {}", Red.paint(&i.artist), Blue.paint(&date), Green.paint(&i.title));
    }
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

    let c = Config { artist_names: entries, ..Default::default()};
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

    /// Write config
    #[clap(short, long)]
    config: bool,
}

fn valid_dir(s: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from_str(s).map_err(|_| "Not a valid directory description".to_string())?;
    if !p.exists() {
        Err("Directory does not exist".to_string())
    } else if !p.is_dir() {
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
    } else if args.config {
        let c = Config::default();
        c.write()?;
    } else {
        let mut cmd = Args::command();
        cmd.print_help()?;
    }

    Ok(())
}
