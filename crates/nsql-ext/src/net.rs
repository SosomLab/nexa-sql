//! 원격 저장소 읽기(`nsql ext verify <url>`) — `curl -fsSL`(3-OS 기본 탑재 · 외부 crate 0 · 26 §8: 사용자 동작으로만 트래픽). GUI
//! 매니저의 `fetch_url`과 같은 길(전송 통계 없이 바이트만).

/// 저장소 원천 — 폴더 또는 URL(`github.com/O/R/tree/B/P` → raw 주소로 · 매니저 `Source::parse`와 같은 규칙).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Dir(std::path::PathBuf),
    Url(String),
}

impl Source {
    #[must_use]
    pub fn parse(s: &str) -> Source {
        let s = s.trim().trim_end_matches('/');
        if let Some(rest) = s
            .strip_prefix("https://github.com/")
            .or_else(|| s.strip_prefix("http://github.com/"))
        {
            let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
            if parts.len() >= 2 {
                let (owner, repo) = (parts[0], parts[1]);
                let (branch, path) = if parts.get(2) == Some(&"tree") && parts.len() >= 4 {
                    (parts[3], parts[4..].join("/"))
                } else {
                    ("main", parts[2..].join("/"))
                };
                let mut u = format!("https://raw.githubusercontent.com/{owner}/{repo}/{branch}");
                if !path.is_empty() {
                    u.push('/');
                    u.push_str(&path);
                }
                return Source::Url(u);
            }
        }
        if s.starts_with("https://") || s.starts_with("http://") {
            Source::Url(s.to_string())
        } else {
            Source::Dir(std::path::PathBuf::from(s))
        }
    }

    /// 상대 경로 읽기(폴더 = fs · URL = curl).
    pub fn read(&self, rel: &str) -> Result<Vec<u8>, String> {
        match self {
            Source::Dir(p) => std::fs::read(p.join(rel)).map_err(|e| format!("{rel}: {e}")),
            Source::Url(u) => fetch_url(&format!("{u}/{rel}")),
        }
    }

    pub fn read_text(&self, rel: &str) -> Result<String, String> {
        String::from_utf8(self.read(rel)?).map_err(|_| format!("{rel}: not UTF-8"))
    }
}

/// 절대 URL GET(curl · 30초).
pub fn fetch_url(url: &str) -> Result<Vec<u8>, String> {
    let out = std::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "30", url])
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("{url}: {}", err.trim()));
    }
    Ok(out.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_tree_urls_become_raw() {
        assert_eq!(
            Source::parse("https://github.com/SosomLab/nexa-sql/tree/main/extensions/"),
            Source::Url(
                "https://raw.githubusercontent.com/SosomLab/nexa-sql/main/extensions".into()
            )
        );
        assert_eq!(
            Source::parse("https://github.com/O/R/ext"),
            Source::Url("https://raw.githubusercontent.com/O/R/main/ext".into())
        );
        assert_eq!(
            Source::parse("https://raw.githubusercontent.com/O/R/main/x"),
            Source::Url("https://raw.githubusercontent.com/O/R/main/x".into())
        );
        assert_eq!(Source::parse("C:/x/ext"), Source::Dir("C:/x/ext".into()));
    }
}
