use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Show {
    pub id: Option<String>,
    pub title: String,
    pub topic: String,
    pub channel: String,
    pub timestamp: Option<i64>,
    #[serde(default, deserialize_with = "de_duration")]
    pub duration: Option<u64>,
    pub description: Option<String>,
    pub url_video: Option<String>,
    pub url_video_low: Option<String>,
    pub url_video_hd: Option<String>,
    // Subtitle and size aren't surfaced in the UI yet but the roadmap calls
    // for both (sidecar .vtt download and a pre-flight size hint on the row).
    #[allow(dead_code)]
    pub url_subtitle: Option<String>,
    pub url_website: Option<String>,
    #[allow(dead_code)]
    pub size: Option<u64>,
}

impl Show {
    pub fn url_for(&self, q: Quality) -> Option<&str> {
        let url = match q {
            Quality::Low => self.url_video_low.as_deref(),
            Quality::Medium => self.url_video.as_deref(),
            Quality::High => self.url_video_hd.as_deref(),
        };
        url.filter(|u| !u.is_empty() && !is_hls(u))
    }
}

/// `MediathekViewWeb` occasionally returns HLS playlists alongside the
/// progressive MP4. Glotze hands the user a file; HLS would need a separate
/// downloader to stitch segments. Match `.m3u8` either as the path suffix or
/// preceding a query string, case-insensitively.
fn is_hls(url: &str) -> bool {
    let path_end = url.find('?').unwrap_or(url.len());
    std::path::Path::new(&url[..path_end])
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("m3u8"))
}

fn de_duration<'de, D>(d: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // MediathekViewWeb sometimes returns duration as a string, sometimes as a number.
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        N(u64),
        F(f64),
        S(String),
        Null,
    }
    Ok(match Option::<Raw>::deserialize(d)? {
        Some(Raw::N(n)) => Some(n),
        Some(Raw::F(f)) => Some(f as u64),
        Some(Raw::S(s)) => s.parse().ok(),
        _ => None,
    })
}
