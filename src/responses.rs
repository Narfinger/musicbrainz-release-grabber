use serde::{Serialize, Deserialize};


const SEARCH_URL: &str = "https://musicbrainz.org/ws/2/artist/?query={}&limit=100&fmt=json";
#[derive(Debug, Serialize, Deserialize)]
struct SearchResponse {

}

#[derive(Debug)]
pub(crate) struct Artist {
    pub(crate) name: String,
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