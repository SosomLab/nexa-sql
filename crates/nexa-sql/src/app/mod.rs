//! `App` 기능 모듈 — 상태(`App` 구조체)는 main.rs 한 곳, 동작은 기능별 파일(docs/93 §4).
//!
//! 새 동작은 해당 기능 파일에 둔다. 두 기능에 걸치면 호출하는 쪽(상위 흐름) 파일에.

mod bookmarks;
pub(crate) mod compare;
mod completion;
mod connwin;
mod demo;
pub(crate) mod drop;
mod event_loop;
mod events;
mod extensions;
mod files;
mod find;
mod format;
pub(crate) mod goto;
mod grid_results;
mod input;
mod license;
mod live;
mod memory;
mod menus;
mod meta;
pub(crate) mod objlink;
mod output;
mod paint;
mod project;
mod run;
mod session;
mod settings;
mod startup_cmd;
pub(crate) mod tabobj;
mod toolbar;
mod tx;
mod vars;
mod windows;
mod zoom;
