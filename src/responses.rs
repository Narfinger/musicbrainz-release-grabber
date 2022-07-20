use reqwest::blocking::Client;
use serde::{Serialize, Deserialize};
use anyhow::Result;

#[derive(Debug, Serialize, Deserialize)]
struct ArtistsResponse {
    id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SearchResponse {
    name: String,
    artists: Vec<ArtistsResponse>
}

#[derive(Debug)]
pub(crate) struct Artist {
    pub(crate) name: String,
    pub(crate) id: String,
}

impl Artist {
    fn new(client: &Client, s: &str) -> Result<Self> {
        let resp: SearchResponse = client.get(format!("https://musicbrainz.org/ws/2/artist/?query={}&limit=10&fmt=json", s))
        .send()?
        .json()?;

        Ok(
            Artist {
                name: resp.name,
                id: resp.artists[0].id.clone(),
            }
        )
    }
}

const LOOKUP_URL: &str = "https://musicbrainz.org/ws/2/artist/?/releases?artist={}&limit=100&fmt=json";
#[derive(Debug, Serialize, Deserialize)]
struct LookupResponse {

}

#[derive(Debug)]
pub(crate) struct Album {
    pub(crate) title: String,
    pub(crate) year: String,
}