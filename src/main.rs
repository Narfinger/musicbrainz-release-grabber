use anyhow::bail;
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use clap::Subcommand;
use clap::ValueEnum;
use directories::ProjectDirs;
use indicatif::ProgressBar;
use indicatif::ProgressIterator;
use indicatif::ProgressStyle;
use nu_ansi_term::Color::{Blue, Green, Red};
use regex::Regex;
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
use time::Date;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::responses::TIMEOUT;

pub mod responses;

/// Progress bar style
const PROGRESS_STYLE: &str =
    "[{spinner:.green}] [{elapsed}/{eta}] {bar:40.cyan/blue} {pos:>7}/{len:7} ({percent}%)";

/// The config struct
#[derive(Debug, Serialize, Deserialize)]
struct Config {
    /// Artists names only, gotten from the directory
    artist_names: Vec<String>,
    /// Artists we currently check
    artist_full: Vec<Artist>,
    /// last time we checked for new
    last_checked_time: Date,
    /// ids that we ignore
    ignore_ids: Vec<Uuid>,
}

impl Default for Config {
    /// default empty config
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
    /// reads the config
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

    /// Writes a given config to file
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

    // writes the config with time today (minus one day for safety)
    fn now(&mut self) -> Result<()> {
        //remove one day just to be sure
        self.last_checked_time = OffsetDateTime::now_utc().date() - time::Duration::DAY;
        self.write()
    }
}

/// get the artists ids for all artists in artist_names
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
            .template(PROGRESS_STYLE)?
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

/// check for releases later then last checked date from artist_full
fn grab_new_releases() -> Result<()> {
    let client = get_client()?;

    let mut c = Config::read()?;
    println!("Finding new albums from {}", c.last_checked_time);
    let duration = TIMEOUT.checked_mul(c.artist_full.len() as i32).unwrap();
    println!("This will take at least {}", duration);
    let pb = ProgressBar::new(c.artist_names.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(PROGRESS_STYLE)?
            .progress_chars("##-"),
    );
    pb.enable_steady_tick(std::time::Duration::new(0, 500));
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

/// create a reqwest client with correct http header
fn get_client() -> Result<reqwest::blocking::Client, anyhow::Error> {
    reqwest::blocking::ClientBuilder::new()
        .user_agent("MusicbrainzReleaseGrabber/1.0 ( https://github.com/narfinger )")
        .build()
        .context("Could not build client")
}

/// Print all the albums we got in the vector in a nice way
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
                Red.bold().paint(&i.artist),
                Blue.bold().paint(&date),
                Green.bold().paint(&i.title)
            );
        }
    }
    Ok(())
}

/// fill all artist_names into the config from a directory
fn get_artists(dir: PathBuf) -> Result<()> {
    //let dir = PathBuf::from_str(&base_dir)?;
    let dir_count = read_dir(&dir)?.count();
    let dur = TIMEOUT.checked_mul(dir_count as i32).unwrap();
    println!("Counting artists. This will take at least {}", dur);
    let mut entries = read_dir(&dir)?
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

/// Find all artists that are in the directory `dir` but not in the config
fn artists_not_in_config(dir: &PathBuf) -> Result<()> {
    /// FIXME this whole thing needs less cloning
    let chars_to_remove_regexp = Regex::new(r"\.\&\'")?;

    let dir_count = read_dir(dir)?.count();
    let dur = TIMEOUT.checked_mul(dir_count as i32).unwrap();
    println!("Counting artists. This will take at least {}", dur);
    let dir_entries = read_dir(dir)?
        .progress_count(dir_count as u64)
        .filter_map(|res| res.map(|e| e.path()).ok())
        .filter_map(|p| p.file_name().and_then(|p| p.to_str()).map(String::from))
        .filter(|r| !r.contains('-') && !r.contains("Best") && !r.contains("Greatest"))
        .map(|s| s.to_lowercase())
        .map(|i| chars_to_remove_regexp.replace_all(&i, "").into())
        .collect::<HashSet<String>>();

    let config = Config::read()?;
    let artist_in_config = config
        .artist_full
        .into_iter()
        .map(|a| a.name.to_lowercase())
        .map(|i| chars_to_remove_regexp.replace_all(&i, "").into())
        .collect::<Vec<String>>();

    let mut res = {
        // remove things that currently match
        let c: HashSet<String> = HashSet::from_iter(artist_in_config.iter().cloned());
        dir_entries.difference(&c).cloned().collect::<Vec<String>>()
    };

    println!("artists found in directory but not config");
    res.sort_unstable();

    for i in res {
        println!("\"{}\"", i);
    }
    Ok(())
}

/// Which values to clear in the config
#[derive(ValueEnum, Clone, Debug, PartialEq, Eq)]
enum ClearValues {
    Ids,
    Artists,
    WholeConfig,
}

/// Subcommands
#[derive(Debug, Subcommand)]
enum SubCommands {
    /// Adds an artist to our list
    Add { name: String },

    /// List artists
    List,

    /// Delete an artist
    Delete { name: String },

    /// Find new albums
    New,
}

/// Arguments for the program
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Get the artists from a file
    #[clap(short, long, value_parser =valid_dir, value_name = "DIR")]
    music_dir: Option<PathBuf>,

    /// get artists ids
    #[clap(short, long)]
    ids: bool,

    /// Clear config values
    #[clap(short, long, value_enum)]
    clear: Option<ClearValues>,

    #[clap(subcommand)]
    artists: Option<SubCommands>,

    /// Artists not in config
    #[clap(short, long, value_parser = valid_dir, value_name = "DIR")]
    artists_not_in_config: Option<PathBuf>,
}

/// is this directory a valid direcotry
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
    } else if let Some(p) = args.artists_not_in_config {
        artists_not_in_config(&p)?;
    } else if let Some(cmd) = args.artists {
        let mut c = Config::read()?;
        match cmd {
            SubCommands::Add { name } => {
                let client = get_client()?;
                let new_artist = Artist::new(&client, &name)?;
                println!(
                    "Found artist \"{}\" for search \"{}\"",
                    new_artist.name, new_artist.search_string
                );
                if c.artist_full.iter().any(|a| a.id == new_artist.id) {
                    println!("Artist is already in the list");
                } else {
                    c.artist_full.push(new_artist);
                    c.artist_full.sort_unstable();
                    c.write()?;
                }
            }
            SubCommands::List => {
                for i in c.artist_full {
                    println!("{}", i.name);
                }
            }
            SubCommands::Delete { name } => {
                c.artist_full.retain(|a| a.name != name);
                c.write()?;
            }
            SubCommands::New => {
                grab_new_releases()?;
            }
        }
    }

    Ok(())
}
