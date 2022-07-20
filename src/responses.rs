use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct ArtistsResponse {
    id: String,
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SearchResponse {
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
            name: resp.artists[0].name.clone(),
            id: resp.artists[0].id.clone(),
        })
    }

    fn get_albums(self, client: &Client) -> Result<Vec<ReleasesResponse>> {
        let mut all_releases = Vec::new();
        let mut resp: LookupResponse = client
            .get(format!(
                "https://musicbrainz.org/ws/2/release?artist={}&limit=100&fmt=json&inc=release-groups",
                self.id
            ))
            .send()
            .context("Error in getting albums")?
            .json()
            .context("Error in decoding albums")?;
        all_releases.append(&mut resp.releases);
        let total_results = resp.release_count.unwrap_or(0);
        while all_releases.len() < total_results {
            let mut resp: LookupResponse = client
            .get(format!(
                "https://musicbrainz.org/ws/2/release?artist={}&offset={}limit=100&fmt=json&inc=release-groups",
                all_releases.len(),
                self.id
            ))
            .send()
            .context("Error in getting albums")?
            .json()
            .context("Error in decoding albums")?;
            all_releases.append(&mut resp.releases);
        }
        Ok(all_releases)
    }

    pub(crate) fn get_albums_basic_filtered(self, client: &Client) -> Result<Vec<Album>> {
        let albs_resp = self.get_albums(client)?;
        let mut albs = albs_resp
            .into_iter()
            .filter(|a| a.status == Status::Official)
            .filter(|a| a.release_group.primary_type == ReleaseType::Album)
            .map(|a| Album {
                title: a.title,
                date: a.date,
            })
            .collect::<Vec<_>>();
        albs.sort_by_key(|a| a.title.clone()); // this is necessary to remove all duplicated elements
        albs.dedup_by(|a, b| a.title.eq(&b.title));
        albs.sort_by_key(|a| a.date.clone());
        Ok(albs)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct LookupResponse {
    #[serde(rename = "release-count")]
    release_count: Option<usize>,
    releases: Vec<ReleasesResponse>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReleaseGroup {
    #[serde(rename = "primary-type")]
    primary_type: ReleaseType,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
enum Status {
    Official,
    Promotion,
    Bootleg,
    Pseudo_Release,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
enum ReleaseType {
    EP,
    Album,
    Single,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReleasesResponse {
    date: String,
    status: Status,
    title: String,
    #[serde(rename = "release-group")]
    release_group: ReleaseGroup,
}

#[derive(Debug)]
pub(crate) struct Album {
    pub(crate) title: String,
    pub(crate) date: String,
}
