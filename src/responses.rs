use anyhow::Result;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct ArtistsResponse {
    id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SearchResponse {
    name: String,
    artists: Vec<ArtistsResponse>,
}

#[derive(Debug)]
pub(crate) struct Artist {
    pub(crate) name: String,
    pub(crate) id: String,
}

impl Artist {
    fn new(client: &Client, s: &str) -> Result<Self> {
        let resp: SearchResponse = client
            .get(format!(
                "https://musicbrainz.org/ws/2/artist/?query={}&limit=10&fmt=json",
                s
            ))
            .send()?
            .json()?;

        Ok(Artist {
            name: resp.name,
            id: resp.artists[0].id.clone(),
        })
    }

    fn get_albums(self, client: &Client) -> Result<Vec<Album>> {
        let resp: LookupResponse = client
            .get(format!(
                "https://musicbrainz.org/ws/2/release?artist={}&limit=100&fmt=json",
                self.id
            ))
            .send()?
            .json()?;
        Ok(resp
            .releases
            .into_iter()
            .map(|r| Album {
                title: r.title,
                date: r.date,
            })
            .collect())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct LookupResponse {
    releases: Vec<ReleasesResponse>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReleasesResponse {
    date: String,
    status: String,
    title: String,
}

#[derive(Debug)]
pub(crate) struct Album {
    pub(crate) title: String,
    pub(crate) date: String,
}
