use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    Low,
    Medium,
    High,
}

impl Quality {
    pub const ALL: [Quality; 3] = [Quality::Low, Quality::Medium, Quality::High];

    /// Short tag appended to download filenames so different-quality downloads
    /// of the same show don't overwrite each other; also names the UI toggle.
    pub fn tag(self) -> &'static str {
        match self {
            Quality::Low => "low",
            Quality::Medium => "medium",
            Quality::High => "hd",
        }
    }
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
    pub url_website: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn show(extra: serde_json::Value) -> Show {
        let mut json = serde_json::json!({ "topic": "t", "title": "t", "channel": "c" });
        json.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn duration_accepts_number_float_string_and_null() {
        let dur = |v: serde_json::Value| show(serde_json::json!({ "duration": v })).duration;
        assert_eq!(dur(serde_json::json!(1800)), Some(1800));
        assert_eq!(dur(serde_json::json!(1800.7)), Some(1800));
        assert_eq!(dur(serde_json::json!("1800")), Some(1800));
        assert_eq!(dur(serde_json::json!("n/a")), None);
        assert_eq!(dur(serde_json::Value::Null), None);
        assert_eq!(show(serde_json::json!({})).duration, None);
    }

    #[test]
    fn url_for_skips_empty_and_hls() {
        let s = show(serde_json::json!({
            "url_video": "https://x/v.mp4",
            "url_video_low": "",
            "url_video_hd": "https://x/master.M3U8?x=1",
        }));
        assert_eq!(s.url_for(Quality::Medium), Some("https://x/v.mp4"));
        assert_eq!(s.url_for(Quality::Low), None);
        assert_eq!(s.url_for(Quality::High), None);
    }
}
