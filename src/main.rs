use ansi_term::Colour::{Blue, Green, Red};
use anyhow::bail;
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use clap::ValueEnum;
use clap::{CommandFactory, Subcommand};
use directories::ProjectDirs;
use indicatif::ProgressBar;
use indicatif::ProgressIterator;
use indicatif::ProgressStyle;
use responses::{Album, Artist};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::create_dir;
use std::thread;
use std::{
    fs::{self, read_dir},
    path::PathBuf,
    str::FromStr,
};
use time::format_description;
use time::OffsetDateTime;
use time::{Date, Duration};
use uuid::Uuid;

use crate::responses::TIMEOUT;

pub mod responses;

const PROGRESS_STYLE: &str =
    "[{spinner:.green}] [{elapsed}/{eta}] {bar:40.cyan/blue} {pos:>7}/{len:7} ({percent}%)";

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

    fn write(&self) -> Result<()> {
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

    fn now(&mut self) -> Result<()> {
        //remove one day just to be sure
        self.last_checked_time = OffsetDateTime::now_utc().date() - time::Duration::DAY;
        self.write()
    }
}

fn get_artist_ids() -> Result<()> {
    let client = get_client()?;
    let mut c = Config::read()?;

    //c.artist_full.clear();
    let already_found_artists: HashSet<String> =
        c.artist_full.iter().map(|a| a.name.clone()).collect();
    let artist_names: HashSet<String> = c.artist_names.iter().cloned().collect();

    let mut error_artist = Vec::new();

    let duration = TIMEOUT.checked_mul(artist_names.len() as i32).unwrap();
    println!("Getting artists ids. This will take roughly {}", duration);

    let pb = ProgressBar::new(c.artist_names.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(PROGRESS_STYLE)
            .progress_chars("##-"),
    );

    for i in artist_names
        .difference(&already_found_artists)
        .progress_with(pb)
    {
        match Artist::new(&client, i) {
            Ok(a) => c.artist_full.push(a),
            Err(e) => error_artist.push(format!("{} with error {:?}", i, e)),
        }
        thread::sleep(responses::TIMEOUT.unsigned_abs()); //otherwise we are hammering the api too much.
    }
    c.artist_full.sort_unstable();
    println!("Writing artists we found");
    c.write()?;

    if !error_artist.is_empty() {
        println!("We did not find matching artist ids for the following artists");
        for i in error_artist {
            println!("{}", i);
        }
    }

    println!("Artist where we found differences");
    for a in c.artist_full {
        if a.name != a.search_string {
            println!(
                "Artist difference name: \"{}\" search: \"{}\"",
                a.name, a.search_string
            );
        }
    }

    Ok(())
}

fn grab_new_releases() -> Result<()> {
    let client = get_client()?;

    let mut c = Config::read()?;
    println!("Finding new albums from {}", c.last_checked_time);
    let duration = TIMEOUT.checked_mul(c.artist_full.len() as i32).unwrap();
    println!("This will take at least {}", duration);
    let pb = ProgressBar::new(c.artist_names.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(PROGRESS_STYLE)
            .progress_chars("##-"),
    );
    pb.enable_steady_tick(500);
    let mut error_artists = Vec::new();
    let mut all_albums: Vec<Album> = Vec::new();
    for a in c.artist_full.iter().progress_with(pb) {
        //for a in c.artist_full.iter() {
        //    println!("artist {}", a.name);
        if let Ok(mut albums) = a.get_albums_basic_filtered(&client) {
            all_albums.append(&mut albums);
        } else {
            error_artists.push(a);
        }
        thread::sleep(responses::TIMEOUT.unsigned_abs()); //otherwise we are hammering the api too much.
    }
    if !error_artists.is_empty() {
        println!("Could not get all artists. Please check manually the following:");
        for i in error_artists {
            println!("{}", i.name);
        }
    }

    println!("Filtering results");
    let mut res = all_albums
        .iter()
        .filter(|a| a.date.is_some() && a.date.unwrap() >= c.last_checked_time)
        .collect::<Vec<&Album>>();
    res.sort_unstable();

    print_new_albums(&res)?;

    // updateing config
    c.now()?;
    Ok(())
}

fn get_client() -> Result<reqwest::blocking::Client, anyhow::Error> {
    reqwest::blocking::ClientBuilder::new()
        .user_agent("MusicbrainzReleaseGrabber")
        .build()
        .context("Could not build client")
}

fn print_new_albums(a: &[&Album]) -> Result<()> {
    println!("Found {} new albums", a.len());
    let today = time::OffsetDateTime::now_utc().date() - time::Duration::DAY;
    let format = format_description::parse("[year]-[month]-[day]")?;
    for i in a {
        let date: String = i
            .date
            .and_then(|d| d.format(&format).ok())
            .unwrap_or_else(|| "NONE".to_string());
        if i.date.is_some() && i.date.unwrap() >= today {
            println!(
                "{} - {} - {}",
                Red.strikethrough().paint(&i.artist),
                Blue.strikethrough().paint(&date),
                Green.strikethrough().paint(&i.title)
            )
        } else {
            println!(
                "{} - {} - {}",
                Red.paint(&i.artist),
                Blue.paint(&date),
                Green.paint(&i.title)
            );
        }
    }
    Ok(())
}

fn get_artists(dir: PathBuf) -> Result<()> {
    //let dir = PathBuf::from_str(&base_dir)?;
    let dir_count = read_dir(&dir)?.count();
    let dur = TIMEOUT.checked_mul(dir_count as i32).unwrap();
    println!("Counting artists. This will take at least {}", dur);
    let mut entries = read_dir(&dir)?
        .into_iter()
        .progress_count(dir_count as u64)
        .filter_map(|res| res.map(|e| e.path()).ok())
        .filter_map(|p| p.file_name().and_then(|p| p.to_str()).map(String::from))
        .filter(|r| !r.contains('-') && !r.contains("Best") && !r.contains("Greatest"))
        .collect::<Vec<String>>();

    entries.sort_unstable();

    let c = Config {
        artist_names: entries,
        ..Default::default()
    };
    c.write()?;
    Ok(())
}

#[derive(ValueEnum, Clone, Debug, PartialEq, Eq)]
enum ClearValues {
    Ids,
    Artists,
    WholeConfig,
}

#[derive(Debug, Subcommand)]
enum ArtistCommands {
    /// Adds an artist to our list
    Add { name: String },

    /// List artists
    List,

    /// Delete an artist
    Delete { name: String },
}

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Get the artists from a file
    #[clap(short, long, value_parser =valid_dir, value_name = "DIR")]
    music_dir: Option<PathBuf>,

    /// get artists ids
    #[clap(short, long)]
    ids: bool,

    /// find new albums
    #[clap(short, long)]
    new: bool,

    /// Clear config values
    #[clap(short, long, value_enum)]
    clear: Option<ClearValues>,

    #[clap(subcommand)]
    artists: Option<ArtistCommands>,
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
    if let Some(path) = args.music_dir {
        println!("Getting artists");
        get_artists(path)?;
    } else if args.ids {
        get_artist_ids()?;
    } else if args.new {
        grab_new_releases()?;
    } else if let Some(cl) = args.clear {
        if cl == ClearValues::WholeConfig {
            let c = Config::default();
            return c.write();
        }
        let mut c = Config::read()?;
        match cl {
            ClearValues::Ids => c.artist_full = vec![],
            ClearValues::Artists => c.artist_names = vec![],
            ClearValues::WholeConfig => bail!("This should never happen"),
        }
    } else if let Some(cmd) = args.artists {
        let mut c = Config::read()?;
        match cmd {
            ArtistCommands::Add { name } => {
                let client = get_client()?;
                let a = Artist::new(&client, &name)?;
                println!("Found artist {} for search {}", a.name, a.search_string);
                c.artist_full.push(a);
                c.artist_full.sort_unstable();
                c.write()?;
            }
            ArtistCommands::List => {
                for i in c.artist_full {
                    println!("{}", i.name);
                }
            }
            ArtistCommands::Delete { name } => {
                c.artist_full.retain(|a| a.name != name);
                c.write()?;
            }
        }
    }

    Ok(())
}
