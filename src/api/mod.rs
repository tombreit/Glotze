pub mod models;

use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

use models::Show;

const QUERY_URL: &str = "https://mediathekviewweb.de/api/query";

#[derive(Clone)]
pub struct Client {
    http: reqwest::blocking::Client,
}

impl Client {
    pub fn new() -> Result<Self> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(concat!("Glotze/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30))
            .build()
            .context("building HTTP client")?;
        Ok(Self { http })
    }

    /// Search `MediathekViewWeb`. Free-text matches both title and topic.
    ///
    /// An empty `query` is interpreted as "give me the most recent episodes":
    /// the `queries` array is sent empty and future broadcasts are excluded.
    /// This is what populates the cold-start view.
    pub fn search(&self, query: &str, offset: u32, size: u32) -> Result<Vec<Show>> {
        let is_recent = query.trim().is_empty();
        let queries = if is_recent {
            Vec::new()
        } else {
            vec![QueryField {
                fields: vec!["title".into(), "topic".into()],
                query: query.to_string(),
            }]
        };
        let body = QueryRequest {
            queries,
            sort_by: "timestamp".into(),
            sort_order: "desc".into(),
            // For the recent view we only want what already aired.
            future: !is_recent,
            offset,
            size,
            duration_min: None,
            duration_max: None,
        };
        let body = serde_json::to_string(&body)?;

        // MediathekViewWeb requires `Content-Type: text/plain` even though the body is JSON
        // (see zapp's IMediathekApiService.kt:11).
        let resp = self
            .http
            .post(QUERY_URL)
            .header("Content-Type", "text/plain")
            .body(body)
            .send()
            .context("POST mediathekviewweb")?
            .error_for_status()?;

        let answer: Answer = resp.json().context("decoding MVW response")?;
        if let Some(err) = answer.err {
            return Err(anyhow!("mediathekviewweb error: {err:?}"));
        }
        Ok(answer.result.map(|r| r.results).unwrap_or_default())
    }
}

#[derive(Serialize)]
struct QueryRequest {
    queries: Vec<QueryField>,
    #[serde(rename = "sortBy")]
    sort_by: String,
    #[serde(rename = "sortOrder")]
    sort_order: String,
    future: bool,
    offset: u32,
    size: u32,
    #[serde(rename = "duration_min", skip_serializing_if = "Option::is_none")]
    duration_min: Option<u64>,
    #[serde(rename = "duration_max", skip_serializing_if = "Option::is_none")]
    duration_max: Option<u64>,
}

#[derive(Serialize)]
struct QueryField {
    fields: Vec<String>,
    query: String,
}

#[derive(Deserialize)]
struct Answer {
    err: Option<serde_json::Value>,
    result: Option<AnswerResult>,
}

#[derive(Deserialize)]
struct AnswerResult {
    results: Vec<Show>,
    #[serde(rename = "queryInfo")]
    #[allow(dead_code)]
    query_info: Option<serde_json::Value>,
}
