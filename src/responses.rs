use anyhow::{Context, Result};
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
    pub(crate) fn new(client: &Client, s: &str) -> Result<Self> {
        let s = String::from(s).replace(" ", "%20");
        println!("{:?}", client.get(format!(
            "https://musicbrainz.org/ws/2/artist/?query={}&limit=3&fmt=json",
            s
        ))
        .send().unwrap().text());
        let resp: SearchResponse = client
            .get(format!(
                "https://musicbrainz.org/ws/2/artist/?query={}&limit=3&fmt=json",
                s
            ))
            .send()
            .context("Error in getting artist id")?
            .json()
            .context("Error in decoding artist id response")?;

        Ok(Artist {
            name: resp.name,
            id: resp.artists[0].id.clone(),
        })
    }

    fn get_albums(self, client: &Client) -> Result<Vec<ReleasesResponse>> {
        let resp: LookupResponse = client
            .get(format!(
                "https://musicbrainz.org/ws/2/release?artist={}&limit=100&fmt=json",
                self.id
            ))
            .send()
            .context("Error in getting albums")?
            .json()
            .context("Error in decoding albums")?;
        Ok(resp.releases)
    }

    pub(crate) fn get_albums_basic_filtered(self, client: &Client) -> Result<Vec<Album>> {
        let albs_resp = self.get_albums(client)?;
        let mut albs = albs_resp.into_iter().filter(|a| a.status==Status::Official).map(|a| Album {
            title: a.title,
            date: a.date,
        }).collect::<Vec<_>>();
        albs.dedup_by_key(|a| a.title.clone());
        albs.sort_by_key(|a| a.date.clone());
        Ok(albs)
    }

}

#[derive(Debug, Serialize, Deserialize)]
struct LookupResponse {
    releases: Vec<ReleasesResponse>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
enum Status {
    Official,
    Promotion,
    Bootleg,
    Pseudo_Release,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReleasesResponse {
    date: String,
    status: Status,
    title: String,
}

#[derive(Debug)]
pub(crate) struct Album {
    pub(crate) title: String,
    pub(crate) date: String,
}
