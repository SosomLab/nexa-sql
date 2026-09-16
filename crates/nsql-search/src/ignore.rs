//! 무시 규칙 — 기본 폴더(`.git` `node_modules` `target` `.nexa-sql`) + `.gitignore` 부분집합(`*` `**` `?` `!` · 자체 매처 · 상위 폴더 규칙 상속).
//!
//! git 의미론 중 지원하는 것: 슬래시 없는 패턴 = 어느 깊이의 이름이든(`**/p`) · 슬래시 있으면 그 `.gitignore` 폴더 기준 고정 ·
//! 끝 `/` = 폴더만 · `!` = 되살리기 · `#` 주석 · 뒤 공백 무시 · 뒤에 나온 규칙이 우선 · 깊은 폴더의 파일이 얕은 것보다 우선.
//! 미지원(리터럴로 취급): `[abc]` 문자 집합 · `\` 이스케이프(`\#` `\!`만 지원).

use std::path::Path;
use std::sync::Arc;

/// 규칙 하나(패턴 조각 = `/`로 나눈 성분 · `**` 성분은 0개 이상의 폴더).
#[derive(Debug, Clone)]
struct Rule {
    segs: Vec<String>,
    negate: bool,
    dir_only: bool,
}

/// `.gitignore` 한 파일(또는 합성 규칙 묶음) — 상위 노드로 이어지는 사슬.
#[derive(Debug)]
pub(crate) struct IgnoreNode {
    rules: Vec<Rule>,
    /// 이 노드의 기준 폴더가 루트에서 몇 단계 아래인가(상대 경로 성분 자르기용).
    depth: usize,
    parent: Option<Arc<IgnoreNode>>,
}

/// 기본 무시 폴더(어느 깊이든 · 폴더만).
pub const DEFAULT_EXCLUDES: &[&str] = &[".git/", "node_modules/", "target/", ".nexa-sql/"];

/// 한 줄을 규칙으로(주석·빈 줄은 `None`).
fn parse_line(line: &str) -> Option<Rule> {
    let mut l = line.trim_end();
    if l.is_empty() || l.starts_with('#') {
        return None;
    }
    let mut negate = false;
    if let Some(rest) = l.strip_prefix('!') {
        negate = true;
        l = rest;
    } else if l.starts_with("\\!") || l.starts_with("\\#") {
        // `\!` `\#` = 리터럴.
        l = &l[1..];
    }
    let mut dir_only = false;
    if let Some(rest) = l.strip_suffix('/') {
        dir_only = true;
        l = rest;
    }
    if l.is_empty() {
        return None;
    }
    let anchored = l.starts_with('/') || l.contains('/');
    let l = l.trim_start_matches('/');
    let mut segs: Vec<String> = l.split('/').map(str::to_string).collect();
    if !anchored {
        segs.insert(0, "**".to_string());
    }
    Some(Rule {
        segs,
        negate,
        dir_only,
    })
}

/// 성분 하나의 글롭(`*` `?` · `/`는 넘지 않는다).
fn seg_match(pat: &[u8], name: &[u8]) -> bool {
    // 고전 백트래킹 와일드카드 — 패턴이 짧아 충분히 빠르다.
    let (mut p, mut n) = (0, 0);
    let (mut star_p, mut star_n) = (usize::MAX, 0);
    while n < name.len() {
        if p < pat.len() && (pat[p] == b'?' || pat[p] == name[n]) {
            p += 1;
            n += 1;
        } else if p < pat.len() && pat[p] == b'*' {
            star_p = p;
            star_n = n;
            p += 1;
        } else if star_p != usize::MAX {
            p = star_p + 1;
            star_n += 1;
            n = star_n;
        } else {
            return false;
        }
    }
    while p < pat.len() && pat[p] == b'*' {
        p += 1;
    }
    p == pat.len()
}

/// 패턴 성분열 vs 경로 성분열(`**` = 0개 이상의 성분).
fn path_match(segs: &[String], comps: &[&str]) -> bool {
    match segs.split_first() {
        None => comps.is_empty(),
        Some((first, rest)) if first == "**" => {
            (0..=comps.len()).any(|skip| path_match(rest, &comps[skip..]))
        }
        Some((first, rest)) => match comps.split_first() {
            Some((c, crest)) => {
                seg_match(first.as_bytes(), c.as_bytes()) && path_match(rest, crest)
            }
            None => false,
        },
    }
}

impl Rule {
    fn matches(&self, comps: &[&str], is_dir: bool) -> bool {
        if self.dir_only && !is_dir {
            return false;
        }
        path_match(&self.segs, comps)
    }
}

impl IgnoreNode {
    /// 패턴 줄들로 노드를 만든다(`depth` = 기준 폴더의 루트 기준 깊이).
    pub(crate) fn from_lines<'a>(
        lines: impl IntoIterator<Item = &'a str>,
        depth: usize,
        parent: Option<Arc<IgnoreNode>>,
    ) -> IgnoreNode {
        IgnoreNode {
            rules: lines.into_iter().filter_map(parse_line).collect(),
            depth,
            parent,
        }
    }

    /// `dir/.gitignore`가 있으면 읽어 자식 노드를 만든다(없거나 못 읽으면 `None`).
    pub(crate) fn load_child(
        dir: &Path,
        depth: usize,
        parent: &Arc<IgnoreNode>,
    ) -> Option<Arc<IgnoreNode>> {
        let text = std::fs::read_to_string(dir.join(".gitignore")).ok()?;
        let node = IgnoreNode::from_lines(text.lines(), depth, Some(Arc::clone(parent)));
        if node.rules.is_empty() {
            None
        } else {
            Some(Arc::new(node))
        }
    }

    /// 루트 기준 상대 성분 `comps`(마지막 = 이름)를 무시하는가. 깊은 노드부터, 노드 안에서는 뒤 규칙부터 본다.
    pub(crate) fn is_ignored(&self, comps: &[&str], is_dir: bool) -> bool {
        let mut node = Some(self);
        while let Some(n) = node {
            if comps.len() > n.depth {
                let rel = &comps[n.depth..];
                for r in n.rules.iter().rev() {
                    if r.matches(rel, is_dir) {
                        return !r.negate;
                    }
                }
            }
            node = n.parent.as_deref();
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(lines: &[&str]) -> IgnoreNode {
        IgnoreNode::from_lines(lines.iter().copied(), 0, None)
    }

    #[test]
    fn seg_glob() {
        assert!(seg_match(b"*.log", b"a.log"));
        assert!(!seg_match(b"*.log", b"a.logx"));
        assert!(seg_match(b"a?c", b"abc"));
        assert!(!seg_match(b"a?c", b"ac"));
        assert!(seg_match(b"*", b""));
        assert!(seg_match(b"x*y*z", b"x123y45z"));
        assert!(!seg_match(b"x*y*z", b"x123y45"));
    }

    #[test]
    fn gitignore_semantics() {
        let n = node(&[
            "# comment",
            "*.log",
            "!keep.log",
            "build/",
            "/root-only.txt",
            "docs/**/draft",
            "**/gen",
            "\\#literal",
        ]);
        assert!(n.is_ignored(&["a.log"], false));
        assert!(n.is_ignored(&["deep", "x", "b.log"], false));
        assert!(!n.is_ignored(&["deep", "keep.log"], false));
        assert!(n.is_ignored(&["build"], true));
        assert!(!n.is_ignored(&["build"], false), "build/는 폴더만");
        assert!(n.is_ignored(&["sub", "build"], true));
        assert!(n.is_ignored(&["root-only.txt"], false));
        assert!(
            !n.is_ignored(&["sub", "root-only.txt"], false),
            "/ 접두 = 고정"
        );
        assert!(n.is_ignored(&["docs", "draft"], false));
        assert!(n.is_ignored(&["docs", "a", "b", "draft"], false));
        assert!(!n.is_ignored(&["other", "draft"], false));
        assert!(n.is_ignored(&["x", "gen"], true));
        assert!(n.is_ignored(&["#literal"], false));
    }

    #[test]
    fn chain_child_overrides_parent() {
        let root = Arc::new(node(&["*.tmp", "secret/"]));
        let child = IgnoreNode::from_lines(["!*.tmp"], 1, Some(Arc::clone(&root)));
        // 자식 폴더(깊이 1) 안: *.tmp 되살아남 · secret/은 부모 규칙 그대로.
        assert!(!child.is_ignored(&["sub", "a.tmp"], false));
        assert!(child.is_ignored(&["sub", "secret"], true));
        assert!(root.is_ignored(&["a.tmp"], false));
    }

    #[test]
    fn defaults() {
        let n = node(DEFAULT_EXCLUDES);
        assert!(n.is_ignored(&["target"], true));
        assert!(n.is_ignored(&["a", "node_modules"], true));
        assert!(!n.is_ignored(&["target"], false));
        assert!(!n.is_ignored(&["targets"], true));
    }
}
