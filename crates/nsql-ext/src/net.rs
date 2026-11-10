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

/// 콘솔 창 없이 외부 명령 준비(3-OS · 외부 crate 0) — GUI(`windows_subsystem = "windows"`) 프로세스가 콘솔 앱(`curl`·`git`·`ssh`)을
/// 띄우면 Windows가 새 콘솔 창을 만들어 **까만 창이 깜빡인다**(사용자 11-10 "확장 탭·확장 항목 클릭 때 까만 화면"). `CREATE_NO_WINDOW`
/// 로 막는다 — gitstat·tunnel과 같은 처방을 부품 하나로(30 §2 · 세 번째 발견). 다른 OS는 그대로.
#[must_use]
pub fn quiet_command(program: &str) -> std::process::Command {
    #[allow(unused_mut)]
    let mut cmd = std::process::Command::new(program); // console-ok: 부품 자신(아래 CREATE_NO_WINDOW)
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.stdin(std::process::Stdio::null());
    cmd
}

/// 절대 URL GET(curl · 30초 · 콘솔 창 없이).
pub fn fetch_url(url: &str) -> Result<Vec<u8>, String> {
    let out = quiet_command("curl")
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

    /// ★ 콘솔 창 금지(사용자 11-10 · 61 §2-2) — GUI 쪽 크레이트의 `Command::new(`는 `quiet_command`를 쓰거나, 콘솔이 생길 수
    ///   없는 자리임을 같은 줄·바로 윗줄의 `// console-ok: <이유>`로 밝혀야 한다. `nsql-cli`는 콘솔 앱 자신이라 제외.
    #[test]
    fn no_console_window_spawns() {
        let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut bad = Vec::new();
        for e in std::fs::read_dir(&crates).expect("crates dir").flatten() {
            if e.file_name().to_string_lossy() == "nsql-cli" {
                continue;
            }
            let src = e.path().join("src");
            if src.is_dir() {
                scan_console_spawns(&src, &mut bad);
            }
        }
        assert!(
            bad.is_empty(),
            "Command::new without `// console-ok:` marker — use nsql_ext::net::quiet_command (rule 61 §2-2):
{}",
            bad.join("
")
        );
    }

    fn scan_console_spawns(dir: &std::path::Path, bad: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).expect("read_dir").flatten() {
            let p = e.path();
            if p.is_dir() {
                scan_console_spawns(&p, bad);
                continue;
            }
            if p.extension().and_then(|x| x.to_str()) != Some("rs") {
                continue;
            }
            let text = std::fs::read_to_string(&p).expect("read source");
            let lines: Vec<&str> = text.lines().collect();
            for (i, l) in lines.iter().enumerate() {
                // 바늘은 쪼개 둔다 — 이 줄 자신이 검사에 걸리지 않게.
                let needle = concat!("Command", "::new(");
                if !l.contains(needle) || l.trim_start().starts_with("//") {
                    continue;
                }
                let ok = l.contains("console-ok:") || i > 0 && lines[i - 1].contains("console-ok:");
                if !ok {
                    bad.push(format!("{}:{}", p.display(), i + 1));
                }
            }
        }
    }
}
