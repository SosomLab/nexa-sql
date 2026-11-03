//! ★ D-448(11-03~04 · 사용자): 결과 내보내기의 **형식 · 기본 이름 · 원천** — 형식은 우클릭 하위 메뉴의 말(Excel · CSV · …)로 고르고
//! (코드 = 확장자 · 표 = `nsql_settings::EXPORT_FORMAT_OPTS` 하나) · 기본 이름은 설정 `export.name_pattern`(`report_{stamp}`)의 토큰 치환
//! 더하기 고른 확장자 · 원천(`export.source`) = 질의 다시 실행(워커 스트리밍 · `grid_results.rs`) / 조회된 행만(여기
//! `spawn_fetched_export` · 그리드 스냅숏을 스레드가 청크로 씀 · 질의·세션 없음). 순수 함수는 시험으로.

use nsql_core::{Dialect, ResultData, View};
use nsql_i18n::Msg;
use nsql_io::{Format, StreamWriter};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// 형식 목록(코드 · 라벨) — 기본 형식을 맨 앞으로, 나머지는 표 순서.
pub(crate) fn formats_ordered(default: &str) -> Vec<(&'static str, Msg)> {
    let all = nsql_settings::EXPORT_FORMAT_OPTS;
    let mut v: Vec<(&'static str, Msg)> =
        all.iter().copied().filter(|(c, _)| *c == default).collect();
    v.extend(all.iter().copied().filter(|(c, _)| *c != default));
    v
}

/// 저장 창 기본 파일 이름: 패턴의 토큰 `{stamp}`(yyyyMMddHHmmss) · `{date}`(yyyyMMdd) · `{time}`(HHmmss) · `{table}`(원본 질의의 표 ·
/// 스키마·인용 부호 제거 · 모르면 `result`) 치환 → 파일 이름에 못 쓰는 글자는 `_` → 비면 `report` → `.확장자`(확장자가 비면 줄기만).
pub(crate) fn default_name(
    pattern: &str,
    now: &nsql_log::LocalTime,
    table: Option<&str>,
    ext: &str,
) -> String {
    let date = format!("{:04}{:02}{:02}", now.year, now.month, now.day);
    let time = format!("{:02}{:02}{:02}", now.hour, now.min, now.sec);
    let table = table
        .map(|t| {
            t.rsplit('.')
                .next()
                .unwrap_or(t)
                .trim_matches(|c| matches!(c, '"' | '`' | '[' | ']' | ' '))
                .to_string()
        })
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "result".into());
    let s = pattern
        .replace("{stamp}", &format!("{date}{time}"))
        .replace("{date}", &date)
        .replace("{time}", &time)
        .replace("{table}", &table);
    let stem: String = s
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '_' | '-' | ' ' | '.' | '(' | ')') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let stem = stem.trim().trim_matches('.').trim();
    let stem = if stem.is_empty() { "report" } else { stem };
    if ext.is_empty() {
        stem.to_string()
    } else {
        format!("{stem}.{ext}")
    }
}

/// 백그라운드 내보내기 한 건(원천 "조회된 행만") — 결과 = (쓴 행 · 초) 또는 오류 글 · 호스트가 한가할 때 폴링(`export_bg_poll`).
pub(crate) struct ExportJob {
    pub rx: Receiver<Result<(u64, f64), String>>,
    pub path: PathBuf,
}

/// 청크 크기(행) — 청크 경계마다 취소 깃발을 본다.
const CHUNK_ROWS: usize = 10_000;

/// 스냅숏(`ResultData` + 행·열 인덱스 = 그리드 뷰)을 파일로 — 순수(스레드 없음 · 시험 가능). 0행도 머리(헤더 · `[]`)는 남는다 ·
/// 취소 = 청크 경계에서 멈추고 `Err("cancelled")`(쓴 데까지는 파일에 남는다 · 워커 경로와 같은 어휘).
#[allow(clippy::too_many_arguments)]
pub(crate) fn write_snapshot(
    path: &Path,
    format: Format,
    dialect: Dialect,
    null: &str,
    data: &ResultData,
    rows: &[usize],
    cols: &[usize],
    cancel: &AtomicBool,
) -> Result<(u64, f64), String> {
    let t0 = Instant::now();
    let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = std::io::BufWriter::new(file);
    let mut w = StreamWriter::new(format, dialect, null);
    let mut cancelled = false;
    let mut start = 0;
    loop {
        let end = (start + CHUNK_ROWS).min(rows.len());
        let view = View::new(data).rows(&rows[start..end]).cols(cols);
        w.write_batch(&mut out, &view).map_err(|e| e.to_string())?;
        start = end;
        if start >= rows.len() {
            break;
        }
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
    }
    w.finish(&mut out).map_err(|e| e.to_string())?;
    out.flush().map_err(|e| e.to_string())?;
    if cancelled {
        return Err("cancelled".into());
    }
    Ok((w.rows(), t0.elapsed().as_secs_f64()))
}

/// 스레드 `result-export`에서 [`write_snapshot`] → 채널 + `Wake`. 스레드를 못 만들면 `None`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_fetched_export(
    path: PathBuf,
    format: Format,
    dialect: Dialect,
    null: String,
    data: ResultData,
    rows: Vec<usize>,
    cols: Vec<usize>,
    cancel: Arc<AtomicBool>,
    proxy: winit::event_loop::EventLoopProxy<crate::Wake>,
) -> Option<ExportJob> {
    let (tx, rx) = channel();
    let p = path.clone();
    let proxy = Mutex::new(proxy);
    std::thread::Builder::new()
        .name("result-export".into())
        .spawn(move || {
            let result = write_snapshot(&p, format, dialect, &null, &data, &rows, &cols, &cancel);
            let _ = tx.send(result);
            if let Ok(px) = proxy.lock() {
                let _ = px.send_event(crate::Wake);
            }
        })
        .ok()?;
    Some(ExportJob { rx, path })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_core::{Column, ResultSet, RowSource, Value};

    fn at() -> nsql_log::LocalTime {
        nsql_log::LocalTime {
            year: 2026,
            month: 11,
            day: 3,
            hour: 15,
            min: 30,
            sec: 7,
            ms: 0,
        }
    }

    #[test]
    fn default_first_then_table_order_and_all_present() {
        let v = formats_ordered("sql");
        assert_eq!(v[0].0, "sql");
        let codes: Vec<&str> = v.iter().map(|(c, _)| *c).collect();
        assert_eq!(
            codes,
            ["sql", "xlsx", "csv", "tsv", "json", "jsonl", "parquet"]
        );
        // 모르는 기본값 = 표 순서 그대로(xlsx 맨 위).
        assert_eq!(formats_ordered("zip")[0].0, "xlsx");
        assert_eq!(
            formats_ordered("zip").len(),
            nsql_settings::EXPORT_FORMAT_OPTS.len()
        );
    }

    #[test]
    fn name_pattern_tokens_and_sanitizing() {
        let t = at();
        assert_eq!(
            default_name("report_{stamp}", &t, None, "xlsx"),
            "report_20261103153007.xlsx"
        );
        assert_eq!(
            default_name("{table}_{date}-{time}", &t, Some("HR.\"EMP\""), "csv"),
            "EMP_20261103-153007.csv"
        );
        assert_eq!(default_name("{table}", &t, None, "sql"), "result.sql");
        // 못 쓰는 글자는 `_` · 빈 결과는 report · 확장자 없음 = 줄기만.
        assert_eq!(default_name("a/b:c?", &t, None, "tsv"), "a_b_c_.tsv");
        assert_eq!(default_name("  ...  ", &t, None, "json"), "report.json");
        assert_eq!(default_name("x", &t, None, ""), "x");
    }

    fn data() -> ResultData {
        let col = |n: &str| Column {
            name: n.into(),
            type_name: "TEXT".into(),
        };
        ResultData::new(ResultSet {
            columns: vec![col("id"), col("name"), col("sal")],
            rows: (1..=25_000)
                .map(|i| {
                    vec![
                        Value::Int(i),
                        Value::Str(format!("n{i}")),
                        if i % 7 == 0 {
                            Value::Null
                        } else {
                            Value::Int(i * 10)
                        },
                    ]
                })
                .collect(),
        })
    }

    /// 뷰(행 역순 일부 · 열 순서 바꿈)가 그대로 파일에 · 0행 = 헤더만 · 취소 = 첫 청크 뒤 멈춤.
    #[test]
    fn write_snapshot_honors_view_and_cancel() {
        let d = data();
        let tp = nsql_core::testpath::TempPath::with_ext("export-snap", "csv");
        let no = AtomicBool::new(false);
        // 행 3개 역순 · 열 = sal, id(이름 열 숨김).
        let (n, _) = write_snapshot(
            tp.path(),
            Format::Csv,
            Dialect::Sqlite,
            "∅",
            &d,
            &[2, 1, 0],
            &[2, 0],
            &no,
        )
        .expect("csv");
        assert_eq!(n, 3);
        let text = std::fs::read_to_string(tp.path()).expect("read");
        assert_eq!(text, "sal,id\n30,3\n20,2\n10,1\n");
        // 0행 = 헤더만.
        let (n0, _) = write_snapshot(
            tp.path(),
            Format::Tsv,
            Dialect::Sqlite,
            "",
            &d,
            &[],
            &[0, 1],
            &no,
        )
        .expect("tsv");
        assert_eq!(n0, 0);
        assert_eq!(
            std::fs::read_to_string(tp.path()).expect("read"),
            "id\tname\n"
        );
        // 취소 깃발 = 첫 청크(10,000행) 뒤 멈춤.
        let yes = AtomicBool::new(true);
        let all: Vec<usize> = (0..RowSource::len(&d)).collect();
        let r = write_snapshot(
            tp.path(),
            Format::JsonLines,
            Dialect::Sqlite,
            "",
            &d,
            &all,
            &[0],
            &yes,
        );
        assert_eq!(r, Err("cancelled".to_string()));
        assert_eq!(
            std::fs::read_to_string(tp.path())
                .expect("read")
                .lines()
                .count(),
            CHUNK_ROWS
        );
    }
}
