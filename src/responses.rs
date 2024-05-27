use std::fmt::{self, Display};

use anyhow::{anyhow, Context, Result};
use ratelimit::Ratelimiter;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use time::{format_description, Date};
use uuid::Uuid;

const HOW_MANY_RELEASE_RESULT: i32 = 100;

/// Json response for an artist
#[derive(Debug, Serialize, Deserialize)]
struct ArtistsResponse {
    id: String,
    name: String,
}

/// JSON response for a search query
#[derive(Debug, Serialize, Deserialize)]
struct SearchResponse {
    artists: Vec<ArtistsResponse>,
}

/// Artist from musicbrainz
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
    /// the uuid on musicbrainz
    pub(crate) id: Uuid,
    /// the artist
    pub(crate) artist: String,
    /// the title of the album
    pub(crate) title: String,
    /// the date of the album
    pub(crate) date: Option<Date>,
    /// type of the release
    pub(crate) release_type: ReleaseType,
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
    /// Search for an artist given by string `s` and construct an artist object
    pub(crate) fn new(client: &Client, s: &str, ratelimit: &Ratelimiter) -> Result<Self> {
        let s_rep = String::from(s).replace(' ', "%20");
        for _ in 0..10 {
            if let Err(sleep) = ratelimit.try_wait() {
                std::thread::sleep(sleep);
                continue;
            }
        }
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

    /// Get albums for this artist
    fn get_albums(
        &self,
        client: &Client,
        ratelimit: &Ratelimiter,
    ) -> Result<Vec<ReleasesResponse>> {
        let mut all_releases = Vec::new();

        for _ in 0..10 {
            if let Err(sleep) = ratelimit.try_wait() {
                std::thread::sleep(sleep);
                continue;
            }
        }

        let mut resp: LookupResponse = client
            .get(format!(
                "https://musicbrainz.org/ws/2/release?artist={artist_id}&limit={limit}&fmt=json&inc=release-groups",
                artist_id = self.id,
                limit = HOW_MANY_RELEASE_RESULT
            ))
            .send()
            .context("Error in getting albums")?
            .error_for_status()?
            .json()
            .context("Error in decoding albums")?;
        all_releases.append(&mut resp.releases);
        let total_results = resp.release_count.unwrap_or(0);
        while all_releases.len() < total_results {
            for _ in 0..10 {
                if let Err(sleep) = ratelimit.try_wait() {
                    std::thread::sleep(sleep);
                    continue;
                }
            }
            let response = client.get(format!(
                "https://musicbrainz.org/ws/2/release?artist={artist_id}&offset={offset}&limit={limit}&fmt=json&inc=release-groups",
                artist_id = self.id,
                offset = all_releases.len(),
                limit = HOW_MANY_RELEASE_RESULT
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

    /// Filter albums by simple release type and returns the albums found
    /// Notice that this filters out also albums that do not have a specific year-month-day release date in the db
    pub(crate) fn get_albums_basic_filtered(
        &self,
        client: &Client,
        ratelimit: &Ratelimiter,
    ) -> Result<Vec<Album>> {
        let albs_resp = self.get_albums(client, ratelimit)?;
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
                    release_type: a
                        .release_group
                        .secondary_types
                        .first()
                        .unwrap_or(&ReleaseType::Album)
                        .to_owned(),
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

/// JSON response for the releases lookup
#[derive(Debug, Serialize, Deserialize, Clone)]
struct LookupResponse {
    #[serde(rename = "release-offset")]
    release_offset: Option<usize>,
    #[serde(rename = "release-count")]
    release_count: Option<usize>,
    releases: Vec<ReleasesResponse>,
}

/// JSON response for ReleaseGroup
#[derive(Debug, Serialize, Deserialize, Clone)]
struct ReleaseGroup {
    #[serde(rename = "primary-type")]
    primary_type: Option<ReleaseType>,
    #[serde(rename = "first-release-date")]
    first_release_date: Option<String>,
    #[serde(rename = "secondary-types")]
    secondary_types: Vec<ReleaseType>,
}

/// type/status of release
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

/// release type
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub(crate) enum ReleaseType {
    None,
    EP,
    Album,
    Single,
    Other,
    Broadcast,
    Compilation,
    Spokenword,
    Interview,
    Soundtrack,
    Audiobook,
    #[serde(rename = "DJ-mix")]
    DJMix,
    #[serde(rename = "Mixtape/Street")]
    Mixtape,
    Live,
    Remix,
    Demo,
}

impl Display for ReleaseType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Json response for release
#[derive(Debug, Serialize, Deserialize, Clone)]
struct ReleasesResponse {
    id: Uuid,
    date: Option<String>,
    status: Option<Status>,
    title: String,
    #[serde(rename = "release-group")]
    release_group: ReleaseGroup,
}
