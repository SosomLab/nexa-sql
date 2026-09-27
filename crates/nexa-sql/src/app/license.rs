//! App — 라이선스 게이트·창·배지(docs/23·25 · T-36).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    pub(crate) fn open_license_window(&mut self, el: &ActiveEventLoop) {
        self.licensing.refresh();
        let owner = self.window.clone();
        let was_open = self.license_win.is_open();
        self.license_win.open(
            el,
            theme::window_theme(self.settings.theme_mode()),
            owner.as_deref(),
        );
        if !was_open {
            if let (Some(o), Some(c)) = (owner.as_deref(), self.license_win.window()) {
                winfocus::attach_child(o, c);
            }
        }
        self.sync_modal();
    }

    /// About 창 줄들(제품 · 버전 · 빌드일 · 시스템 · 라이선스 · 저작권 · 조건 · 저장소).
    pub(crate) fn about_lines(&self) -> Vec<(String, String)> {
        vec![
            (
                String::new(),
                format!("Nexa SQL {}", env!("CARGO_PKG_VERSION")),
            ),
            (
                t(Msg::AboutVersion).into(),
                env!("CARGO_PKG_VERSION").to_string(),
            ),
            (
                t(Msg::AboutBuild).into(),
                nsql_license::PRODUCT.build_date.to_string(),
            ),
            (
                t(Msg::AboutSystem).into(),
                format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
            ),
            (t(Msg::AboutLicenseState).into(), self.license_badge()),
            (
                t(Msg::AboutRepo).into(),
                "https://github.com/SosomLab/nexa-sql".to_string(),
            ),
            (String::new(), t(Msg::AboutCopyright).to_string()),
            (String::new(), t(Msg::AboutTerms).to_string()),
        ]
    }

    /// 상태줄 배지 글(docs/23 §4-4): `Free · non-commercial use only` / `Pro · ACME` / `⚠ …`.
    fn license_badge(&self) -> String {
        Self::license_badge_of(&self.licensing)
    }

    /// 필드만 빌린다(상태줄 paint 중 `surface`가 가변 빌림 상태라 `&self`를 못 쓴다).
    pub(crate) fn license_badge_of(lic: &nsql_license::Licensing) -> String {
        use nsql_license::{LicenseState, Tier};
        match lic.state() {
            LicenseState::Free => t(Msg::StLicFree).to_string(),
            LicenseState::Licensed(l) => {
                let tier = match Tier::parse(&l.tier) {
                    Tier::Trial => t(Msg::LicTierTrial),
                    Tier::Org => t(Msg::LicTierOrg),
                    Tier::Pro | Tier::Free => t(Msg::LicTierPro),
                };
                tf(Msg::StLicLicensed, &[tier, &l.licensee])
            }
            LicenseState::Invalid(i) => {
                tf(Msg::StLicInvalid, &[&format!("{i:?}").to_ascii_lowercase()])
            }
            LicenseState::Outdated(_) => t(Msg::StLicOutdated).to_string(),
            LicenseState::Expired(l) => tf(Msg::StLicExpired, &[&l.expires]),
        }
    }

    /// 라이선스 창 보기(표 · 요청 코드) — 그릴 때마다 만든다(값 몇 줄).
    pub(crate) fn license_view(&self) -> LicView {
        use nsql_license::LicenseState;
        let st = self.licensing.state();
        let warn = !matches!(st, LicenseState::Licensed(_));
        let dash = || "-".to_string();
        let mut rows: Vec<(String, String)> = vec![
            (t(Msg::LicLblState).into(), st.name().to_string()),
            (
                t(Msg::LicLblFile).into(),
                self.licensing
                    .path()
                    .map_or_else(dash, |p| p.display().to_string()),
            ),
        ];
        if let Some(l) = st.license() {
            let feats = if l.features.contains("*") {
                "*".to_string()
            } else {
                nsql_license::features_of(l)
                    .into_iter()
                    .map(|f| f.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            rows.push((t(Msg::LicLblId).into(), l.id.clone()));
            rows.push((t(Msg::LicLblLicensee).into(), l.licensee.clone()));
            rows.push((
                t(Msg::LicLblKind).into(),
                format!("{} / {}", l.kind, l.tier),
            ));
            rows.push((t(Msg::LicLblFeatures).into(), feats));
            rows.push((t(Msg::LicLblIssued).into(), l.issued.clone()));
            rows.push((t(Msg::LicLblUntil).into(), l.updates_until.clone()));
            rows.push((
                t(Msg::LicLblExpires).into(),
                if l.expires.is_empty() {
                    dash()
                } else {
                    l.expires.clone()
                },
            ));
            rows.push((
                t(Msg::LicLblMaxMajor).into(),
                l.max_major.map_or_else(dash, |m| format!("{m}.x")),
            ));
            if !l.max_version.is_empty() {
                rows.push((t(Msg::LicLblMaxVersion).into(), l.max_version.clone()));
            }
        }
        rows.push((
            t(Msg::LicLblVersion).into(),
            nsql_license::PRODUCT.version.to_string(),
        ));
        rows.push((
            t(Msg::LicLblBuild).into(),
            nsql_license::PRODUCT.build_date.to_string(),
        ));
        rows.push((
            t(Msg::LicLblMachine).into(),
            nsql_license::Licensing::machine_code().unwrap_or_else(dash),
        ));
        rows.push((
            t(Msg::LicLblInstallTo).into(),
            self.licensing
                .primary_path()
                .map_or_else(dash, |p| p.display().to_string()),
        ));
        LicView {
            state: self.license_badge(),
            warn,
            rows,
            request: nsql_license::Licensing::request_code(&nsql_license::RequestMeta::default()),
            contact: nsql_license::LICENSE_CONTACT.to_string(),
        }
    }

    /// 파일 창/기동 명령 → 설치(검증 통과만 씀 · 원본 보존) → 창 안내 + 상태줄 + 배지 갱신.
    pub(crate) fn license_install(&mut self, path: &Path) {
        use nsql_license::InstallError;
        let (note, warn) = match self.licensing.install(path) {
            Ok(l) => (tf(Msg::LicNoteInstalled, &[&l.id]), false),
            Err(InstallError::Rejected(s)) => (tf(Msg::LicNoteRejected, &[s.name()]), true),
            Err(InstallError::Io(e)) => (tf(Msg::LicNoteIo, &[&e.to_string()]), true),
        };
        self.sess.status = note.clone();
        self.license_win.set_note(note, warn);
        self.redraw();
    }

    pub(crate) fn license_remove(&mut self) {
        let (note, warn) = match self.licensing.remove() {
            Ok(true) => (t(Msg::LicNoteRemoved).to_string(), false),
            Ok(false) => (t(Msg::LicNoteNothing).to_string(), true),
            Err(e) => (tf(Msg::LicNoteIo, &[&e.to_string()]), true),
        };
        self.sess.status = note.clone();
        self.license_win.set_note(note, warn);
        self.redraw();
    }

    pub(crate) fn license_copy_request(&mut self, name: &str, email: &str) {
        let meta = nsql_license::RequestMeta {
            name: name.to_string(),
            email: email.to_string(),
        };
        let (note, warn) = match nsql_license::Licensing::request_code(&meta) {
            Some(code) if clipboard::write_text(&code) => {
                (t(Msg::LicNoteCopied).to_string(), false)
            }
            Some(_) => (t(Msg::LicNoteCopyFailed).to_string(), true),
            None => (t(Msg::LicNoMachine).to_string(), true),
        };
        self.license_win.set_note(note, warn);
    }

    /// 게이트가 켜져 있는가 — Release 빌드는 늘 · Debug는 HIDDEN 설정(개발·자동 시험 기본 off).
    fn gates_on(&self) -> bool {
        !cfg!(debug_assertions) || self.settings.flag("license.gates_dev")
    }

    /// 순수 판정(안내 없음) — 상한식·조용한 폴백용.
    pub(crate) fn entitled(&self, f: nsql_license::Feature) -> bool {
        !self.gates_on() || self.licensing.check(f).is_allowed()
    }

    /// ★ 행위 게이트: 허용이면 true · 거부면 상태줄 안내 + 라이선스 창(요청 코드 복사)으로 안내(D-43 · 기능은 숨기지 않는다).
    pub(crate) fn lic_gate(&mut self, f: nsql_license::Feature) -> bool {
        if self.entitled(f) {
            return true;
        }
        let msg = self.license_denied_text(f);
        self.sess.status = msg.clone();
        self.license_win.set_note(msg, true);
        self.open_license = true;
        self.redraw();
        false
    }

    /// 상한식(D-46 · TablePlus식): 무료면 `cap`까지 · 막지 않고 값만 줄인다.
    pub(crate) fn cap(&self, f: nsql_license::Feature, n: usize, cap: usize) -> usize {
        if self.entitled(f) {
            n
        } else {
            n.min(cap)
        }
    }

    /// 상한에 닿았을 때 한 줄(상태줄만 · 창 없음).
    pub(crate) fn license_limit_note(&mut self, f: nsql_license::Feature) {
        let (label, alt) = Self::feature_texts(f);
        self.sess.status = tf(Msg::LicLimit, &[label, alt]);
    }

    pub(crate) fn license_denied_text(&self, f: nsql_license::Feature) -> String {
        let (label, alt) = Self::feature_texts(f);
        tf(Msg::LicDenied, &[label, alt])
    }

    /// 기능 이름 · 무료 대체 경로(25 §13-3 "대체" 열 = 비면 게이트 후보에서 뺀다).
    fn feature_texts(f: nsql_license::Feature) -> (&'static str, &'static str) {
        use nsql_license::Feature as F;
        match f {
            F::ImportGui => (t(Msg::LicFeatImportGui), t(Msg::LicAltImportGui)),
            F::DriverExtensions => (
                t(Msg::LicFeatDriverExtensions),
                t(Msg::LicAltDriverExtensions),
            ),
            F::ProjectRestore => (t(Msg::LicFeatProjectRestore), t(Msg::LicAltProjectRestore)),
            F::BookmarkGroups => (t(Msg::LicFeatBookmarkGroups), t(Msg::LicAltBookmarkGroups)),
            F::GrammarCompletion => (
                t(Msg::LicFeatGrammarCompletion),
                t(Msg::LicAltGrammarCompletion),
            ),
            F::LobImage => (t(Msg::LicFeatLobImage), t(Msg::LicAltLobImage)),
            F::RichCopy => (t(Msg::LicFeatRichCopy), t(Msg::LicAltRichCopy)),
            F::LogExport => (t(Msg::LicFeatLogExport), t(Msg::LicAltLogExport)),
            F::MultiConnection => (
                t(Msg::LicFeatMultiConnection),
                t(Msg::LicAltMultiConnection),
            ),
            F::MultiTabs => (t(Msg::LicFeatMultiTabs), t(Msg::LicAltMultiTabs)),
            F::LargeEdit => (t(Msg::LicFeatLargeEdit), t(Msg::LicAltLargeEdit)),
            F::ResultTabs => (t(Msg::LicFeatResultTabs), t(Msg::LicAltResultTabs)),
            // 아직 기능 자체가 없는 것(T-8 xlsx · T-3 SSH · 19 비교 · Org 운영 기능) — 게이트 자리도 없다.
            _ => (f.as_str(), ""),
        }
    }

    /// 편집 탭 상한(D-46 무료 5): 자리가 있으면 true · 없으면 상태줄 안내(막는다 = TablePlus식).
    pub(crate) fn tab_room(&mut self) -> bool {
        const FREE_TABS: usize = 5;
        if self.entitled(nsql_license::Feature::MultiTabs)
            || self.editors.tab_titles().len() < FREE_TABS
        {
            return true;
        }
        self.license_limit_note(nsql_license::Feature::MultiTabs);
        self.redraw();
        false
    }
}
