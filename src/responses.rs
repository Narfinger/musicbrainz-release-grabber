use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use time::{Date, format_description};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
struct ArtistsResponse {
    id: String,
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SearchResponse {
    artists: Vec<ArtistsResponse>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Artist {
    pub(crate) name: String,
    pub(crate) id: Uuid,
}

impl Artist {
    pub(crate) fn new(client: &Client, s: &str) -> Result<Self> {
        let s = String::from(s).replace(' ', "%20");
        let resp: SearchResponse = client
            .get(format!(
                "https://musicbrainz.org/ws/2/artist/?query={}&limit=3&fmt=json",
                s
            ))
            .send()
            .context("Error in getting artist id")?
            .json()
            .context("Error in decoding artist id response")?;

        let id = Uuid::parse_str(&resp.artists[0].id)?;
        Ok(Artist {
            name: resp.artists[0].name.clone(),
            id,
        })
    }

    fn get_albums(&self, client: &Client) -> Result<Vec<ReleasesResponse>> {
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
            let response = client.get(format!(
                "https://musicbrainz.org/ws/2/release?artist={}&offset={}&limit=100&fmt=json&inc=release-groups",
                self.id,
                all_releases.len(),
            ))
            .send()
            .context("Error in getting albums step")?;
            if !response.status().is_success() {
                return Err(anyhow!("Found wrong status, code {}", response.status()));
            }

            let mut resp: LookupResponse = response.json().context("Error in decoding albums")?;
            println!(
                "we have done: offset {}, release offset {}",
                all_releases.len(),
                resp.release_offset.unwrap_or(0)
            );
            all_releases.append(&mut resp.releases);
        }
        Ok(all_releases)
    }

    pub(crate) fn get_albums_basic_filtered(&self, client: &Client) -> Result<Vec<Album>> {
        let albs_resp = self.get_albums(client)?;
        let format = format_description::parse("[year]-[month]-[day]")?;
        let mut albs = albs_resp
            .into_iter()
            .filter(|a| a.status == Status::Official)
            .filter(|a| a.release_group.primary_type == ReleaseType::Album)
            .filter(|a| a.date.is_some())
            .map(|a| Album {
                artist: self.name.to_owned(),
                title: a.title,
                date: a.date.and_then(|d| Date::parse(&d, &format).ok()),
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
    #[serde(rename = "release-offset")]
    release_offset: Option<usize>,
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
    Other,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReleasesResponse {
    date: Option<String>,
    status: Status,
    title: String,
    #[serde(rename = "release-group")]
    release_group: ReleaseGroup,
}

#[derive(Debug)]
pub(crate) struct Album {
    pub(crate) artist: String,
    pub(crate) title: String,
    pub(crate) date: Option<Date>,
}
