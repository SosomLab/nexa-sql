//! # nsql-script — SQL*Plus식 스크립트 엔진 (DB 무의존 · docs/08)
//!
//! 편집기 버퍼(또는 CLI 입력)를 **항목**([`split::Item`])으로 나누고, 각 항목을
//! **행동**([`engine::Action`])으로 계획한다. 엔진은 DB를 모른다 — 호스트(GUI·CLI)가
//! `Action::Execute`를 드라이버에 넘기고 [`engine::Engine::absorb`]로 결과를 되돌려 준다.
//!
//! ```text
//! 텍스트 ─split─▶ Item ─engine.plan─▶ Action ─호스트─▶ 드라이버 ─ExecResult─▶ engine.absorb
//!                                   └ LocalAssign / Print / Connect … (DB 왕복 없음)
//! ```
//!
//! 핵심 불변식(docs/04 §8.2 · SQL*Plus 동일): **변수는 클라이언트 메모리에만 산다.**
//! 방언 차이는 [`dialect`]가 흡수한다 — Oracle은 `:NAME` 그대로, SQL Server는
//! `sp_executesql` 파라미터(1차) / `DECLARE` 프리펜드(2차), PG는 `$n`, 나머지는 `?`.

#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod bind;
pub mod command;
pub mod connect;
pub mod dialect;
pub mod engine;
pub mod lexer;
pub mod split;
pub mod vars;

pub use bind::{extract_binds, BindRef};
pub use command::{Command, SetOption};
pub use connect::ConnectSpec;
pub use dialect::{prepare, PrepareMode, Prepared};
pub use engine::{Action, Diagnostic, Engine};
pub use split::statement_at;
pub use split::{split_script, Item, ItemKind, SqlKind};
pub use vars::{Var, VarStore};
