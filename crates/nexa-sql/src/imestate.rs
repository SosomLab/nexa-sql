//! 지금 키보드의 **입력 언어·IME 변환 상태**를 작업 표시줄 표시처럼 한 글자로(사용자 09-22 "가린 입력란에 IME가 영어가 아니면
//! 어떤 상태인지 마우스 주변에 안내 · 국가별 IME 상태값 또는 작업 표시줄의 대표 글자").
//!
//! - Windows: 전경(또는 넘겨받은) 창의 스레드 키보드 배열(`GetKeyboardLayout` → LANGID) + IME 언어(한·일·중)는 기본 IME 창에
//!   `WM_IME_CONTROL/IMC_GETCONVERSIONMODE`로 변환 모드(`IME_CMODE_NATIVE`)를 물어 **가/A · あ/A · 中/英**. IME가 아닌 비라틴 배열
//!   (러시아어 · 태국어 · 아랍어 …)은 언어 코드 셋 글자(`RUS` …). 라틴 배열(영어 · 독일어 …)은 `latin = true`(안내 없음).
//! - macOS: 입력 소스가 한국어면 `가`(nexa-sys `input_source`) · 그 밖은 판정 없음. Linux: 엔진 이름(`ibus engine`) + 엔진 안 한/영 =
//!   ibus 패널 감시([`crate::imewatch`] · 유일한 원천) · 감시가 없으면 폴백(토글 키 · 입력 종류 · `initial-input-mode`).
//!
//! 안내를 띄우는 쪽은 [`crate::imehint`] — 이 모듈은 상태만 읽는다(창 핸들은 있으면 넘긴다 · 없으면 전경 창).

/// ★ 마지막 실제 입력의 종류(Linux 보정 · 사용자 09-27 "영어로 바꿔도 안내가 바로 안 사라진다"): ibus-hangul은 한/영을 **엔진 안에서**
/// 토글해 `ibus engine`은 계속 `hangul`이다 → 가린 칸에 글자가 **일반 키**로 오면(영문) 라틴, IME 조합·확정으로 오면(한글) 본연으로 본다.
/// 0 = 모름 · 1 = 라틴 · 2 = 본연. 엔진 이름이 바뀌면 다시 모름.
static LAST_INPUT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Linux 감시(ibus 패널 엿듣기 · [`crate::imewatch`])가 살아 있는가 — 살아 있으면 한/영의 **유일한 원천**: 키보드 판정은 무시하고
/// 엔진 이름이 바뀌어도 감시가 준 값을 버리지 않는다(사용자 09-27 "다른 창에서 바꾸고 돌아올 때도 정확하게" — 돌아오는 순간 ibus가
/// `RegisterProperties`로 지금 모드를 다시 보내는데, 같은 때 `ibus engine` 재조회가 "엔진 바뀜"으로 그 값을 0으로 되돌리던 경주).
static WATCH_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
pub(crate) fn set_watch_active(on: bool) {
    WATCH_ACTIVE.store(on, std::sync::atomic::Ordering::Relaxed);
}

#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
pub(crate) fn watch_active() -> bool {
    WATCH_ACTIVE.load(std::sync::atomic::Ordering::Relaxed)
}

/// 엔진 이름이 바뀌었을 때의 입력 종류 기억 처리 — 폴백(감시 없음)일 때만 "모름"으로 되돌린다.
#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
fn on_engine_change(changed: bool) {
    if changed && !watch_active() {
        LAST_INPUT.store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

/// 한/영 **토글 키**가 앱에 도달했다(Linux · `HangulMode`/`Shift+Space`/오른쪽 Alt — 입력기가 삼키지 않은 경우만 온다) → 지금 상태를 뒤집는다.
#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
pub(crate) fn note_toggle(currently_latin: bool) {
    LAST_INPUT.store(
        if currently_latin { 2 } else { 1 },
        std::sync::atomic::Ordering::Relaxed,
    );
}

/// 감시(ibus 패널)가 준 확정 상태 — 폴백 가드를 거치지 않는다.
#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
pub(crate) fn set_from_watch(native: bool) {
    LAST_INPUT.store(
        if native { 2 } else { 1 },
        std::sync::atomic::Ordering::Relaxed,
    );
}

/// 가린 칸에 입력이 들어왔다 — `latin` = 일반 키 글자(영문) · false = IME 조합/확정(한글 등).
/// ★ **폴백 전용**(사용자 09-27 "키보드 감시와 ibus 감시가 중복 아닌가"): Linux 감시(ibus 패널 엿듣기)가 돌면 그것이 유일한 원천 — 무시.
#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
pub(crate) fn note_input(latin: bool) {
    if watch_active() {
        return;
    }
    LAST_INPUT.store(
        if latin { 1 } else { 2 },
        std::sync::atomic::Ordering::Relaxed,
    );
}

/// 지금 입력 상태.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImeState {
    /// 대표 글자(`가` · `A` · `あ` · `中` · `英` · `RUS` …).
    pub(crate) glyph: &'static str,
    /// 언어 코드(ISO 639-2 세 글자 대문자 · 모르면 `???`).
    pub(crate) lang: &'static str,
    /// 라틴 글자가 들어가는 상태인가(영어 모드 · 라틴 배열) — 그러면 안내하지 않는다.
    pub(crate) latin: bool,
}

/// LANGID의 주 언어(하위 10비트) → (IME 언어면 (본연 글자, 라틴 글자) · 코드 · 라틴 배열 여부).
/// 표에 없는 언어는 비라틴 취급(모르는 것을 라틴으로 넘겨 안내를 빼먹지 않게).
// Linux는 입력 소스 조회가 없어(`current` = None) 순수 함수 둘이 테스트에서만 쓰인다(CI ubuntu `-D warnings`).
#[cfg_attr(not(any(windows, target_os = "macos", test)), allow(dead_code))]
fn describe(primary: u16) -> (Option<(&'static str, &'static str)>, &'static str, bool) {
    match primary {
        0x12 => (Some(("가", "A")), "KOR", false),
        0x11 => (Some(("あ", "A")), "JPN", false),
        0x04 => (Some(("中", "英")), "CHN", false),
        // 라틴 배열(IME 없음) — 영어 · 독일어 · 프랑스어 · 스페인어 · 이탈리아어 · 네덜란드어 · 포르투갈어 · 스웨덴어 · 노르웨이어 ·
        // 덴마크어 · 핀란드어 · 폴란드어 · 체코어 · 헝가리어 · 튀르키예어 · 루마니아어 · 크로아티아어 · 슬로바키아어 · 슬로베니아어 ·
        // 인도네시아어 · 말레이어 · 베트남어 · 에스토니아어 · 라트비아어 · 리투아니아어 · 아이슬란드어 · 아일랜드어 · 바스크어 · 카탈루냐어.
        0x09 => (None, "ENG", true),
        0x07 => (None, "DEU", true),
        0x0c => (None, "FRA", true),
        0x0a => (None, "SPA", true),
        0x10 => (None, "ITA", true),
        0x13 => (None, "NLD", true),
        0x16 => (None, "POR", true),
        0x1d => (None, "SWE", true),
        0x14 => (None, "NOR", true),
        0x06 => (None, "DAN", true),
        0x0b => (None, "FIN", true),
        0x15 => (None, "POL", true),
        0x05 => (None, "CES", true),
        0x0e => (None, "HUN", true),
        0x1f => (None, "TUR", true),
        0x18 => (None, "RON", true),
        0x1a => (None, "HRV", true),
        0x1b => (None, "SLK", true),
        0x24 => (None, "SLV", true),
        0x21 => (None, "IND", true),
        0x3e => (None, "MSA", true),
        0x2a => (None, "VIE", true),
        0x25 => (None, "EST", true),
        0x26 => (None, "LAV", true),
        0x27 => (None, "LIT", true),
        0x0f => (None, "ISL", true),
        0x3c => (None, "GLE", true),
        0x2d => (None, "EUS", true),
        0x03 => (None, "CAT", true),
        // 비라틴 배열(IME 없음).
        0x19 => (None, "RUS", false),
        0x22 => (None, "UKR", false),
        0x02 => (None, "BUL", false),
        0x23 => (None, "BEL", false),
        0x2f => (None, "MKD", false),
        0x1e => (None, "THA", false),
        0x01 => (None, "ARA", false),
        0x29 => (None, "FAS", false),
        0x20 => (None, "URD", false),
        0x0d => (None, "HEB", false),
        0x08 => (None, "ELL", false),
        0x39 => (None, "HIN", false),
        0x45 => (None, "BEN", false),
        0x49 => (None, "TAM", false),
        0x37 => (None, "KAT", false),
        0x2b => (None, "HYE", false),
        0x3f => (None, "KAZ", false),
        0x50 => (None, "MON", false),
        _ => (None, "???", false),
    }
}

/// LANGID + (IME 언어면) 변환 모드의 본연 비트 → 상태. 순수 함수(테스트).
#[cfg_attr(not(any(windows, target_os = "macos", test)), allow(dead_code))]
pub(crate) fn classify(langid: u16, native: bool) -> ImeState {
    let (ime, lang, latin) = describe(langid & 0x3FF);
    match ime {
        Some((nat, lat)) => ImeState {
            glyph: if native { nat } else { lat },
            lang,
            latin: !native,
        },
        None => ImeState {
            glyph: if latin { "A" } else { lang },
            lang,
            latin,
        },
    }
}

/// 지금 상태 — `hwnd` = 우리 창(Windows · 없으면 전경 창). 판정 못 하면 `None`.
#[cfg(windows)]
pub(crate) fn current(hwnd: Option<isize>) -> Option<ImeState> {
    #[link(name = "user32")]
    extern "system" {
        fn GetForegroundWindow() -> isize;
        fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
        fn GetKeyboardLayout(thread: u32) -> isize;
        fn SendMessageW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize;
    }
    #[link(name = "imm32")]
    extern "system" {
        fn ImmGetDefaultIMEWnd(hwnd: isize) -> isize;
    }
    const WM_IME_CONTROL: u32 = 0x0283;
    const IMC_GETCONVERSIONMODE: usize = 0x0001;
    const IME_CMODE_NATIVE: isize = 0x0001;
    // SAFETY: 인자 없는 조회 API · 핸들이 0이어도 실패값만 돌아온다.
    unsafe {
        let hwnd = hwnd
            .filter(|&h| h != 0)
            .unwrap_or_else(|| GetForegroundWindow());
        if hwnd == 0 {
            return None;
        }
        let tid = GetWindowThreadProcessId(hwnd, std::ptr::null_mut());
        let hkl = GetKeyboardLayout(tid);
        if hkl == 0 {
            return None;
        }
        let langid = (hkl as usize & 0xFFFF) as u16;
        let is_ime_lang = matches!(langid & 0x3FF, 0x12 | 0x11 | 0x04);
        let native = if is_ime_lang {
            let ime_wnd = ImmGetDefaultIMEWnd(hwnd);
            if ime_wnd == 0 {
                // IME 창이 없으면(IME 끔) 라틴으로 본다.
                false
            } else {
                SendMessageW(ime_wnd, WM_IME_CONTROL, IMC_GETCONVERSIONMODE, 0) & IME_CMODE_NATIVE
                    != 0
            }
        } else {
            false
        };
        Some(classify(langid, native))
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn current(_hwnd: Option<isize>) -> Option<ImeState> {
    match nexa_sys::input_source::is_korean() {
        Some(true) => Some(classify(0x0412, true)),
        Some(false) => Some(classify(0x0409, false)),
        None => None,
    }
}

/// Linux(09-27 사용자 "리눅스에서 안 보인다"): 입력기 프레임워크에 **지금 엔진 이름**을 묻는다 — ibus = `ibus engine`(`hangul` → 가 ·
/// `anthy`/`mozc`/`kkc` → あ · `pinyin`/`libpinyin`/`chewing`/`rime` → 中 · `xkb:…` 배열 = 라틴) · fcitx5 = `fcitx5-remote -n`.
/// 프로세스 하나를 띄우는 조회라 **500 ms 캐시**(가린 칸에 포커스가 있을 때만 불린다 · 39 §3) · 프레임워크가 없으면 `None`.
#[cfg(not(any(windows, target_os = "macos")))]
pub(crate) fn current(_hwnd: Option<isize>) -> Option<ImeState> {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    /// (조회 시각, (엔진 이름, 상태)) — 프레임워크가 없으면 안쪽 None.
    type Probe = Option<(String, ImeState, bool)>;
    static CACHE: Mutex<Option<(Instant, Probe)>> = Mutex::new(None);
    let cached = CACHE.lock().ok().and_then(|g| {
        g.as_ref()
            .filter(|(at, _)| at.elapsed() < Duration::from_millis(500))
            .map(|(_, st)| st.clone())
    });
    let probed = match cached {
        Some(c) => c,
        None => {
            let now = linux_probe();
            if let Ok(mut g) = CACHE.lock() {
                // 엔진이 바뀌었으면 입력 종류 기억은 버린다.
                let prev = g
                    .as_ref()
                    .and_then(|(_, s)| s.as_ref().map(|(e, _, _)| e.clone()));
                on_engine_change(prev != now.as_ref().map(|(e, _, _)| e.clone()));
                *g = Some((Instant::now(), now.clone()));
            }
            now
        }
    };
    let (engine, st, initial_native) = probed?;
    let _ = engine;
    if !st.latin {
        // 엔진 안 한/영 상태(ibus-hangul · 사용자 09-27): 토글 키·실제 입력으로 알게 된 값 → 없으면 엔진의 초기 모드 설정(`initial-input-mode`).
        let native = match LAST_INPUT.load(std::sync::atomic::Ordering::Relaxed) {
            1 => false,
            2 => true,
            _ => initial_native,
        };
        if !native {
            return Some(classify(0x0409, false));
        }
    }
    Some(st)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn linux_probe() -> Option<(String, ImeState, bool)> {
    let xmod = std::env::var("XMODIFIERS").unwrap_or_default();
    let qt = std::env::var("QT_IM_MODULE").unwrap_or_default();
    let gtk = std::env::var("GTK_IM_MODULE").unwrap_or_default();
    let all = format!("{xmod} {qt} {gtk}");
    let engine = if all.contains("ibus") {
        run_quiet("ibus", &["engine"])
    } else if all.contains("fcitx") {
        run_quiet("fcitx5-remote", &["-n"]).or_else(|| run_quiet("fcitx-remote", &["-n"]))
    } else {
        None
    }?;
    let st = engine_state(&engine);
    // ibus-hangul의 초기 모드(`initial-input-mode` · 기본 latin) — 토글·입력을 보기 전의 상태.
    let initial_native = st.glyph == "가"
        && run_quiet(
            "gsettings",
            &[
                "get",
                "org.freedesktop.ibus.engine.hangul",
                "initial-input-mode",
            ],
        )
        .is_some_and(|v| v.contains("hangul"));
    Some((engine, st, initial_native))
}

/// 엔진 이름 → 상태(순수 · 시험).
#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
fn engine_state(engine: &str) -> ImeState {
    let e = engine.trim().to_ascii_lowercase();
    let (primary, native) = if e.contains("hangul") || e.contains("korean") {
        (0x12, true)
    } else if e.contains("anthy") || e.contains("mozc") || e.contains("kkc") || e.contains("skk") {
        (0x11, true)
    } else if e.contains("pinyin")
        || e.contains("chewing")
        || e.contains("rime")
        || e.contains("cangjie")
        || e.contains("wubi")
    {
        (0x04, true)
    } else {
        // `xkb:us::eng` · `xkb:kr::kor`(한글 배열 = 영문 모드) · `keyboard-us` — 라틴.
        (0x09, false)
    };
    classify(primary, native)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn run_quiet(cmd: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 한국어 IME: 한글 모드 = 가(안내) · 영문 모드 = A(안내 없음) · 일본어 あ · 중국어 中/英 · 라틴 배열 = 안내 없음 · 비라틴 배열 = 코드.
    #[test]
    fn classify_by_language_and_mode() {
        let ko = classify(0x0412, true);
        assert_eq!((ko.glyph, ko.lang, ko.latin), ("가", "KOR", false));
        let ko_en = classify(0x0412, false);
        assert_eq!((ko_en.glyph, ko_en.latin), ("A", true));
        assert_eq!(classify(0x0411, true).glyph, "あ");
        assert_eq!(classify(0x0804, false).glyph, "英");
        assert_eq!(classify(0x0404, true).glyph, "中");
        let en = classify(0x0409, false);
        assert!(en.latin && en.glyph == "A" && en.lang == "ENG");
        assert!(
            classify(0x0407, true).latin,
            "독일어 배열은 IME가 없어 native 비트를 무시"
        );
        let ru = classify(0x0419, false);
        assert_eq!((ru.glyph, ru.latin), ("RUS", false));
        let unknown = classify(0x03ff, false);
        assert!(!unknown.latin, "모르는 언어는 안내한다");
    }

    /// 감시가 살아 있으면 엔진 이름이 바뀌어도(다른 창에서 바꾸고 돌아옴 · 500 ms 재조회) 감시가 준 값이 남고 · 폴백이면 "모름"으로.
    #[test]
    fn watch_value_survives_engine_change() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_watch_active(true);
        set_from_watch(true);
        on_engine_change(true);
        assert_eq!(LAST_INPUT.load(std::sync::atomic::Ordering::Relaxed), 2);
        note_input(true);
        assert_eq!(
            LAST_INPUT.load(std::sync::atomic::Ordering::Relaxed),
            2,
            "감시 중 키보드 판정은 무시"
        );
        set_watch_active(false);
        on_engine_change(false);
        assert_eq!(LAST_INPUT.load(std::sync::atomic::Ordering::Relaxed), 2);
        on_engine_change(true);
        assert_eq!(LAST_INPUT.load(std::sync::atomic::Ordering::Relaxed), 0);
        note_input(true);
        assert_eq!(LAST_INPUT.load(std::sync::atomic::Ordering::Relaxed), 1);
        LAST_INPUT.store(0, std::sync::atomic::Ordering::Relaxed);
    }

    /// 전역 상태를 만지는 시험은 하나씩.
    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// 이 PC의 실제 입력기 상태(ibus/fcitx 필요 · `cargo test -p nexa-sql imestate -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn linux_live_probe() {
        let st = current(None);
        eprintln!("live probe = {st:?}");
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            note_input(false);
            eprintln!("after native input = {:?}", current(None));
            note_toggle(false);
            eprintln!("after toggle(from native) = {:?}", current(None));
        }
    }

    #[test]
    fn linux_engine_names_map_to_states() {
        assert_eq!(engine_state("hangul").glyph, "가");
        assert!(!engine_state("hangul").latin);
        assert_eq!(engine_state("mozc-jp").glyph, "あ");
        assert_eq!(engine_state("libpinyin").glyph, "中");
        assert!(engine_state("xkb:us::eng").latin);
        assert!(
            engine_state("xkb:kr::kor").latin,
            "한글 배열(IME 아님) = 영문"
        );
    }
}
