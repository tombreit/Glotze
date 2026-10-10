pub mod models;

use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use ureq::Agent;

use crate::net;
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
    http: Agent,
}

impl Client {
    pub fn new() -> Self {
        Self {
            http: net::agent(Some(Duration::from_secs(30)), None),
        }
    }

    /// Search `MediathekViewWeb`. Free-text matches both title and topic.
    ///
    /// An empty `query` is interpreted as "give me the most recent episodes":
    /// the `queries` array is sent empty and future broadcasts are excluded.
    /// This is what populates the cold-start view.
    pub fn search(&self, query: &str, size: u32, sort: Sort) -> Result<Vec<Show>> {
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
            offset: 0,
            size,
        };
        let body = serde_json::to_string(&body)?;

        // MediathekViewWeb requires `Content-Type: text/plain` even though the body is JSON
        // (see zapp's IMediathekApiService.kt:11).
        // ureq treats a non-2xx status as an error by default, so there is no
        // separate `error_for_status` step.
        let mut resp = self
            .http
            .post(QUERY_URL)
            .header("Content-Type", "text/plain")
            .send(body)
            .context("POST mediathekviewweb")?;

        let answer: Answer = resp
            .body_mut()
            .read_json()
            .context("decoding MVW response")?;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANSWER: &str = r#"{
        "result": {
            "results": [
                { "channel": "ARD", "topic": "Tatort", "title": "A", "timestamp": 200,
                  "duration": "5400", "url_video": "https://x/a.mp4", "size": 1 },
                { "channel": "ZDF", "topic": "heute", "title": "B", "timestamp": null,
                  "duration": 900 },
                { "channel": "3Sat", "topic": "nano", "title": "C", "timestamp": 100 }
            ],
            "queryInfo": { "resultCount": 3 }
        },
        "err": null
    }"#;

    fn titles(shows: &[Show]) -> Vec<&str> {
        shows.iter().map(|s| s.title.as_str()).collect()
    }

    #[test]
    fn parses_answer_ignoring_unknown_fields() {
        let answer: Answer = serde_json::from_str(ANSWER).unwrap();
        assert!(answer.err.is_none());
        let shows = answer.result.unwrap().results;
        assert_eq!(titles(&shows), ["A", "B", "C"]);
        assert_eq!(shows[0].duration, Some(5400));
    }

    #[test]
    fn sort_puts_missing_keys_last() {
        let mut shows = serde_json::from_str::<Answer>(ANSWER)
            .unwrap()
            .result
            .unwrap()
            .results;
        Sort::DateNewest.apply(&mut shows);
        assert_eq!(titles(&shows), ["A", "C", "B"]);
        Sort::DateOldest.apply(&mut shows);
        assert_eq!(titles(&shows), ["C", "A", "B"]);
        Sort::DurationLongest.apply(&mut shows);
        assert_eq!(titles(&shows), ["A", "B", "C"]);
        Sort::DurationShortest.apply(&mut shows);
        assert_eq!(titles(&shows), ["B", "A", "C"]);
    }

    #[test]
    fn sort_ids_round_trip() {
        for sort in [
            Sort::DateNewest,
            Sort::DateOldest,
            Sort::DurationLongest,
            Sort::DurationShortest,
        ] {
            assert_eq!(Sort::from_id(sort.id()), Some(sort));
        }
    }
}
