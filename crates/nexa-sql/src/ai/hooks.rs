//! ★ hooks(docs/110 §16 · 사용자 11-11 "MCP·hooks·skills·agents 연계"): 하네스가 **모델 없이 결정적으로** 끼워 넣는 단계. 지금 둘 다
//! 턴이 끝난 뒤(PostTurn) 어시스턴트 글에 적용 — `format_sql` = ```sql 블록을 앱 포매터(사용자 포맷 설정 그대로)로 · `tidy` = 빈 줄
//! 뭉치·끝 공백 정리(코드 블록 안은 손대지 않음). 설정 `ai.hook_format_sql` · `ai.hook_tidy`(기본 켬). 순수 함수(시험).
//! (PreTool 훅 = 승인·비밀 가림·행 데이터 동의는 `App::ai_tool_request`에 이미 있고 끌 수 없다.)

/// 글의 **마지막** ```sql 코드 블록 본문(없으면 None) — 턴 끝 자동 표시(`ai.show_result`)가 쓴다.
pub(crate) fn last_sql_block(text: &str) -> Option<String> {
    let mut last: Option<String> = None;
    let mut cur: Option<(bool, Vec<&str>)> = None;
    for line in text.lines() {
        let t = line.trim_end();
        if t.trim_start().starts_with("```") {
            match cur.take() {
                Some((is_sql, body)) => {
                    let b = body.join("\n");
                    if is_sql && !b.trim().is_empty() {
                        last = Some(b);
                    }
                }
                None => {
                    let lang = t
                        .trim_start()
                        .trim_start_matches('`')
                        .trim()
                        .to_ascii_lowercase();
                    let is_sql =
                        matches!(lang.as_str(), "sql" | "plsql" | "tsql" | "mysql" | "pgsql");
                    cur = Some((is_sql, Vec::new()));
                }
            }
        } else if let Some((_, body)) = cur.as_mut() {
            body.push(line);
        }
    }
    last
}

/// 턴 끝 후처리. `fmt` = SQL 한 덩이 → 포맷 결과(실패 = None → 원문 유지).
pub(crate) fn post_turn(
    text: &str,
    format_sql: bool,
    tidy: bool,
    fmt: &mut dyn FnMut(&str) -> Option<String>,
) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut in_code = false;
    let mut is_sql = false;
    let mut block: Vec<String> = Vec::new();
    let mut blank_run = 0usize;
    for line in text.lines() {
        let trimmed = line.trim_end();
        if !in_code && trimmed.starts_with("```") {
            in_code = true;
            let lang = trimmed.trim_start_matches('`').trim().to_ascii_lowercase();
            is_sql = lang == "sql"
                || lang == "plsql"
                || lang == "tsql"
                || lang == "mysql"
                || lang == "pgsql";
            block.clear();
            blank_run = 0;
            out.push(trimmed.to_string());
            continue;
        }
        if in_code {
            if trimmed.starts_with("```") {
                in_code = false;
                let body = block.join("\n");
                // ★ 문장 끝 `;` 보정(사용자 11-12 "쿼리에 항상 ;이 있었는데 빠졌다"): 모델이 끝 `;`을 빼고 쓰면 포매터의
                //   "`;`은 다음 줄" 규칙이 걸릴 것이 없어 결과에서도 빠졌다 — 포맷 전에 붙인다(`/`로 끝나는 PL/SQL 블록은 그대로).
                let body = if is_sql && format_sql {
                    let t = body.trim_end();
                    let src = if !t.is_empty() && !t.ends_with(';') && !t.ends_with('/') {
                        format!("{t}\n;")
                    } else {
                        body.clone()
                    };
                    // 포맷이 실패하면 원문 그대로(붙인 `;`도 넣지 않는다).
                    match fmt(&src) {
                        Some(f) if !f.trim().is_empty() => f.trim_end().to_string(),
                        _ => body,
                    }
                } else {
                    body
                };
                if !body.is_empty() {
                    out.push(body);
                }
                out.push("```".to_string());
            } else {
                block.push(line.to_string());
            }
            continue;
        }
        if tidy {
            if trimmed.is_empty() {
                blank_run += 1;
                if blank_run > 1 {
                    continue;
                }
            } else {
                blank_run = 0;
            }
            out.push(trimmed.to_string());
        } else {
            out.push(line.to_string());
        }
    }
    if in_code {
        // 닫히지 않은 블록(스트리밍 중단) = 그대로.
        out.extend(block);
    }
    let mut s = out.join("\n");
    if tidy {
        while s.ends_with('\n') {
            s.pop();
        }
    }
    if text.ends_with('\n') && !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_sql_block_picks_the_final_sql_fence() {
        let t = "a\n```sql\nSELECT 1\n```\nb\n```text\nx\n```\n```sql\nSELECT 2\nFROM t\n```\nend";
        assert_eq!(last_sql_block(t).as_deref(), Some("SELECT 2\nFROM t"));
        assert_eq!(last_sql_block("no code"), None);
        assert_eq!(last_sql_block("```python\nx\n```"), None);
    }

    #[test]
    fn formats_sql_blocks_and_tidies_prose() {
        let text = "Answer:\n\n\n\nSee below:  \n```sql\nselect a,b from t\n```\n\n\n\nDone.\n```python\nx  \n```\n";
        let mut calls = 0;
        let out = post_turn(text, true, true, &mut |s| {
            calls += 1;
            assert!(
                s.starts_with("select a,b from t") && s.trim_end().ends_with(';'),
                "{s}"
            );
            Some("SELECT a, b\nFROM t".into())
        });
        assert_eq!(calls, 1);
        assert_eq!(
            out,
            "Answer:\n\nSee below:\n```sql\nSELECT a, b\nFROM t\n```\n\nDone.\n```python\nx  \n```\n"
        );
        // 포매터 실패 = 원문 · 훅 둘 다 끔 = 그대로.
        let keep = post_turn("```sql\nselect 1\n```", true, false, &mut |_| None);
        assert_eq!(keep, "```sql\nselect 1\n```");
        assert_eq!(post_turn(text, false, false, &mut |_| None), text);
    }
}
