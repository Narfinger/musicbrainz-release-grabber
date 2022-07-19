use serde::{Serialize, Deserialize};

#[derive(Serialize,Deserialize)]
pub(crate) struct Pagination {
    pub(crate) per_page: usize,
    pub(crate) items: usize,
    pub(crate) page: usize,
    pub(crate) urls: String,
    pub(crate) pages: usize,
}

#[derive(Serialize,Deserialize)]
pub(crate) struct ArtistsRelease {
    pub(crate) artist: String,
    pub(crate) id: usize,
    pub(crate) main_release: usize,
    pub(crate) resource_url: String,
    pub(crate) role: String,
    pub(crate) thumb: String,
    pub(crate) title: String,
    pub(crate) year: usize,
}

pub(crate) struct ArtistReleasesResponse {
    pub(crate) pagination: Pagination,
    pub(crate) releases: Vec<ArtistsRelease>,
}