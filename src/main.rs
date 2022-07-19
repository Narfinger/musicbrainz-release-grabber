use anyhow::{Context, Result, anyhow};
use clap::Parser;
use indicatif::{ProgressIterator};
use reqwest::{blocking::Client, StatusCode};
use responses::SearchResponse;
use serde::{Deserialize, Serialize};
use ansi_term::Colour::{Blue, Red};
use std::{
    fs::{self, read_dir},
    path::PathBuf,
    str::FromStr,
};

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
    todo!()
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

fn get_artist_ids(client: Client) -> Result<()> {
    let mut c = Config::read()?;
    c.artist_ids = Vec::new();
    for i in c.artist_names.iter().progress() {
        let query_url = format!("https://api.discogs.com/database/search?q={}&artist", i);

        let resp = client
            .get(&query_url)
            .header(
                "Authorization",
                format!("Discogs key={}, secret={}", c.discogs_key, c.discogs_secret),
            )
            .send()?;
        println!("resp.status() {}", resp.status());
        if resp.status() != StatusCode::OK {
            println!(
                "Got status code {}, aborting the whole thing",
                Red.paint(resp.status().to_string())
            );

            println!("{}", Red.paint("Response is following"));
            let resp_string = format!("{:?}", client.get(&query_url).send().unwrap());
            println!("{}", Blue.paint(resp_string));
            return Err(anyhow!("Aborting"));
        }

        let search_response: SearchResponse = resp.json()?;
        c.artist_ids.push(search_response.result[0].id);
    }
    c.write()?;
    Ok(())
}

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Get the artists from a file
    #[clap(short, long, value_parser)]
    get_artists: Option<String>,

    /// update ids
    #[clap(short, long)]
    update_ids: bool,

    /// find new albums
    #[clap(short, long)]
    new: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let client = Client::builder()
        .user_agent("discogs-new-release-scraper")
        .build()?;
    if let Some(path) = args.get_artists {
        println!("Getting artists");
        let p = PathBuf::from_str(&path)?;
        get_artists(p)?;
    } else if args.update_ids {
        get_artist_ids(client)?;
    } else if args.new {
        grab_new_releases()?;
    } else {
        println!("Please use an argument");
    }

    Ok(())
}
