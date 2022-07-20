use ansi_term::Colour::{Blue, Green, Red};
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use indicatif::ProgressIterator;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fmt::Display,
    fs::{self, read_dir},
    path::PathBuf,
    str::FromStr,
};
use time::{Date, Month};

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

fn releases() -> Result<()> {
    let c = Config::read()?;
    todo!()
   /* let mut all_releases: Vec<Release> = Vec::new();
    for i in c.artist_names.iter() {
        let q = Artist::query_builder().name(i).build();

        let query_result = Artist::search(q).execute()?;
        let artist_id = query_result.entities[0].id.clone();
        let artist = Artist::fetch().id(&artist_id).with_releases().execute()?;

        println!("artist_id {}", artist_id);
        let mut albums: Vec<MyRelease> = artist
            .releases
            .unwrap()
            .into_iter()
            .filter(|a| a.status == Some(ReleaseStatus::Official))
            .map(|r: Release| r.into())
            .collect();
        //albums.dedup_by_key(|r| r.title.clone());

        for i in albums {
            i.pretty_print();
        }
    }
    Ok(all_releases)
     */
}

fn grab_new_releases() -> Result<()> {
    releases()?;
    //let rough_releases = releases(client)?;

    //filtering
    //let releases: HashSet<Release> = HashSet::from_iter(rough_releases.into_iter());

    //for i in releases {
    //    i.pretty_print();
    // }

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
