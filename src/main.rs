use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use directories::ProjectDirs;
use indicatif::{ProgressBar, ProgressIterator, ProgressStyle};
use ratelimit::Ratelimiter;
use responses::{Album, Artist};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::create_dir;
use std::time::Duration;
use std::{
    fs::{self, read_dir},
    path::PathBuf,
    str::FromStr,
};
use time::format_description;
use time::Date;
use time::OffsetDateTime;
use yansi::Paint;

use crate::responses::ReleaseType;

pub mod responses;

/// Progress bar style
const PROGRESS_STYLE: &str =
    "[{spinner:.green}] [{elapsed}/{eta}] {bar:40.cyan/blue} {pos:>7}/{len:7} ({percent}%)";

const CHARS_TO_REMOVE: &[char; 5] = &['.', '&', '\'', '’', '/'];

/// The config struct
#[derive(Debug, Serialize, Deserialize)]
struct Config {
    /// Artists names only, gotten from the directory
    artist_names: Vec<String>,
    /// Artists we currently check
    artist_full: Vec<Artist>,
    /// last time we checked for new
    last_checked_time: Date,
    /// paths that we ignore
    ignore_paths: Vec<String>,
}

impl Default for Config {
    /// default empty config
    fn default() -> Self {
        Self {
            artist_full: vec![],
            artist_names: vec![],
            last_checked_time: OffsetDateTime::now_utc().date(),
            ignore_paths: vec![],
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

    fn add_ignore(&mut self, p: PathBuf) -> Result<()> {
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

/// get the artists ids for all artists in artist_names
fn get_artist_ids(ratelimiter: &Ratelimiter) -> Result<()> {
    let client = get_client()?;
    let mut c = Config::read()?;

    //c.artist_full.clear();
    let already_found_artists: HashSet<String> =
        c.artist_full.iter().map(|a| a.name.clone()).collect();
    let artist_names: HashSet<String> = c.artist_names.iter().cloned().collect();

    let mut error_artist = Vec::new();

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
        match Artist::new(&client, i, ratelimiter) {
            Ok(a) => c.artist_full.push(a),
            Err(e) => error_artist.push(format!("{} with error {:?}", i, e)),
        }
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
fn grab_new_releases(ratelimiter: &Ratelimiter) -> Result<()> {
    let client = get_client()?;

    let mut c = Config::read()?;
    println!("Finding new albums from {}", c.last_checked_time);
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
        let res = a.get_albums_basic_filtered(&client, ratelimiter);
        if let Ok(mut albums) = res {
            all_albums.append(&mut albums);
        } else {
            error_artists.push(a);
            println!("re {:?}", res);
        }
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

    let others = res
        .clone()
        .into_iter()
        .filter(|a| a.release_type != ReleaseType::Album)
        .collect::<Vec<&Album>>();
    println!("Printing {} Others", others.len());
    print_new_albums(&others)?;
    let albums = res
        .into_iter()
        .filter(|a| a.release_type == ReleaseType::Album)
        .collect::<Vec<&Album>>();
    println!("---------------------------------------------------------");
    println!("Printing {} Albums", albums.len());
    print_new_albums(&albums)?;

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
    let today = time::OffsetDateTime::now_utc().date() - time::Duration::DAY;
    let format = format_description::parse("[year]-[month]-[day]")?;
    for i in a {
        let date: String = i
            .date
            .and_then(|d| d.format(&format).ok())
            .unwrap_or_else(|| "NONE".to_string());
        if i.date.is_some() && i.date.unwrap() >= today {
            println!(
                "{} - {} - {} - ({})",
                i.artist.red().strike(),
                date.blue().strike(),
                i.title.green().strike(),
                i.release_type.to_string().yellow().strike(),
            )
        } else {
            println!(
                "{} - {} - {} - ({})",
                i.artist.red().bold(),
                date.blue().blue().bold(),
                i.title.green().bold(),
                i.release_type.to_string().yellow(),
            );
        }
    }
    Ok(())
}

/// fill all artist_names into the config from a directory
fn get_artists(dir: PathBuf) -> Result<()> {
    //let dir = PathBuf::from_str(&base_dir)?;
    let dir_count = read_dir(&dir)?.count();
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
    let dir_count = read_dir(dir)?.count();
    let dir_entries = read_dir(dir)?
        .progress_count(dir_count as u64)
        .filter_map(|res| res.map(|e| e.path()).ok())
        .filter(|res| res.is_dir())
        .filter_map(|p| p.file_name().and_then(|p| p.to_str()).map(String::from))
        .filter(|r| !r.contains('-') && !r.contains("Best") && !r.contains("Greatest"))
        .map(|i| i.replace(CHARS_TO_REMOVE, ""))
        .map(|s| s.to_ascii_lowercase())
        .collect::<HashSet<String>>();

    let config = Config::read()?;
    let artist_in_config = config
        .artist_full
        .into_iter()
        .map(|a| a.name)
        .map(|i| i.replace(CHARS_TO_REMOVE, ""))
        .map(|i| i.to_ascii_lowercase())
        .collect::<HashSet<String>>();

    // remove things that we do not need
    //let c: HashSet<String> = HashSet::from_iter(artist_in_config.iter().cloned());
    println!("{:?}", artist_in_config);
    let ignore = HashSet::from_iter(config.ignore_paths.iter());
    let mut res = dir_entries
        .difference(&artist_in_config)
        .collect::<HashSet<&String>>()
        .difference(&ignore)
        .cloned()
        .collect::<Vec<&String>>();

    println!("artists found in directory but not config");
    res.sort_unstable();

    for i in res {
        println!("\"{}\"", i);
    }
    Ok(())
}

fn get_specific_artists(str: &str, ratelimiter: &Ratelimiter) -> Result<()> {
    let client = get_client()?;
    let artist = Artist::new(&client, str, ratelimiter)?;
    println!("Foudn artist {}", artist.name);
    let mut albums = artist.get_albums_basic_filtered(&client, ratelimiter)?;
    albums.sort_by_cached_key(|a| a.date);

    for i in albums {
        let format = format_description::parse("[year]-[month]-[day]")?;
        let date: String = i
            .date
            .and_then(|d| d.format(&format).ok())
            .unwrap_or_else(|| "NONE".to_string());
        println!("{} - {}", date.red(), i.title.green());
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

    /// Add To Ignore List
    Ignore { name: PathBuf },

    /// Bump date back by number of days
    BumpBack { days: u64 },
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

    /// Search a specific artist and print complete discography
    #[clap(short = 's', long)]
    artist: Option<String>,
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

fn run_subcommand(cmd: SubCommands, ratelimiter: Ratelimiter) -> Result<(), anyhow::Error> {
    let mut c = Config::read()?;
    Ok(match cmd {
        SubCommands::Add { name } => {
            let client = get_client()?;
            let new_artist = Artist::new(&client, &name, &ratelimiter)?;
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
            grab_new_releases(&ratelimiter)?;
        }
        SubCommands::Ignore { name } => {
            c.add_ignore(name)?;
        }
        SubCommands::BumpBack { days } => {
            let last_date = c.last_checked_time;
            c.last_checked_time -= Duration::new(60 * 60 * 24 * days, 0);
            println!(
                "Change date from |{}| to |{}|",
                last_date, c.last_checked_time
            );
            c.write()?;
        }
    })
}

fn main() -> Result<()> {
    let args = Args::parse();
    let ratelimiter = Ratelimiter::builder(30, Duration::from_secs(5))
        .max_tokens(30)
        .build()?;
    if let Some(path) = args.music_dir {
        println!("Getting artists");
        get_artists(path)?;
    } else if args.ids {
        get_artist_ids(&ratelimiter)?;
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
    } else if let Some(artist) = args.artist {
        get_specific_artists(&artist, &ratelimiter)?;
    } else if let Some(cmd) = args.artists {
        run_subcommand(cmd, ratelimiter)?;
    }

    Ok(())
}
