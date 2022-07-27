use std::{thread, time::Duration};

use anyhow::{anyhow, bail, Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use time::{format_description, Date};
use uuid::Uuid;

/// Timeout we do between connections.
/// This is intentionally large.
pub(crate) const TIMEOUT: Duration = Duration::from_millis(1500);

#[derive(Debug, Serialize, Deserialize)]
struct ArtistsResponse {
    id: String,
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SearchResponse {
    artists: Vec<ArtistsResponse>,
}

/// Artists from musicbrainz
#[derive(Debug, Serialize, Deserialize, Eq)]
pub(crate) struct Artist {
    /// Artist String from musicbrainz
    pub(crate) name: String,
    /// Musicbrainz Artist UUID
    pub(crate) id: Uuid,
    /// The original search string, i.e., the directory. Good to see where our search went wrong
    pub(crate) search_string: String,
}

impl PartialEq for Artist {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl PartialOrd for Artist {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.name.partial_cmp(&other.name)
    }
}

impl Ord for Artist {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name.cmp(&other.name)
    }
}

/// Album that got released
#[derive(Debug, Eq)]
pub(crate) struct Album {
    pub(crate) id: Uuid,
    pub(crate) artist: String,
    pub(crate) title: String,
    pub(crate) date: Option<Date>,
}

impl PartialEq for Album {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl PartialOrd for Album {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.artist
            .partial_cmp(&other.artist)
            .or_else(|| self.date.partial_cmp(&other.date))
            .or_else(|| self.title.partial_cmp(&other.title))
    }
}

impl Ord for Album {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.artist
            .cmp(&other.artist)
            .then(self.date.cmp(&other.date))
            .then(self.title.cmp(&other.title))
    }
}

impl Artist {
    pub(crate) fn new(client: &Client, s: &str) -> Result<Self> {
        let s_rep = String::from(s).replace(' ', "%20");
        let resp: SearchResponse = client
            .get(format!(
                "https://musicbrainz.org/ws/2/artist/?query={}&limit=3&fmt=json",
                s_rep
            ))
            .send()
            .context("Error in getting artist id")?
            .error_for_status()
            .context("Error in getting status")?
            .json()
            .context("Error in decoding artist id response")?;

        if resp.artists.is_empty() {
            Err(anyhow!("could not find UUID for {}", s))
        } else {
            let id = Uuid::parse_str(&resp.artists[0].id).context("Error in parsing uuid")?;
            Ok(Artist {
                name: resp.artists[0].name.clone(),
                id,
                search_string: s.to_owned(),
            })
        }
    }

    fn get_albums(&self, client: &Client) -> Result<Vec<ReleasesResponse>> {
        let mut all_releases = Vec::new();

        if false {
            let response = client
            .get(format!(
                "https://musicbrainz.org/ws/2/release?artist={}&limit=100&fmt=json&inc=release-groups",
                self.id
            ))
            .send()
            .context("Error in getting albums")?
            .error_for_status()?;
            let res_cl = response.text();
            let res_cl = res_cl.unwrap();
            let jd = &mut serde_json::Deserializer::from_str(&res_cl);
            let other_res: Result<LookupResponse, _> = serde_path_to_error::deserialize(jd);
            if let Err(e) = other_res {
                println!("{} {}", e.path(), e);
                bail!("this is error");
            }
        }

        let mut resp: LookupResponse = client
            .get(format!(
                "https://musicbrainz.org/ws/2/release?artist={}&limit=100&fmt=json&inc=release-groups",
                self.id
            ))
            .send()
            .context("Error in getting albums")?
            .error_for_status()?
            .json()
            .context("Error in decoding albums")?;
        all_releases.append(&mut resp.releases);
        let total_results = resp.release_count.unwrap_or(0);
        while all_releases.len() < total_results {
            thread::sleep(TIMEOUT);
            let response = client.get(format!(
                "https://musicbrainz.org/ws/2/release?artist={}&offset={}&limit=100&fmt=json&inc=release-groups",
                self.id,
                all_releases.len(),
            ))
            .send()
            .context("Error in getting albums step")?
            .error_for_status()
            .context("Error in getting status code")?;

            let mut resp: LookupResponse = response.json().context("Error in decoding albums")?;
            all_releases.append(&mut resp.releases);
        }

        Ok(all_releases)
    }

    pub(crate) fn get_albums_basic_filtered(&self, client: &Client) -> Result<Vec<Album>> {
        let albs_resp = self.get_albums(client)?;
        let format = format_description::parse("[year]-[month]-[day]")?;
        let mut albs = albs_resp
            .into_iter()
            .filter(|a| a.status == Some(Status::Official))
            .filter(|a| a.release_group.primary_type == Some(ReleaseType::Album))
            .map(|a: ReleasesResponse| {
                let date = a
                    .release_group
                    .first_release_date
                    .or(a.date)
                    .and_then(|s| Date::parse(&s, &format).ok());
                Album {
                    id: a.id,
                    artist: self.name.to_owned(),
                    title: a.title,
                    date,
                }
            })
            .filter(|a| a.date.is_some())
            .collect::<Vec<_>>();
        albs.sort_by_key(|a| a.title.clone()); // this is necessary to remove all duplicated elements
        albs.dedup_by(|a, b| a.title.eq(&b.title));
        albs.sort_by_key(|a| a.date);
        Ok(albs)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct LookupResponse {
    #[serde(rename = "release-offset")]
    release_offset: Option<usize>,
    #[serde(rename = "release-count")]
    release_count: Option<usize>,
    releases: Vec<ReleasesResponse>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ReleaseGroup {
    #[serde(rename = "primary-type")]
    primary_type: Option<ReleaseType>,
    #[serde(rename = "first-release-date")]
    first_release_date: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
enum Status {
    Official,
    Promotion,
    Bootleg,
    #[serde(rename = "Pseudo-Release")]
    PseudoRelease,
    Withdrawn,
    Cancelled,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
enum ReleaseType {
    EP,
    Album,
    Single,
    Other,
    Broadcast,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ReleasesResponse {
    id: Uuid,
    date: Option<String>,
    status: Option<Status>,
    title: String,
    #[serde(rename = "release-group")]
    release_group: ReleaseGroup,
}
