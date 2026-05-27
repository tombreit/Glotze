pub mod models;

use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

use models::Show;

const QUERY_URL: &str = "https://mediathekviewweb.de/api/query";

/// How the result list is ordered. `MediathekViewWeb` honours `sortBy` for the
/// empty-query "recent" view, but free-text queries come back relevance-ranked,
/// so the same order is also enforced client-side via [`Sort::apply`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    #[default]
    DateNewest,
    DateOldest,
    DurationLongest,
    DurationShortest,
}

impl Sort {
    /// The (`sortBy`, `sortOrder`) pair sent in the request.
    fn request_fields(self) -> (&'static str, &'static str) {
        match self {
            Sort::DateNewest => ("timestamp", "desc"),
            Sort::DateOldest => ("timestamp", "asc"),
            Sort::DurationLongest => ("duration", "desc"),
            Sort::DurationShortest => ("duration", "asc"),
        }
    }

    /// Stable string id, used as the `win.sort` action's state.
    pub fn id(self) -> &'static str {
        match self {
            Sort::DateNewest => "date-newest",
            Sort::DateOldest => "date-oldest",
            Sort::DurationLongest => "duration-longest",
            Sort::DurationShortest => "duration-shortest",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "date-newest" => Some(Sort::DateNewest),
            "date-oldest" => Some(Sort::DateOldest),
            "duration-longest" => Some(Sort::DurationLongest),
            "duration-shortest" => Some(Sort::DurationShortest),
            _ => None,
        }
    }

    /// Sort `shows` in place by the chosen key/order. Shows missing the sort
    /// key always sort last, regardless of direction.
    pub fn apply(self, shows: &mut [Show]) {
        match self {
            Sort::DateNewest => {
                shows.sort_by_key(|s| std::cmp::Reverse(s.timestamp.unwrap_or(i64::MIN)));
            }
            Sort::DateOldest => {
                shows.sort_by_key(|s| s.timestamp.unwrap_or(i64::MAX));
            }
            Sort::DurationLongest => {
                shows.sort_by_key(|s| std::cmp::Reverse(s.duration.unwrap_or(0)));
            }
            Sort::DurationShortest => {
                shows.sort_by_key(|s| s.duration.unwrap_or(u64::MAX));
            }
        }
    }
}

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
    pub fn search(&self, query: &str, offset: u32, size: u32, sort: Sort) -> Result<Vec<Show>> {
        let is_recent = query.trim().is_empty();
        let queries = if is_recent {
            Vec::new()
        } else {
            vec![QueryField {
                fields: vec!["title".into(), "topic".into()],
                query: query.to_string(),
            }]
        };
        let (sort_by, sort_order) = sort.request_fields();
        let body = QueryRequest {
            queries,
            sort_by: sort_by.into(),
            sort_order: sort_order.into(),
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
        let mut results = answer.result.map(|r| r.results).unwrap_or_default();
        // `sortBy` in the request is honored for the empty-query "recent" view
        // but free-text queries fall back to relevance ranking, so enforce the
        // chosen order client-side too.
        sort.apply(&mut results);
        Ok(results)
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
