//! ★ 가져오기 읽기 포트(docs/127 §3 · 11-04 · T-8 가져오기 ①): 형식 6(csv · tsv · json · jsonl · xlsx · parquet)을 **`RecordSource` 하나**로 —
//! 레코드 = 셀 글(없음 = NULL) + 원료 행·줄 번호. 글 파일은 **인코딩 자동**(BOM → UTF-8 검증 → cp949 · `encoding_rs` 스트리밍 디코더 ·
//! D-454) · 구분자 자동(앞 64 KB에서 `,` `\t` `;` `|` 중 줄마다 개수가 일정한 것) · 헤더 없으면 `col1..` · `null_text`로 NULL.
//! 외부 crate는 형식 모듈 안에만(`xlsx_in` = calamine · `parquet_in` = parquet · 여기 = encoding_rs) · 공개 시그니처에 외부 타입 0.

use std::io::{self, BufRead, BufReader, Read};
use std::path::Path;

/// 원료 형식(`Auto` = 확장자로 · 모르면 csv).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SourceFormat {
    #[default]
    Auto,
    Csv,
    Tsv,
    Json,
    Jsonl,
    Xlsx,
    Parquet,
}

impl SourceFormat {
    /// CLI/설정 글 → 형식(`excel` = xlsx · `ndjson` = jsonl).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "auto" => SourceFormat::Auto,
            "csv" => SourceFormat::Csv,
            "tsv" | "tab" => SourceFormat::Tsv,
            "json" => SourceFormat::Json,
            "jsonl" | "ndjson" => SourceFormat::Jsonl,
            "xlsx" | "excel" | "xls" | "xlsb" | "ods" => SourceFormat::Xlsx,
            "parquet" => SourceFormat::Parquet,
            _ => return None,
        })
    }

    /// 확장자로 확정(`Auto`일 때만 · 모르면 csv).
    #[must_use]
    pub fn resolve(self, path: &Path) -> Self {
        if self != SourceFormat::Auto {
            return self;
        }
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "tsv" | "tab" => SourceFormat::Tsv,
            "json" => SourceFormat::Json,
            "jsonl" | "ndjson" => SourceFormat::Jsonl,
            "xlsx" | "xlsm" | "xls" | "xlsb" | "ods" => SourceFormat::Xlsx,
            "parquet" | "pq" => SourceFormat::Parquet,
            _ => SourceFormat::Csv,
        }
    }

    /// 표시·보고용 이름.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            SourceFormat::Auto => "auto",
            SourceFormat::Csv => "csv",
            SourceFormat::Tsv => "tsv",
            SourceFormat::Json => "json",
            SourceFormat::Jsonl => "jsonl",
            SourceFormat::Xlsx => "xlsx",
            SourceFormat::Parquet => "parquet",
        }
    }

    /// 글 파일(인코딩·구분자 옵션이 뜻 있는 형식).
    #[must_use]
    pub fn is_text(self) -> bool {
        matches!(
            self,
            SourceFormat::Csv | SourceFormat::Tsv | SourceFormat::Json | SourceFormat::Jsonl
        )
    }
}

/// 원료 열기 옵션(CLI 플래그 · GUI Import 창 A 구역 · 설정 `import.*` 기본).
#[derive(Clone, Debug)]
pub struct SourceOpts {
    pub format: SourceFormat,
    /// 구분자(없으면 자동 · tsv = 탭).
    pub delim: Option<u8>,
    /// 첫 레코드 = 열 이름(구분자 파일 · xlsx).
    pub header: bool,
    /// `auto` · `utf-8` · `euc-kr` · `utf-16le` · `utf-16be` · 그 밖 WHATWG 라벨.
    pub encoding: String,
    /// xlsx 시트(이름 또는 0부터 번호 · 없으면 첫 시트).
    pub sheet: Option<String>,
    /// 셀 앞뒤 공백 다듬기.
    pub trim: bool,
    /// 이 글과 같은 셀 = NULL(비면 끔 · 빈 셀은 `bulk.empty_null`이 다룬다).
    pub null_text: String,
}

impl Default for SourceOpts {
    fn default() -> Self {
        SourceOpts {
            format: SourceFormat::Auto,
            delim: None,
            header: true,
            encoding: "auto".into(),
            sheet: None,
            trim: false,
            null_text: String::new(),
        }
    }
}

/// 원료 한 레코드.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// 셀 글(`None` = NULL) · 열 수는 `columns()`와 다를 수 있다(짧으면 NULL · 길면 자른다 = 호스트 몫).
    pub cells: Vec<Option<String>>,
    /// 원료 행 번호(1부터 · 헤더 제외).
    pub row: u64,
    /// 원료 줄 번호(글 파일 · 1부터 · 이진은 행 번호와 같음).
    pub line: u64,
}

/// 읽기 포트 — 형식마다 구현 · 호스트(`nsql-run` 적재 · 미리보기 · 형 추론)는 이것만 안다.
pub trait RecordSource {
    /// 열 이름(헤더 · 없으면 `col1..`) — 첫 호출에서 머리를 읽는다 · 뒤 호출은 같은 값.
    fn columns(&mut self) -> io::Result<Vec<String>>;
    /// 다음 레코드(끝 = `None`).
    fn next(&mut self) -> io::Result<Option<Record>>;
    /// 전체 행 수를 미리 알면(parquet 메타 · xlsx 범위) 진행률에.
    fn total_hint(&self) -> Option<u64> {
        None
    }
    /// 알아낸 것(인코딩 · 구분자 · 시트) — 미리보기 창이 콤보 초깃값으로 보여 준다.
    fn detected(&self) -> Detected {
        Detected::default()
    }
}

/// 자동 판정 결과.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Detected {
    pub encoding: Option<&'static str>,
    pub delim: Option<u8>,
    pub sheet: Option<String>,
    pub sheets: Vec<String>,
}

/// 열 이름 정리: 공백 다듬기 · 빈 이름 → `col{i}` · 중복 → `_2` `_3`.
pub fn tidy_columns(raw: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(raw.len());
    for (i, r) in raw.into_iter().enumerate() {
        let base = r.trim().trim_start_matches('\u{feff}').to_string();
        let mut name = if base.is_empty() {
            format!("col{}", i + 1)
        } else {
            base
        };
        if out.iter().any(|o| o.eq_ignore_ascii_case(&name)) {
            let stem = name.clone();
            let mut k = 2;
            loop {
                name = format!("{stem}_{k}");
                if !out.iter().any(|o| o.eq_ignore_ascii_case(&name)) {
                    break;
                }
                k += 1;
            }
        }
        out.push(name);
    }
    out
}

/// 파일 열기(형식 = `opts.format` · Auto면 확장자 · `-` = 표준 입력(글 형식만)).
pub fn open_source(path: &Path, opts: &SourceOpts) -> io::Result<Box<dyn RecordSource>> {
    let fmt = opts.format.resolve(path);
    if path.as_os_str() == "-" {
        if !fmt.is_text() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "{}: stdin needs a text format (csv/tsv/json/jsonl)",
                    fmt.name()
                ),
            ));
        }
        return open_text(Box::new(io::stdin()), fmt, opts);
    }
    match fmt {
        SourceFormat::Xlsx => open_xlsx(path, opts),
        SourceFormat::Parquet => open_parquet(path, opts),
        _ => {
            let f = std::fs::File::open(path)
                .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", path.display())))?;
            open_text(Box::new(f), fmt, opts)
        }
    }
}

#[cfg(feature = "import-xlsx")]
fn open_xlsx(path: &Path, opts: &SourceOpts) -> io::Result<Box<dyn RecordSource>> {
    Ok(Box::new(crate::xlsx_in::XlsxSource::open(path, opts)?))
}

#[cfg(not(feature = "import-xlsx"))]
fn open_xlsx(_path: &Path, _opts: &SourceOpts) -> io::Result<Box<dyn RecordSource>> {
    Err(io::Error::other(
        "xlsx import is not available in this build",
    ))
}

#[cfg(feature = "import-parquet")]
fn open_parquet(path: &Path, opts: &SourceOpts) -> io::Result<Box<dyn RecordSource>> {
    Ok(Box::new(crate::parquet_in::ParquetSource::open(
        path, opts,
    )?))
}

#[cfg(not(feature = "import-parquet"))]
fn open_parquet(_path: &Path, _opts: &SourceOpts) -> io::Result<Box<dyn RecordSource>> {
    Err(io::Error::other(
        "parquet import is not available in this build",
    ))
}

/// 글 원료(csv · tsv · json · jsonl) — 앞 64 KB를 먼저 읽어 인코딩·구분자를 정하고 나머지는 스트리밍.
pub fn open_text(
    mut r: Box<dyn Read>,
    fmt: SourceFormat,
    opts: &SourceOpts,
) -> io::Result<Box<dyn RecordSource>> {
    let mut prefix = vec![0u8; SNIFF_BYTES];
    let mut got = 0;
    while got < prefix.len() {
        let n = r.read(&mut prefix[got..])?;
        if n == 0 {
            break;
        }
        got += n;
    }
    prefix.truncate(got);
    let enc = if opts.encoding.eq_ignore_ascii_case("auto") {
        detect_encoding(&prefix)
    } else {
        canonical_encoding(&opts.encoding)
    };
    let (head_text, _, _) = decode_prefix(&prefix, enc);
    let chained: Box<dyn Read> = Box::new(io::Cursor::new(prefix).chain(r));
    let decoded = BufReader::with_capacity(1 << 16, DecodeReader::new(chained, enc));
    match fmt {
        SourceFormat::Json => {
            let mut text = String::new();
            let mut br = decoded;
            br.read_to_string(&mut text)?;
            Ok(Box::new(JsonSource::from_text(&text, opts, enc)?))
        }
        SourceFormat::Jsonl => Ok(Box::new(JsonlSource {
            rd: crate::jsonl::JsonlReader::new(decoded),
            keys: Vec::new(),
            first: None,
            row: 0,
            opts: opts.clone(),
            enc,
        })),
        _ => {
            let delim = match (opts.delim, fmt) {
                (Some(d), _) => d,
                (None, SourceFormat::Tsv) => b'\t',
                (None, _) => sniff_delim(&head_text),
            };
            Ok(Box::new(DelimSource {
                rd: crate::delim::DelimReader::new(decoded, delim),
                cols: None,
                pending: None,
                row: 0,
                opts: opts.clone(),
                enc,
                delim,
            }))
        }
    }
}

const SNIFF_BYTES: usize = 64 * 1024;

/// 인코딩 라벨 정리(`cp949` → `euc-kr` · 모르면 utf-8).
pub fn canonical_encoding(label: &str) -> &'static str {
    match label.trim().to_ascii_lowercase().as_str() {
        "utf-8" | "utf8" | "" => "utf-8",
        "utf-16le" | "utf-16" | "utf16le" => "utf-16le",
        "utf-16be" | "utf16be" => "utf-16be",
        "euc-kr" | "cp949" | "ms949" | "ks_c_5601-1987" | "euckr" => "euc-kr",
        "shift_jis" | "sjis" | "cp932" | "windows-31j" => "shift_jis",
        "gbk" | "gb2312" | "cp936" | "gb18030" => "gb18030",
        "big5" | "cp950" => "big5",
        "windows-1252" | "cp1252" | "latin1" | "iso-8859-1" => "windows-1252",
        "euc-jp" => "euc-jp",
        _ => "utf-8",
    }
}

/// ★ D-454 자동 판정: BOM(UTF-8 · UTF-16 LE/BE) → 앞부분이 UTF-8로 온전하면 UTF-8 → 아니면 **euc-kr**(한국 Windows Excel CSV).
pub fn detect_encoding(prefix: &[u8]) -> &'static str {
    if prefix.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return "utf-8";
    }
    if prefix.starts_with(&[0xFF, 0xFE]) {
        return "utf-16le";
    }
    if prefix.starts_with(&[0xFE, 0xFF]) {
        return "utf-16be";
    }
    // 잘린 꼬리(멀티바이트 중간)는 최대 3바이트까지 봐준다.
    let mut end = prefix.len();
    for _ in 0..4 {
        if std::str::from_utf8(&prefix[..end]).is_ok() {
            return "utf-8";
        }
        if end == 0 {
            break;
        }
        end -= 1;
    }
    "euc-kr"
}

/// 앞부분을 글로(스니핑용 · 잘린 꼬리는 버린다).
fn decode_prefix(prefix: &[u8], enc: &str) -> (String, bool, &'static str) {
    let e = encoding_rs::Encoding::for_label(enc.as_bytes()).unwrap_or(encoding_rs::UTF_8);
    let (cow, used, bad) = e.decode(prefix);
    (cow.into_owned(), bad, used.name())
}

/// 구분자 자동 판정: 첫 20줄(따옴표 밖)에서 후보 4의 개수가 **모든 줄에서 같고 ≥ 1**인 것 중 가장 많은 것 · 없으면 합계 최대 · 그래도 0이면 `,`.
pub fn sniff_delim(head: &str) -> u8 {
    const CANDS: [u8; 4] = *b",\t;|";
    let lines: Vec<&str> = head
        .lines()
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.trim().is_empty())
        .take(20)
        .collect();
    if lines.is_empty() {
        return b',';
    }
    let count = |l: &str, d: u8| -> usize {
        let mut n = 0;
        let mut in_q = false;
        for c in l.chars() {
            if c == '"' {
                in_q = !in_q;
            } else if !in_q && c as u32 == u32::from(d) {
                n += 1;
            }
        }
        n
    };
    let mut best_consistent: Option<(u8, usize)> = None;
    let mut best_total: (u8, usize) = (b',', 0);
    for d in CANDS {
        let counts: Vec<usize> = lines.iter().map(|l| count(l, d)).collect();
        let total: usize = counts.iter().sum();
        if total > best_total.1 {
            best_total = (d, total);
        }
        let first = counts[0];
        if first >= 1 && counts.iter().all(|&c| c == first) {
            match best_consistent {
                Some((_, n)) if n >= first => {}
                _ => best_consistent = Some((d, first)),
            }
        }
    }
    best_consistent.map_or(best_total.0, |(d, _)| d)
}

/// `encoding_rs` 스트리밍 디코더 → UTF-8 바이트 `Read`(BOM 제거 · 깨진 바이트 = U+FFFD).
pub struct DecodeReader<R: Read> {
    inner: R,
    dec: Option<encoding_rs::Decoder>,
    inbuf: Vec<u8>,
    out: Vec<u8>,
    out_pos: usize,
    eof: bool,
    first: bool,
}

impl<R: Read> std::fmt::Debug for DecodeReader<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DecodeReader(decoder={}, eof={})",
            self.dec.is_some(),
            self.eof
        )
    }
}

impl<R: Read> DecodeReader<R> {
    pub fn new(inner: R, enc: &str) -> Self {
        let e = encoding_rs::Encoding::for_label(enc.as_bytes()).unwrap_or(encoding_rs::UTF_8);
        // UTF-8은 통과(BOM만 벗김) · 그 밖은 디코더.
        let dec = (e != encoding_rs::UTF_8).then(|| e.new_decoder_with_bom_removal());
        DecodeReader {
            inner,
            dec,
            inbuf: vec![0u8; 1 << 15],
            out: Vec::new(),
            out_pos: 0,
            eof: false,
            first: true,
        }
    }

    fn fill(&mut self) -> io::Result<()> {
        self.out.clear();
        self.out_pos = 0;
        if self.eof {
            return Ok(());
        }
        let n = self.inner.read(&mut self.inbuf)?;
        let last = n == 0;
        if last {
            self.eof = true;
        }
        match self.dec.as_mut() {
            None => {
                let mut src = &self.inbuf[..n];
                if self.first {
                    self.first = false;
                    if src.starts_with(&[0xEF, 0xBB, 0xBF]) {
                        src = &src[3..];
                    }
                }
                self.out.extend_from_slice(src);
            }
            Some(dec) => {
                let src = &self.inbuf[..n];
                let cap = dec
                    .max_utf8_buffer_length(src.len())
                    .unwrap_or(src.len() * 4 + 16);
                let mut dst = vec![0u8; cap.max(16)];
                let (_, _read, written, _bad) = dec.decode_to_utf8(src, &mut dst, last);
                dst.truncate(written);
                self.out = dst;
                self.first = false;
            }
        }
        Ok(())
    }
}

impl<R: Read> Read for DecodeReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.out_pos < self.out.len() {
                let n = (self.out.len() - self.out_pos).min(buf.len());
                buf[..n].copy_from_slice(&self.out[self.out_pos..self.out_pos + n]);
                self.out_pos += n;
                return Ok(n);
            }
            if self.eof {
                return Ok(0);
            }
            self.fill()?;
        }
    }
}

fn cell_of(s: String, opts: &SourceOpts) -> Option<String> {
    let s = if opts.trim { s.trim().to_string() } else { s };
    if !opts.null_text.is_empty() && s == opts.null_text {
        None
    } else {
        Some(s)
    }
}

/// 구분자 파일 원료.
struct DelimSource<R: BufRead> {
    rd: crate::delim::DelimReader<R>,
    cols: Option<Vec<String>>,
    /// 헤더가 없을 때 열 수를 세려고 먼저 읽어 둔 첫 레코드.
    pending: Option<(Vec<String>, u64)>,
    row: u64,
    opts: SourceOpts,
    enc: &'static str,
    delim: u8,
}

impl<R: BufRead> RecordSource for DelimSource<R> {
    fn columns(&mut self) -> io::Result<Vec<String>> {
        if let Some(c) = &self.cols {
            return Ok(c.clone());
        }
        let first = self.rd.next_record()?;
        let cols = match (first, self.opts.header) {
            (None, _) => Vec::new(),
            (Some(h), true) => tidy_columns(h),
            (Some(r), false) => {
                let n = r.len();
                self.pending = Some((r, self.rd.line_no));
                (1..=n).map(|i| format!("col{i}")).collect()
            }
        };
        self.cols = Some(cols.clone());
        Ok(cols)
    }

    fn next(&mut self) -> io::Result<Option<Record>> {
        if self.cols.is_none() {
            self.columns()?;
        }
        let (rec, line) = match self.pending.take() {
            Some(p) => p,
            None => match self.rd.next_record()? {
                Some(r) => (r, self.rd.line_no),
                None => return Ok(None),
            },
        };
        self.row += 1;
        Ok(Some(Record {
            cells: rec.into_iter().map(|s| cell_of(s, &self.opts)).collect(),
            row: self.row,
            line,
        }))
    }

    fn detected(&self) -> Detected {
        Detected {
            encoding: Some(self.enc),
            delim: Some(self.delim),
            ..Detected::default()
        }
    }
}

/// JSONL 객체 = (키, 값) 목록.
type Obj = Vec<(String, Option<String>)>;

/// JSON Lines 원료(열 = 첫 객체의 키 · 뒤 객체의 모르는 키는 버림 · 없는 키 = NULL).
struct JsonlSource<R: BufRead> {
    rd: crate::jsonl::JsonlReader<R>,
    keys: Vec<String>,
    first: Option<(Obj, u64)>,
    row: u64,
    opts: SourceOpts,
    enc: &'static str,
}

impl<R: BufRead> RecordSource for JsonlSource<R> {
    fn columns(&mut self) -> io::Result<Vec<String>> {
        if !self.keys.is_empty() || self.first.is_some() {
            return Ok(self.keys.clone());
        }
        if let Some(o) = self.rd.next_object()? {
            self.keys = tidy_columns(o.iter().map(|(k, _)| k.clone()).collect());
            self.first = Some((o, self.rd.line_no));
        }
        Ok(self.keys.clone())
    }

    fn next(&mut self) -> io::Result<Option<Record>> {
        if self.keys.is_empty() && self.first.is_none() {
            self.columns()?;
        }
        let (obj, line) = match self.first.take() {
            Some(p) => p,
            None => match self.rd.next_object()? {
                Some(o) => (o, self.rd.line_no),
                None => return Ok(None),
            },
        };
        self.row += 1;
        let cells = self
            .keys
            .iter()
            .map(|k| {
                obj.iter()
                    .find(|(kk, _)| kk == k)
                    .and_then(|(_, v)| v.clone())
                    .and_then(|s| cell_of(s, &self.opts))
            })
            .collect();
        Ok(Some(Record {
            cells,
            row: self.row,
            line,
        }))
    }

    fn detected(&self) -> Detected {
        Detected {
            encoding: Some(self.enc),
            ..Detected::default()
        }
    }
}

/// JSON 배열 원료(전체 파싱 · 열 = 모든 객체 키의 합집합(등장 순) · 스칼라 배열 = `value` 한 열 · 최상위 객체면 첫 배열 필드).
struct JsonSource {
    cols: Vec<String>,
    rows: std::vec::IntoIter<Vec<Option<String>>>,
    total: u64,
    row: u64,
    enc: &'static str,
}

impl JsonSource {
    fn from_text(text: &str, opts: &SourceOpts, enc: &'static str) -> io::Result<Self> {
        use nsql_json::Json;
        let v =
            nsql_json::parse(text).map_err(|m| io::Error::new(io::ErrorKind::InvalidData, m))?;
        let items: Vec<Json> = match v {
            Json::Arr(a) => a,
            Json::Obj(fields) => match fields.into_iter().find_map(|(_, v)| match v {
                Json::Arr(a) => Some(a),
                _ => None,
            }) {
                Some(a) => a,
                None => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "json: expected an array of objects (or an object holding one)",
                    ))
                }
            },
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "json: expected an array",
                ))
            }
        };
        let text_of = |j: &Json| -> Option<String> {
            match j {
                Json::Null => None,
                Json::Bool(b) => Some(if *b { "true" } else { "false" }.into()),
                Json::Num(_) | Json::Str(_) => j.scalar_text(),
                Json::Arr(_) | Json::Obj(_) => Some(nsql_json::dump(j)),
            }
        };
        let all_objects = items.iter().all(|j| matches!(j, Json::Obj(_)));
        let (cols, rows): (Vec<String>, Vec<Vec<Option<String>>>) = if all_objects {
            let mut keys: Vec<String> = Vec::new();
            for j in &items {
                if let Json::Obj(f) = j {
                    for (k, _) in f {
                        if !keys.iter().any(|x| x == k) {
                            keys.push(k.clone());
                        }
                    }
                }
            }
            let rows = items
                .iter()
                .map(|j| {
                    let Json::Obj(f) = j else { unreachable!() };
                    keys.iter()
                        .map(|k| {
                            f.iter()
                                .find(|(kk, _)| kk == k)
                                .and_then(|(_, v)| text_of(v))
                                .and_then(|s| cell_of(s, opts))
                        })
                        .collect()
                })
                .collect();
            (tidy_columns(keys), rows)
        } else {
            (
                vec!["value".to_string()],
                items
                    .iter()
                    .map(|j| vec![text_of(j).and_then(|s| cell_of(s, opts))])
                    .collect(),
            )
        };
        Ok(JsonSource {
            cols,
            total: rows.len() as u64,
            rows: rows.into_iter(),
            row: 0,
            enc,
        })
    }
}

impl RecordSource for JsonSource {
    fn columns(&mut self) -> io::Result<Vec<String>> {
        Ok(self.cols.clone())
    }

    fn next(&mut self) -> io::Result<Option<Record>> {
        Ok(self.rows.next().map(|cells| {
            self.row += 1;
            Record {
                cells,
                row: self.row,
                line: self.row,
            }
        }))
    }

    fn total_hint(&self) -> Option<u64> {
        Some(self.total)
    }

    fn detected(&self) -> Detected {
        Detected {
            encoding: Some(self.enc),
            ..Detected::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(bytes: &[u8], fmt: SourceFormat, opts: &SourceOpts) -> Box<dyn RecordSource> {
        open_text(Box::new(io::Cursor::new(bytes.to_vec())), fmt, opts).expect("open")
    }

    fn all(s: &mut dyn RecordSource) -> Vec<Vec<Option<String>>> {
        let mut v = Vec::new();
        while let Some(r) = s.next().expect("next") {
            v.push(r.cells);
        }
        v
    }

    fn some(items: &[&str]) -> Vec<Option<String>> {
        items.iter().map(|s| Some((*s).to_string())).collect()
    }

    #[test]
    fn csv_bom_header_and_sniffed_semicolon() {
        let o = SourceOpts::default();
        let mut s = src(
            "\u{feff}id;name;memo\n1;Kim;\"a;b\"\n2;Lee;\n".as_bytes(),
            SourceFormat::Csv,
            &o,
        );
        assert_eq!(s.columns().expect("cols"), vec!["id", "name", "memo"]);
        assert_eq!(s.detected().delim, Some(b';'));
        assert_eq!(s.detected().encoding, Some("utf-8"));
        let rows = all(s.as_mut());
        assert_eq!(
            rows,
            vec![some(&["1", "Kim", "a;b"]), some(&["2", "Lee", ""])]
        );
    }

    #[test]
    fn csv_no_header_null_text_trim_and_line_numbers() {
        let o = SourceOpts {
            header: false,
            trim: true,
            null_text: "NULL".into(),
            ..SourceOpts::default()
        };
        let mut s = src(b"1, Kim ,NULL\n\n2,Lee,x\n", SourceFormat::Csv, &o);
        assert_eq!(s.columns().expect("cols"), vec!["col1", "col2", "col3"]);
        let r1 = s.next().expect("n").expect("r");
        assert_eq!(r1.cells, vec![Some("1".into()), Some("Kim".into()), None]);
        assert_eq!((r1.row, r1.line), (1, 1));
        let r2 = s.next().expect("n").expect("r");
        assert_eq!(
            (r2.row, r2.line),
            (2, 3),
            "빈 줄은 건너뛰고 줄 번호는 원료 기준"
        );
        assert!(s.next().expect("n").is_none());
    }

    #[test]
    fn cp949_auto_detected_and_decoded() {
        let (bytes, _, _) = encoding_rs::EUC_KR.encode("이름,부서\n홍길동,영업\n");
        assert_eq!(detect_encoding(&bytes), "euc-kr");
        let mut s = src(&bytes, SourceFormat::Csv, &SourceOpts::default());
        assert_eq!(s.columns().expect("cols"), vec!["이름", "부서"]);
        assert_eq!(all(s.as_mut()), vec![some(&["홍길동", "영업"])]);
        assert_eq!(s.detected().encoding, Some("euc-kr"));
        // 명시 인코딩(cp949 별칭)도 같다.
        let o = SourceOpts {
            encoding: "cp949".into(),
            ..SourceOpts::default()
        };
        let mut s2 = src(&bytes, SourceFormat::Csv, &o);
        assert_eq!(s2.columns().expect("cols"), vec!["이름", "부서"]);
    }

    #[test]
    fn utf16le_bom_tsv() {
        let text = "a\tb\n한\t2\n";
        let mut bytes = vec![0xFF, 0xFE];
        for u in text.encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        let mut s = src(&bytes, SourceFormat::Tsv, &SourceOpts::default());
        assert_eq!(s.columns().expect("cols"), vec!["a", "b"]);
        assert_eq!(all(s.as_mut()), vec![some(&["한", "2"])]);
    }

    #[test]
    fn sniff_prefers_consistent_delimiter() {
        assert_eq!(
            sniff_delim("a,b;c\n1,2;3\n"),
            b',',
            "동률이면 후보 순서(쉼표 먼저)"
        );
        assert_eq!(sniff_delim("a|b|c\n1|2|3\n"), b'|');
        assert_eq!(sniff_delim("x\ty\n1\t2\n"), b'\t');
        assert_eq!(
            sniff_delim("\"a,b\";c\n\"1,2\";3\n"),
            b';',
            "따옴표 안 쉼표는 세지 않음"
        );
        assert_eq!(sniff_delim("plain\n"), b',');
    }

    #[test]
    fn jsonl_keys_from_first_object_and_missing_is_null() {
        let o = SourceOpts::default();
        let mut s = src(
            b"{\"id\":1,\"name\":\"Kim\"}\n{\"name\":\"Lee\",\"extra\":true}\n",
            SourceFormat::Jsonl,
            &o,
        );
        assert_eq!(s.columns().expect("cols"), vec!["id", "name"]);
        let rows = all(s.as_mut());
        assert_eq!(rows[0], some(&["1", "Kim"]));
        assert_eq!(rows[1], vec![None, Some("Lee".into())]);
    }

    #[test]
    fn json_array_union_keys_scalars_and_wrapped_object() {
        let o = SourceOpts::default();
        let mut s = src(
            b"[{\"id\":1,\"tags\":[\"a\",\"b\"]},{\"id\":2,\"name\":null,\"n\":2.5}]",
            SourceFormat::Json,
            &o,
        );
        assert_eq!(s.columns().expect("cols"), vec!["id", "tags", "name", "n"]);
        assert_eq!(s.total_hint(), Some(2));
        let rows = all(s.as_mut());
        assert_eq!(
            rows[0],
            vec![Some("1".into()), Some("[\"a\",\"b\"]".into()), None, None]
        );
        assert_eq!(
            rows[1],
            vec![Some("2".into()), None, None, Some("2.5".into())]
        );
        let mut sc = src(b"[1, \"x\", true]", SourceFormat::Json, &o);
        assert_eq!(sc.columns().expect("cols"), vec!["value"]);
        assert_eq!(
            all(sc.as_mut()),
            vec![some(&["1"]), some(&["x"]), some(&["true"])]
        );
        let mut w = src(
            b"{\"count\":1,\"data\":[{\"a\":1}]}",
            SourceFormat::Json,
            &o,
        );
        assert_eq!(w.columns().expect("cols"), vec!["a"]);
        assert!(
            open_text(
                Box::new(io::Cursor::new(b"42".to_vec())),
                SourceFormat::Json,
                &o
            )
            .is_err(),
            "스칼라 하나는 배열이 아니다"
        );
    }

    /// 내보내기(StreamWriter xlsx · parquet) → 가져오기 왕복: 열 이름 · 값 글(정수 · 소수 · 글 · NULL · 불) · 행 수 힌트.
    #[cfg(all(feature = "import-xlsx", feature = "import-parquet"))]
    #[test]
    fn xlsx_and_parquet_round_trip() {
        use nsql_core::{Column, Dialect, ResultSet, Value};
        let col = |n: &str| Column {
            name: n.into(),
            type_name: String::new(),
        };
        let rs = ResultSet {
            columns: vec![col("id"), col("name"), col("sal"), col("ok")],
            rows: vec![
                vec![
                    Value::Int(1),
                    Value::Str("Kim 김".into()),
                    Value::Decimal("3500.50".into()),
                    Value::Bool(true),
                ],
                vec![
                    Value::Int(2),
                    Value::Null,
                    Value::Float(1.25),
                    Value::Bool(false),
                ],
            ],
        };
        for (fmt, ext) in [
            (crate::Format::Xlsx, "xlsx"),
            (crate::Format::Parquet, "parquet"),
        ] {
            let tp = nsql_core::testpath::TempPath::with_ext("import-rt", ext);
            {
                let mut out = std::fs::File::create(tp.path()).expect("create");
                let mut w = crate::StreamWriter::new(fmt, Dialect::Sqlite, "");
                w.write_batch(&mut out, &rs).expect("batch");
                w.finish(&mut out).expect("finish");
            }
            let mut s = open_source(tp.path(), &SourceOpts::default()).expect("open");
            assert_eq!(
                s.columns().expect("cols"),
                vec!["id", "name", "sal", "ok"],
                "{ext}"
            );
            assert_eq!(s.total_hint(), Some(2), "{ext}");
            let rows = all(s.as_mut());
            assert_eq!(rows[0][0].as_deref(), Some("1"), "{ext}");
            assert_eq!(rows[0][1].as_deref(), Some("Kim 김"), "{ext}");
            assert_eq!(rows[0][2].as_deref(), Some("3500.5"), "{ext}");
            assert_eq!(rows[0][3].as_deref(), Some("true"), "{ext}");
            assert_eq!(rows[1][1], None, "{ext}");
            assert_eq!(rows[1][2].as_deref(), Some("1.25"), "{ext}");
            assert_eq!(rows[1][3].as_deref(), Some("false"), "{ext}");
        }
        // xlsx 시트 선택(이름 · 번호 · 없음).
        let tp = nsql_core::testpath::TempPath::with_ext("import-sheet", "xlsx");
        {
            let mut out = std::fs::File::create(tp.path()).expect("create");
            let mut w = crate::StreamWriter::new(crate::Format::Xlsx, Dialect::Sqlite, "");
            w.write_batch(&mut out, &rs).expect("batch");
            w.finish(&mut out).expect("finish");
        }
        let o = SourceOpts {
            sheet: Some("0".into()),
            ..SourceOpts::default()
        };
        let s = open_source(tp.path(), &o).expect("open");
        assert_eq!(s.detected().sheets.len(), 1);
        let bad = SourceOpts {
            sheet: Some("nope".into()),
            ..SourceOpts::default()
        };
        assert!(open_source(tp.path(), &bad).is_err());
    }

    #[test]
    fn tidy_columns_fills_and_dedups() {
        assert_eq!(
            tidy_columns(vec!["a".into(), "".into(), "A".into(), " b ".into()]),
            vec!["a", "col2", "A_2", "b"]
        );
    }

    #[test]
    fn format_parse_and_resolve() {
        assert_eq!(SourceFormat::parse("excel"), Some(SourceFormat::Xlsx));
        assert_eq!(SourceFormat::parse("ndjson"), Some(SourceFormat::Jsonl));
        assert_eq!(
            SourceFormat::Auto.resolve(Path::new("a.PARQUET")),
            SourceFormat::Parquet
        );
        assert_eq!(
            SourceFormat::Auto.resolve(Path::new("a.txt")),
            SourceFormat::Csv
        );
        assert_eq!(
            SourceFormat::Tsv.resolve(Path::new("a.csv")),
            SourceFormat::Tsv
        );
    }
}
