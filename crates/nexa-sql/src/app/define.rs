//! App — 상태줄 `&` 치환 세그먼트(D-141 "상태줄 토글" · T-149 잔여 · 10-30).
//!
//! 표시 = 설정 `script.define`(auto | on | off) 그대로 `& auto` · `& on` · `& off`(off = 흐림) · 클릭 = 팝업으로 전환 ·
//! 전환 = 설정 저장 + **모든 세션의 러너**에 `Cmd::DefineMode`(자동 커밋 토글과 같은 통제 명령 · DB로 가는 것 없음 · 52 §3).
//! 스크립트의 `SET DEFINE`은 그 세션 안에서 여전히 우선이다(세그먼트는 설정값을 보인다).

use crate::*;

impl App {
    /// 세그먼트 글과 밝기(`off`만 흐림).
    pub(crate) fn define_seg_text(&self) -> (String, bool) {
        let mode = self.settings.get("script.define").unwrap_or("auto");
        (format!("& {mode}"), mode != "off")
    }

    /// 클릭 팝업: auto(DBMS 기본) · on(미정의면 묻기) · off — 지금 값에 ✓.
    pub(crate) fn open_define_menu(&mut self) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let cur = self
            .settings
            .get("script.define")
            .unwrap_or("auto")
            .to_string();
        let mark = |on: bool, s: &str| {
            if on {
                format!("✓ {s}")
            } else {
                format!("   {s}")
            }
        };
        let items = vec![
            CtxItem::item("define.auto", mark(cur == "auto", t(Msg::MnDefineAuto)))
                .with_active(cur == "auto"),
            CtxItem::item("define.on", mark(cur == "on", t(Msg::MnDefineOn)))
                .with_active(cur == "on"),
            CtxItem::item("define.off", mark(cur == "off", t(Msg::MnDefineOff)))
                .with_active(cur == "off"),
        ];
        self.open_status_popup_w(self.status_define_rect, items, 260.0);
    }

    /// 팝업의 답(`auto` · `on` · `off`) — 설정 저장 → 살아 있는 세션 전부에 반영 → 상태줄 안내.
    pub(crate) fn define_pick(&mut self, mode: &str) {
        if !matches!(mode, "auto" | "on" | "off") {
            return;
        }
        let _ = self.settings.set("script.define", mode);
        self.persist_settings();
        self.define_apply_live();
        self.sess.status = tf(Msg::StDefineMode, &[mode]);
        self.redraw();
    }

    /// 지금 설정값을 모든 세션의 러너에(통제 명령 · 문지기 없이) — 설정 화면 변경과 팝업이 같이 쓴다.
    pub(crate) fn define_apply_live(&mut self) {
        let mode = self
            .settings
            .get("script.define")
            .unwrap_or("auto")
            .to_string();
        for s in self.all_sess() {
            s.control(worker::Cmd::DefineMode(mode.clone()));
        }
        self.log_win.push(LogEntry::new(
            LogKind::Info,
            format!("script.define = {mode}"),
        ));
    }
}
