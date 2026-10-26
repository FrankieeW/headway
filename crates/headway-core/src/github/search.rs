//! Bounded repository search. Query values are encoded independently from the API path.
use super::GitHub;
use crate::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchRepository {
    pub full_name: String,
    pub html_url: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub stargazers_count: u64,
    #[serde(default)]
    pub fork: bool,
    #[serde(default)]
    pub archived: bool,
}

#[derive(Deserialize)]
struct SearchResponse {
    items: Vec<SearchRepository>,
}

impl GitHub {
    pub async fn search_repositories(
        &self,
        query: &str,
        per_page: u8,
    ) -> Result<Vec<SearchRepository>> {
        let encoded: String = reqwest::Url::parse("https://api.github.com/search/repositories")
            .map_err(|error| crate::Error::parse("repository search URL", &error))?
            .query_pairs_mut()
            .append_pair("q", query)
            .append_pair("per_page", &per_page.clamp(1, 10).to_string())
            .finish()
            .query()
            .unwrap_or_default()
            .to_owned();
        let response: SearchResponse = self
            .get_json(
                &format!("/search/repositories?{encoded}"),
                "repository search",
            )
            .await?;
        Ok(response.items)
    }
}
