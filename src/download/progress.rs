use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Progress {
    pub id: u64,
    pub title: String,
    pub state: State,
}

#[derive(Debug, Clone)]
pub enum State {
    Running { bytes_done: u64, bytes_total: u64 },
    Done { bytes_total: u64, path: PathBuf },
    Failed { reason: String },
    Cancelled,
}

impl Progress {
    pub fn running(id: u64, title: String, bytes_done: u64, bytes_total: u64) -> Self {
        Self {
            id,
            title,
            state: State::Running {
                bytes_done,
                bytes_total,
            },
        }
    }
    pub fn done(id: u64, title: String, bytes_total: u64, path: PathBuf) -> Self {
        Self {
            id,
            title,
            state: State::Done { bytes_total, path },
        }
    }
    pub fn failed(id: u64, title: String, reason: String) -> Self {
        Self {
            id,
            title,
            state: State::Failed { reason },
        }
    }
    pub fn cancelled(id: u64, title: String) -> Self {
        Self {
            id,
            title,
            state: State::Cancelled,
        }
    }
}

/// Turn an arbitrary show title into a kebab-case, ASCII-safe filename slug.
///
/// Applied the moment we first need to put bytes on disk, so the `.part`
/// file and the final file share the same slug — no rename surprises and
/// nothing weird (umlauts, spaces, punctuation) ever lands in `~/Videos`.
pub fn slugify(input: &str) -> String {
    const MAX_LEN: usize = 100;
    let mut out = String::with_capacity(input.len());
    // Start as "just emitted a dash" so any leading non-alphanumerics are
    // suppressed without an explicit trim afterwards.
    let mut last_was_dash = true;

    for ch in input.chars() {
        let german: Option<&str> = match ch {
            'ä' | 'Ä' => Some("ae"),
            'ö' | 'Ö' => Some("oe"),
            'ü' | 'Ü' => Some("ue"),
            'ß' => Some("ss"),
            _ => None,
        };
        if let Some(s) = german {
            out.push_str(s);
            last_was_dash = false;
            continue;
        }
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            out.push('-');
            last_was_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.len() > MAX_LEN {
        out.truncate(MAX_LEN);
        while out.ends_with('-') {
            out.pop();
        }
    }
    if out.is_empty() {
        "download".into()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::slugify;

    #[test]
    fn umlauts_get_ascii_equivalents() {
        assert_eq!(slugify("Tagesschau für Kinder"), "tagesschau-fuer-kinder");
        assert_eq!(slugify("Straße der Lieder"), "strasse-der-lieder");
    }

    #[test]
    fn punctuation_and_spaces_collapse_to_one_dash() {
        assert_eq!(slugify("heute-show: Spezial!"), "heute-show-spezial");
        assert_eq!(
            slugify("Tagesschau in 100 Sekunden – 23.05.2026"),
            "tagesschau-in-100-sekunden-23-05-2026"
        );
    }

    #[test]
    fn leading_and_trailing_junk_stripped() {
        assert_eq!(slugify("  …Hallo Welt…  "), "hallo-welt");
    }

    #[test]
    fn empty_or_unmappable_falls_back() {
        assert_eq!(slugify(""), "download");
        assert_eq!(slugify("———"), "download");
    }
}
