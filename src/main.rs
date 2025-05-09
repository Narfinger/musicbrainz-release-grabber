use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use dialoguer::Confirm;
use indicatif::{ProgressBar, ProgressIterator, ProgressStyle};
use ratelimit::Ratelimiter;
use responses::{Album, Artist};
use std::collections::HashSet;
use std::time::Duration;
use std::{fs::read_dir, path::PathBuf, str::FromStr};
use time::format_description;
use yansi::Paint;

use crate::responses::ReleaseType;
use config::{Config, CHARS_TO_REMOVE};

mod config;
mod responses;
#[cfg(feature = "tui")]
mod tui;

/// Progress bar style
const PROGRESS_STYLE: &str =
    "[{spinner:.green}] [{pos:.green}/{len:.green}] ({percent:>2}%) {bar:40.cyan/blue} [ETA: {eta:>3}] |                 {msg}";

/// get the artists ids for all artists in artist_names
fn get_artist_ids(ratelimiter: &Ratelimiter) -> Result<()> {
    let client = get_client()?;
    let mut c = Config::read()?;

    if c.artist_names.is_empty() {
        println!("We do not have artist names, you need to add some");
        return Ok(());
    }

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
    pb.enable_steady_tick(Duration::from_millis(250));
    for i in pb.wrap_iter(artist_names.difference(&already_found_artists)) {
        pb.set_message(format!("Artist: {}", i));
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

struct AlbumResult {
    others: Vec<Album>,
    albums: Vec<Album>,
}

fn grab_new_releases(ratelimiter: &Ratelimiter) -> Result<AlbumResult> {
    let client = get_client()?;

    let c = Config::read()?;
    println!("Finding new albums from {}", c.last_checked_time);
    let pb = ProgressBar::new(c.artist_names.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(PROGRESS_STYLE)?
            .progress_chars("##-"),
    );
    pb.enable_steady_tick(std::time::Duration::new(0, 500));
    let mut errors = Vec::new();
    let mut all_albums: Vec<Album> = Vec::new();
    for a in pb.wrap_iter(c.artist_full.iter()) {
        pb.set_message(format!("Artist: {}", a.name));
        let res = a.get_albums_basic_filtered(&client, ratelimiter);
        match res {
            Ok(mut albums) => all_albums.append(&mut albums),
            Err(e) => errors.push(e),
        };
    }
    if !errors.is_empty() {
        println!("Could not get all artists. Please check manually the following:");
        for i in errors {
            println!("{:#}", i);
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
        .cloned()
        .collect::<Vec<Album>>();
    let albums = res
        .into_iter()
        .filter(|a| a.release_type == ReleaseType::Album)
        .cloned()
        .collect::<Vec<Album>>();
    Ok(AlbumResult { others, albums })
}

/// check for releases later then last checked date from artist_full
fn print_new_releases(albums: AlbumResult) -> Result<()> {
    println!("Printing {} Others", albums.others.len());
    print_new_albums(&albums.others)?;
    println!("---------------------------------------------------------");
    println!("Printing {} Albums", albums.albums.len());
    print_new_albums(&albums.albums)?;
    let mut c = Config::read()?;
    c.previous = albums.albums;
    c.write()?;

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
fn print_new_albums(a: &[Album]) -> Result<()> {
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
fn get_artists_from_directory(dir: PathBuf) -> Result<()> {
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
        .map(|a| a.sort_name)
        .map(|i| i.replace(CHARS_TO_REMOVE, ""))
        .map(|i| i.to_ascii_lowercase())
        .collect::<HashSet<String>>();

    // remove things that we do not need
    //let c: HashSet<String> = HashSet::from_iter(artist_in_config.iter().cloned());
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

fn get_specific_artist_id(str: &str, ratelimiter: &Ratelimiter) -> Result<()> {
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
    /// Initiale Setup
    Init {
        /// Give a directory to parse artist names
        #[arg(short, long, value_parser =valid_dir, value_name = "DIR", group = "init")]
        dir: Option<PathBuf>,
        /// should we fill the artists
        #[arg(short, long, group = "init")]
        fill_ids: bool,
        /// Clear config values
        #[clap(short, long, value_enum, group = "init")]
        clear: Option<ClearValues>,
    },

    /// Adds an artist to our list
    Add { name: String },

    /// List artists
    List,

    /// Delete an artist or a list of artists
    Delete { names: Vec<String> },

    /// Find new albums
    New,

    /// Add To Ignore List
    Ignore { name: PathBuf },

    /// Bump date back by number of days
    BumpBack { days: u64 },

    /// List the previous albums
    Previous,

    /// Same as previous
    History,

    /// Artists not in config
    NotInConfig {
        #[clap(value_parser = valid_dir, value_name = "DIR")]
        path: PathBuf,
    },

    /// Search a specific artist and print complete discography
    Discography { artist_search: String },

    /// Searches if an artist is in the config
    ConfigSearch { artist_search: String },

    /// First gets the new ones, combines them with the old ones and puts them in a nice tui
    #[cfg(feature = "tui")]
    Tui,
}

/// Arguments for the program
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(subcommand)]
    commands: Option<SubCommands>,
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
    match cmd {
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
        SubCommands::Delete { names } => {
            for name in names {
                if let Some(index) = c.artist_full.iter().position(|a| a.name == name) {
                    println!("{} {}", "Removing".green(), name);
                    c.artist_full.remove(index);
                } else {
                    println!("{} {}", "Did not find:".red(), name);
                }
            }
            c.write()?;
        }
        SubCommands::New => {
            if c.artist_full.is_empty() {
                println!("We do not have any artists, did you forget to run init -f?");
                return Ok(());
            }
            let album_result = grab_new_releases(&ratelimiter)?;
            print_new_releases(album_result)?;
            c.now()?;
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
        SubCommands::Previous | SubCommands::History => {
            println!("Last checked on {}", c.last_checked_time);
            println!("---------------------------------------------------------");
            print_new_albums(&c.previous)?;
        }
        SubCommands::Init {
            dir,
            fill_ids,
            clear,
        } => {
            if dir.is_none() && !fill_ids && clear.is_none() {
                println!("Use at least one init argument");
                println!("Try init -h");
                return Ok(());
            }

            if let Some(d) = dir {
                let confirmation = Confirm::new()
                    .default(false)
                    .with_prompt("This will delete the whole configuration")
                    .interact()
                    .unwrap();
                if confirmation {
                    get_artists_from_directory(d)?;
                }
            } else if fill_ids {
                get_artist_ids(&ratelimiter)?;
            } else if let Some(cl) = clear {
                let mut c = Config::read()?;
                let confirm_string = match cl {
                    ClearValues::Ids => {
                        c.artist_full = vec![];
                        "This will clear all artist ids!"
                    }
                    ClearValues::Artists => {
                        c.artist_names = vec![];
                        "This will clear all artist names."
                    }
                    ClearValues::WholeConfig => {
                        c = Config::default();
                        "This will clear the whole configuration!"
                    }
                };
                let confirmation = Confirm::new()
                    .default(false)
                    .with_prompt(confirm_string)
                    .interact()
                    .unwrap();
                if confirmation {
                    return c.write();
                }
            }
        }
        SubCommands::NotInConfig { path } => {
            artists_not_in_config(&path)?;
        }
        SubCommands::Discography { artist_search } => {
            get_specific_artist_id(&artist_search, &ratelimiter)?;
        }
        SubCommands::ConfigSearch { artist_search } => {
            let artist_found = c.artist_full.iter().find(|p| {
                p.name.contains(&artist_search) || p.search_string.contains(&artist_search)
            });
            if let Some(a) = artist_found {
                println!("Found artist {}", a.name);
            } else {
                println!("Artist not found");
            }
        }

        #[cfg(feature = "tui")]
        SubCommands::Tui => {
            let previous_albums = c.previous;
            let album_result = grab_new_releases(&ratelimiter)?;

            tui::run(tui::InitTui {
                //new_albums: vec![],
                //new_other: vec![],
                new_albums: album_result.albums,
                new_other: album_result.others,
                old_albums: previous_albums,
            })?;
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let ratelimiter = Ratelimiter::builder(30, Duration::from_secs(5))
        .max_tokens(30)
        .build()?;
    if let Some(cmd) = args.commands {
        run_subcommand(cmd, ratelimiter)?;
    }

    Ok(())
}
