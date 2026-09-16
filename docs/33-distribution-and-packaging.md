# 33 · 배포판 설계 — 설치본만(포터블 없음) · 목적별 실행파일 · 공유/정적 라이브러리 분리 · macOS 일관성

> **요청**(사용자 09-15): *"포터블 배포는 하지 않을 것 · 목적별 실행파일을 분리하고 공유/정적 라이브러리도 별도 구성해서 설치본으로 배포 · macOS의 특징을 최대한 고려해 일관성 있는 배포판"*.
> **선행**: [22 드라이버 확장](22-driver-extensions.md)(SxS 드라이버 프로세스) · [25 §9](25-license-tiers-and-server.md)(저장소 분리) · nexa-clip `packaging/`(3-OS 파이프라인 · choco/winget/homebrew/linux) · [01 아키텍처](01-architecture.md).
> **상태**: ✅ **DR-27** · **DR-32**(09-16: D-49 MSI · D-50 pkg 링크) · **T-72/T-62 ✅ 09-16 49차**(맥 실기 pkg/dmg · MSI/deb/rpm은 CI · 서명·매니페스트 후속 · [journal 49차](journal/2026-09-16.md)).

---

## 0. 원칙

1. **설치본만 배포한다.** 포터블(zip 풀어 실행 · exe 옆 `data\`)은 만들지 않는다. 사용자 데이터·설정·프로필은 항상 **OS 사용자 폴더**(`%APPDATA%\nexa-sql` · `~/Library/Application Support/nexa-sql` · `~/.config/nexa-sql`). `NSQL_HOME` 재지정은 개발·테스트 전용.
2. **실행파일은 목적별로 나눈다** — GUI · CLI · (드라이버 프로세스) · (라이선스 도구). 한 exe에 여러 모드를 욱여넣지 않는다(`nexa-sql --cli` 같은 것 없음).
3. **공통 코드는 라이브러리로 한 번만** — Rust 크레이트(`nsql-core/script/run/io/catalog/…`)는 **정적**(rlib · 각 exe에 링크). OS/외부 런타임(Oracle Instant Client · 향후 ODBC)은 **공유 라이브러리로 별도 위치**에 두고 실행 시 로드한다. 설치본 안에서 한 사본을 모든 exe가 공유.
4. **3-OS에서 같은 레이아웃 의미** — 폴더 이름과 역할이 같고, OS 관례(macOS `.app` 번들 · Windows `Program Files` · Linux FHS)에만 맞춘다.
5. **서명·검증은 파이프라인 단계**(DR-20 코드 서명은 별도 요청 시 · 다운로드 산출물 sha256 + Ed25519는 [22 §4](22-driver-extensions.md)).

---

## 1. 산출물(실행파일 · 라이브러리)

| 산출물 | 종류 | 무엇 | 링크 |
|---|---|---|---|
| `nexa-sql` (`Nexa SQL.app` on mac) | exe(GUI) | 편집기·그리드·탐색기·접속 창 | 정적 Rust 크레이트 전부 · nexa-ui |
| `nsql` | exe(CLI) | run/shell/export/conn/cat/config — GUI와 같은 코어 | 정적 |
| `nsql-driver-<id>` | exe(드라이버 프로세스 · [22](22-driver-extensions.md)) | stdio JSON-RPC · SxS `drivers/<id>/<ver>/` | 정적 + 그 드라이버의 공유 lib |
| `nexa-license-tool` | exe(발급기 · 비공개 저장소) | 배포판에 **포함하지 않음** | — |
| Oracle Instant Client | 공유 lib(`oci.dll`·`libclntsh.dylib`·`libclntsh.so`) | 사용자 설치(D-1 재배포 조건) 또는 드라이버 확장이 내려받음 · **설치본에 동봉 안 함** | 런타임 dlopen(ODPI-C) |
| 폰트·아이콘·문법 패키지 | 자원 | `Packages/*.nexa-syntax` · 아이콘(`packaging/branding` SSOT) · 한글 폴백 폰트는 OS 폰트 우선(14) | 파일 |

Rust 크레이트를 **동적 라이브러리(cdylib)** 로 나누지 않는 이유: Rust ABI 불안정 · 두 exe가 같은 코어를 정적으로 가져도 합계 수 MB · 갱신은 설치본 단위. "공유 라이브러리 분리"는 **OS 런타임·드라이버 계층**에 적용한다.

---

## 2. 레이아웃(3-OS)

```
Windows  %ProgramFiles%\Nexa SQL\
           nexa-sql.exe · nsql.exe · lib\ (향후 공유 dll) · Packages\ · LICENSE · THIRD-PARTY-NOTICES
           (드라이버 확장 = %LOCALAPPDATA%\nexa-sql\drivers\<id>\<ver>\ — 사용자 단위 · [22])
macOS    /Applications/Nexa SQL.app/
           Contents/Info.plist · Contents/MacOS/nexa-sql (GUI) · Contents/MacOS/nsql (CLI · 심볼릭 링크 /usr/local/bin/nsql는 설치 후 스크립트)
           Contents/Frameworks/ (공유 dylib · @rpath) · Contents/Resources/ (Packages · icon.icns · 폰트 폴백)
           설정/프로필 = ~/Library/Application Support/nexa-sql/ · 로그 = ~/Library/Logs/nexa-sql/
Linux    /usr/bin/nexa-sql · /usr/bin/nsql · /usr/lib/nexa-sql/ (공유 so) · /usr/share/nexa-sql/Packages/ · /usr/share/applications/nexa-sql.desktop · hicolor 아이콘
```

**macOS 특징 반영**(일관성의 핵심):

| 항목 | 방침 |
|---|---|
| 번들 | GUI는 반드시 `.app`(Info.plist · `CFBundleIdentifier com.sosomlab.nexa-sql` · `LSMinimumSystemVersion` · Retina `NSHighResolutionCapable`) · CLI는 번들 안 `Contents/MacOS/nsql` + `/usr/local/bin` 링크(Homebrew cask 관례) |
| 아키텍처 | **Universal 2**(arm64 + x86_64 `lipo`) 단일 번들 — Instant Client는 아키텍처별(사용자 설치 · [journal 09-13 mac](journal/2026-09-13.md) 규칙 `~/Oracle/instantclient`) |
| 공유 lib | `Contents/Frameworks/` + `@rpath` · 외부 lib 경로는 `NSQL_ORACLE_CLIENT_DIR`/기본 탐색(`~/lib` 심볼릭 링크 관행) |
| 서명·공증 | codesign(Developer ID) + notarytool + stapler는 **파이프라인 단계로 자리만**(DR-20 · 별도 요청 시 키 투입) · 미서명 빌드는 Gatekeeper 안내 문서 |
| 설치 형식 | **`.pkg`**(설치본 원칙 · 스크립트로 CLI 링크) + 편의용 `.dmg`(drag-to-Applications)도 같은 번들에서 생성 · Homebrew cask(nexa-clip `packaging/homebrew` 이식) |
| 데이터 | `~/Library/Application Support/nexa-sql/` · 로그 `~/Library/Logs/` · 캐시 `~/Library/Caches/` — Windows/Linux의 같은 역할 폴더와 1:1 |
| 창·메뉴 | 메뉴바는 앱 내 그리기(nexa-ui · 3-OS 동일) · ⌘ 단축키는 키맵 표의 mac 열(09-15 keymap) |

**Windows**: MSI(WiX) 또는 NSIS — 설치본 원칙상 **MSI 권장**(관리자 배포·조용한 설치 `msiexec /qn` · 기업 환경) + winget/choco 매니페스트(nexa-clip `packaging/winget`·`choco` 이식). 파일 연결 `.sql`은 선택 설치 옵션.
**Linux**: `.deb` + `.rpm` + AppImage 없음(포터블 성격) — 저장소 매니페스트는 후속.

---

## 3. 파이프라인(T-72)

| 단계 | 내용 | 원천 |
|---|---|---|
| build ✅ | `packaging/macos/build-app.sh`(두 타깃 `--locked` + lipo) · `windows/build-msi.ps1` · `linux/build-deb.sh` — 로컬·CI 같은 스크립트 | nexa-clip 이식 |
| assemble ✅ | `packaging/lib.sh stage_common`(LICENSE 2종·README·THIRD-PARTY-NOTICES·`Packages/`) · 아이콘 = branding SSOT(iconutil / `.rc`+build.rs / hicolor 8종) | `packaging/<os>/` |
| package ✅ | MSI(WiX v4 `nexa-sql.wxs` · UpgradeCode 고정 · PathEnv/SqlAssoc Feature) · pkg(pkgbuild+productbuild · postinstall 링크 = DR-32)+dmg · deb(dpkg-deb)/rpm(spec · 같은 스테이징) | 맥 실기 pkg/dmg · MSI/deb/rpm은 CI |
| sign 📐 | 자리만 — `MACOS_SIGN_IDENTITY`/`MACOS_INSTALLER_IDENTITY`/`MACOS_NOTARY_PROFILE` · `WINDOWS_SIGN_PFX(_PASSWORD)`/`WINDOWS_SIGN_THUMBPRINT` 없으면 unsigned | DR-20 |
| verify ✅(CI) | release.yml 스모크 = 실제 설치 → `nsql --version`·`nexa-sql --smoke` → 제거(uninstall.sh / msiexec /x / dpkg -r) → 잔여 0 · rpm은 목록만 | — |
| publish ✅ 초안 | `sha256sums.txt` + GitHub Release **초안**(`gh release create --draft`) · 매니페스트(winget/choco/brew)는 후속 | [22 §4](22-driver-extensions.md) |

---

## 4. 결정 · 작업

| # | 결정 |
|---|---|
| **DR-27** | **배포 = 설치본만**(MSI · pkg/dmg · deb/rpm) · 포터블 없음 · 목적별 exe(GUI `nexa-sql` · CLI `nsql` · 드라이버 프로세스) · Rust 코어는 정적 · OS 런타임/드라이버 lib은 공유 lib 별도 위치 · macOS `.app`+Universal 2+`.pkg` · 사용자 데이터는 OS 사용자 폴더 |
| ~~D-49~~ | → **DR-32** = MSI(WiX v4) · 09-16 |
| ~~D-50~~ | → **DR-32** = pkg postinstall `/usr/local/bin/nsql` 링크 · 09-16 |

| ID | 항목 | 의존 |
|---|---|---|
| **T-72** | 배포 파이프라인 — `packaging/{windows,macos,linux}` 스테이징·패키징 스크립트(nexa-clip 이식) · CI `release.yml`(태그 → 3-OS 산출물 · sha256) · `--smoke` 검증 · 제거 검증 | T-62 T-10 D-49 D-50 |
| T-62 | 아이콘 배포(.rc · .icns · .desktop) | — |
