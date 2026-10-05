# 33 · 배포판 설계 — 설치본만(포터블 없음) · 목적별 실행파일 · 공유/정적 라이브러리 분리 · macOS 일관성

> **요청**(사용자 09-15): *"포터블 배포는 하지 않을 것 · 목적별 실행파일을 분리하고 공유/정적 라이브러리도 별도 구성해서 설치본으로 배포 · macOS의 특징을 최대한 고려해 일관성 있는 배포판"*.
> **선행**: [22 드라이버 확장](22-driver-extensions.md)(SxS 드라이버 프로세스) · [25 §9](25-license-tiers-and-server.md)(저장소 분리) · nexa-clip `packaging/`(3-OS 파이프라인 · choco/winget/homebrew/linux) · [01 아키텍처](01-architecture.md).
> **상태**: ✅ **DR-27**(→ **D-78 포터블 모드 재검토 · 09-17 사용자 "큰 방향성에서는 포터블 배포도 고려"** · §0-1) · **DR-32**(09-16: D-49 MSI · D-50 pkg 링크) · **T-72/T-62 ✅ 09-16 49차**(맥 실기 pkg/dmg · MSI/deb/rpm은 CI · 서명·매니페스트 후속 · [journal 49차](journal/2026-09-16.md)).

---

## 0. 원칙

1. **설치본이 기본.** 사용자 데이터·설정·프로필은 **OS 사용자 폴더**(`%APPDATA%\nexa-sql` · `~/Library/Application Support/nexa-sql` · `~/.config/nexa-sql`). **포터블 모드는 D-78로 재검토(09-17 사용자)** — 설계 조건: exe 옆 `data\`(또는 `NSQL_HOME`)가 있으면 **그 폴더를 사용자 폴더로**(설정·프로필·툴바 배치·최근 파일 = 한 파일 트리 · 코드 경로는 지금의 `NSQL_HOME` 규약 그대로) · 자동 갱신·OS 등록(연결 프로그램·PATH) 없음 · zip 채널은 MSI/pkg와 같은 산출물에서 · 기기 키(DPAPI)·라이선스 기기 ID는 포터블에서 어떻게 할지 결정 필요. **그 전까지 새로 저장하는 상태는 전부 설정 레지스트리/`NSQL_HOME` 아래**에 둔다(exe 옆 파일 금지).
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
**Linux**: `.deb` + `.rpm`(**둘 다 배포에 포함** · 사용자 10-05) + AppImage 없음(포터블 성격) — 저장소 = **pkg.sosomlab.com**(`SosomLab/linux-repo` · 서명된 APT + RPM(dnf) · 패키지 파일은 싣지 않고 이 저장소의 Release 자산으로 302 · 최신 정식 릴리스 하나만 · §5-5).

---

## 3. 파이프라인(T-72)

| 단계 | 내용 | 원천 |
|---|---|---|
| build ✅ | `packaging/macos/build-app.sh`(두 타깃 `--locked` + lipo) · `windows/build-msi.ps1` · `linux/build-deb.sh` — 로컬·CI 같은 스크립트 | nexa-clip 이식 |
| assemble ✅ | `packaging/lib.sh stage_common`(LICENSE 2종·README·THIRD-PARTY-NOTICES·`Packages/`) · 아이콘 = branding SSOT(iconutil / `.rc`+build.rs / hicolor 8종) | `packaging/<os>/` |
| package ✅ | MSI(WiX v4 `nexa-sql.wxs` · UpgradeCode 고정 · PathEnv/SqlAssoc Feature) · pkg(pkgbuild+productbuild · postinstall 링크 = DR-32)+dmg · deb(dpkg-deb)/rpm(spec · 같은 스테이징) | 맥 실기 pkg/dmg · MSI/deb/rpm은 CI |
| sign 📐 | 자리만 — `MACOS_SIGN_IDENTITY`/`MACOS_INSTALLER_IDENTITY`/`MACOS_NOTARY_PROFILE` · `WINDOWS_SIGN_PFX(_PASSWORD)`/`WINDOWS_SIGN_THUMBPRINT` 없으면 unsigned | DR-20 |
| verify ✅(CI) | release.yml 스모크 = 실제 설치 → `nsql --version`·`nexa-sql --smoke` → 제거(uninstall.sh / msiexec /x / dpkg -r) → 잔여 0 · rpm은 목록만 | — |
| publish ✅ 초안(v0.1.0 첫 공개 09-28 · v0.1.2 09-30 · v0.1.3 10-02 · v0.1.4 10-04 — 공개 = `gh release edit <태그> --draft=false --latest`) | `sha256sums.txt` + GitHub Release **초안**(`gh release create --draft`) → 확인 뒤 공개 | [22 §4](22-driver-extensions.md) |
| homebrew ✅(09-28) | 릴리스 **공개**(published) 때 `homebrew.yml`: dmg 해시로 Cask(`packaging/homebrew/nexa-sql.rb`) 채움 → `kiros33/homebrew-tap` 반영(`TAP_TOKEN`) → macOS 러너에서 `brew install --cask` → `nsql --version`·`--smoke` → 제거 · 사전 릴리스(`-`)는 건너뜀 | 사용자 09-28 "brew만" — **winget·choco 채널은 두지 않는다**(다른 저장소의 오류·조치 기록 = §5) · v0.1.4 = 37179733964 ✓(탭 41ea3ae · 10-04) |
| linux-repo 🚧(10-05 · 워크플로 작성 · 실제 발행 미검증) | 릴리스 **공개**(published) 때 `linux-repo.yml`(`homebrew.yml`과 같은 시점 · 초안에서는 안 돎 · 사전 릴리스 건너뜀 · 수동 실행 = tag 입력): 자산 확인(`.deb` 없으면 실패 · `.rpm` 없으면 경고) → `SosomLab/linux-repo`에 `repository_dispatch`(`app-released` · app/tag/repo/run_url) → linux-repo `publish`(색인 생성 · GPG 서명) → Cloudflare Pages(pkg.sosomlab.com) · 시크릿 `LINUX_REPO_DISPATCH_TOKEN`이 없으면 알림만(linux-repo 하루 1회 정기 실행이 반영) | 사용자 10-05 "rpm 배포 포함" · "linux-repo 저장소 기준 배포에 포함" · §5-5 |

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

## 5. 형제 저장소의 winget·Chocolatey·Homebrew 오류·조치(09-28 수집 · 사용자 "빌드 구성·배포에 참고")

대상 = nexa-beep · nexa-clip · sosomlab-nexa-viewer(마크다운 뷰어) · nexa-dir2 · nexa-shortcut · nexa-coffee (+ 보조 nexa-memkeeper). 출처는 각 저장소 `docs/journal`·`packaging/README.md`·워크플로. **지금 nexa-sql 채널 = brew만**(사용자 09-28) — winget·choco는 열 때 §5-3을 체크리스트로 쓴다. 이들 저장소에 **MSI를 winget·choco에 낸 사례는 없다**(NSIS·Inno·포터블) — MSI 항목은 일반 지식.

### 5-1. v0.1.0 전에 이미 반영한 것

| 교훈(증상 → 원인) | 출처 | nexa-sql 조치 |
|---|---|---|
| winget 자동 검증 `STATUS_DLL_NOT_FOUND`(0xC0000135) → Rust MSVC 기본 = 동적 CRT → `vcruntime140.dll`(인박스 아님) · CI 러너·개발 PC에는 있어서 안 보임 | beep 09-17 · clip 09-14 | `.cargo/config.toml` 두 MSVC 타깃 `+crt-static` · **임포트 게이트** `scripts/check-imports.ps1`(beep 원본 · PE 헤더 직접)를 `build-msi.ps1` 안에(로컬·CI 공통) · 실측 = GUI 16 · CLI 9 DLL 전부 인박스 |
| env `RUSTFLAGS`가 config.toml의 target rustflags를 통째로 덮음 → CI 산출물만 동적 CRT | beep 09-17 · clip | 워크플로에 `RUSTFLAGS` 없음(유지) · 게이트가 실측으로 막음 |
| **버전 정보 공란** → Defender ML 오탐(`Wacatac`·`Tecabans`) · winget 검증 실패 · 작업 관리자에 내부 이름 | dir2 08-24(Inno 설치형 파일 버전 공란) · coffee 09-17(VERSIONINFO 없음) · memkeeper(FileDescription) | CLI `nsql.exe`가 바로 그 상태였다(ProductName·FileVersion·Company 전부 공란) → `crates/nsql-cli/build.rs` + `packaging/windows/nsql.rc` 신설 · 리소스 삽입 본체 = `packaging/windows/winres.rs`(GUI·CLI 공용 `include!`) · 버전 단일 원천 = Cargo.toml |
| 태그 ≠ Cargo 버전 → brew·설치 검증 실패 | beep 08-11 · clip meta 잡 | release.yml meta 잡이 대조(기존) · 워크스페이스 0.1.0 |
| Cask `verified:` deprecated · `license` 스탠자 미지원 · `depends_on macos: :mojave` 거부 · `brew audit` 경로 인자 불가 → `brew style` | coffee 09-13 · viewer · beep 08-11 | `packaging/homebrew/nexa-sql.rb` = `>= :big_sur` · `verified:`/`license` 없음 · 검증 = `ruby -c` + `brew style` + 실제 `brew install --cask` |
| 서명 없는 앱 + quarantine → SIGKILL(137)·앱 삭제(애드혹 서명으로도 못 넘음) | beep 08-11 · clip 08-11 | Cask `postflight` xattr 제거 + caveats에 공개(서명 = DR-20 별도) |
| `release: published`는 **GITHUB_TOKEN으로 만든/공개한 릴리스**에서 다른 워크플로를 트리거하지 않는다 | viewer | 공개는 사람(또는 사용자 토큰 `gh release edit --draft=false`)이 한다 → `homebrew.yml` 자동 · 자동 공개로 바꾸면 `workflow_call`로 직접 호출(beep·clip 구조) |
| 공개된 버전의 자산·태그를 다시 만들면 매니페스트 SHA가 깨진다 | beep · shortcut · dir2 · coffee | 규칙: **공개 뒤 고칠 것 = 새 버전**(v0.1.1 …) |

### 5-2. 버전·메타데이터 점검표(릴리스마다 · 사용자 09-28 "버전 정보가 비었거나 메타 정보를 부족하게")

| 자리 | 채울 것 | 지금 |
|---|---|---|
| Windows exe VERSIONINFO(모든 exe) | ProductName · ProductVersion · FileVersion · CompanyName · FileDescription · LegalCopyright · OriginalFilename(exe마다 다르게) · InternalName | ✅ `nexa-sql.rc` · `nsql.rc`(09-28) — 드라이버 프로세스 exe가 생기면 같은 `winres.rs`로 |
| MSI(ARP) | Manufacturer · ProductVersion(숫자 3단) · ARPURLINFOABOUT · ARPHELPLINK · ARPPRODUCTICON · UpgradeCode 고정 | ✅ `nexa-sql.wxs` · (선택) ARPCONTACT·ARPURLUPDATEINFO |
| macOS Info.plist | CFBundleVersion · CFBundleShortVersionString(빌드가 @VERSION@ 치환) · CFBundleIdentifier · NSHumanReadableCopyright · LSMinimumSystemVersion 11.0 | ✅ |
| Cask | version · sha256(워크플로가 실제 dmg로) · desc(영문 한 줄) · homepage · zap | ✅ |
| (열 때) winget 매니페스트 | **글 필드 전부 영어**(Publisher · PackageName · ShortDescription · Description · Tags · ReleaseNotes · 기본 로캘 en-US · 사용자 10-04) · ManifestVersion 세 파일 동일(1.12.0) + 스키마 헤더 · Publisher · License · ShortDescription · ReleaseNotesUrl · **DisplayVersion은 PackageVersion과 같으면 넣지 않는다**(dir2 반려) · MSI면 InstallerType `wix`·Scope `machine`·UpgradeCode | — |
| (열 때) choco nuspec | **전부 영어**(title · summary · description · releaseNotes · tags · 스크립트 주석/출력 · VERIFICATION.txt · 사용자 10-04) · 영문 summary·description(이메일 금지 — dir2 반려) · `<copyright>`(beep·coffee 반려) · iconUrl = jsDelivr 태그 고정(raw.githubusercontent 불가) · owners=kiros33 / authors=SosomLab · 다운로드형이면 VERIFICATION.txt 없음 · tags 남용 금지 · ps1 = 영문 + UTF-8 BOM + `${var}:` | — |

### 5-3. winget·choco를 열 때(지금은 닫힘)

- ★ **winget · Chocolatey에 제출하는 내용은 전부 영어로 쓴다**(사용자 10-04 "choco, winget에 제출하는 내용은 모두 영어로") — 범위 = 외부 저장소·검수자에게 나가는 모든 글: winget 매니페스트 3종의 글 필드(Publisher · PackageName · ShortDescription · Description · Tags · ReleaseNotes · 기본 로캘 en-US) · winget-pkgs PR 제목·본문·코멘트(CLA 동의 · 검수자 답변 포함) · Chocolatey nuspec(title · summary · description · releaseNotes · tags) · 패키지 안 스크립트 주석/출력 · VERIFICATION.txt · moderator와 주고받는 코멘트·메일 초안. 제출물이 가리키는 문서(ReleaseNotesUrl · 프로젝트/라이선스 URL)가 한국어뿐이면 **영어 판(또는 영어 절)을 먼저** 마련한다 — 지금 GitHub 릴리스 노트는 한국어 형식이라 채널을 열 때 영어 병기/전환이 선행 과제(TODO T-279). 한글·이메일 주소를 제출물에 넣지 않는다(형제 저장소 반려 사례와 같은 줄). 이 규칙은 "모든 답은 한글로"(사용자와의 대화 · 저장소 내부 문서)와 충돌하지 않는다 — 대상이 외부 패키지 저장소일 때만 영어 · 사용자에게 보고할 때는 제출한 영어 원문 + 한글 요약.

- **winget**: 첫 등록 = `wingetcreate submit <렌더 폴더>`/komac(`update`·winget-releaser는 등록된 패키지만) · `kiros33/winget-pkgs` 포크 재사용 · PAT = classic `repo`+`workflow`(포크 동기화 422) · CLA 동의 코멘트 뒤 그 PR에 push 금지 · 판정은 체크 아이콘이 아니라 **라벨**(`Azure-Pipeline-Passed`·`Validation-Completed`) · `winget validate` exit 40 = 경고 처리 · 검수 중 새 버전 PR 금지(guard 잡 `gh pr list --author`) · 첫 등록 13~18일 · 후속 40분~2시간.
- **choco**: 미승인 버전이 걸려 있으면 새 버전 push = **403** → 같은 버전으로 재제출만 가능 · 승인 판정 = OData `IsApproved=true`(미승인도 목록에 나온다) · 사람 검수 코멘트는 메일·페이지로만(API 없음) — 세션마다 `Reviewed:` 날짜 확인 · 재제출은 빌드 없이 자산 해시만으로(dir2 `resubmit-chocolatey.yml`) · MSI = `fileType='msi'` · `silentArgs='/qn /norestart'` · `validExitCodes=@(0,3010,1641)` · 첫 등록 27~44일.
- **공통**: 스위치 = 변수(`WINGET_PUBLISH`/`CHOCO_PUSH` 기본 false) × 시크릿 이중 게이트 · 꺼져 있어도 매니페스트·nupkg는 만들어 아티팩트로 · 치환 지점 = `render-manifests.sh` 하나(남은 `@X@` = 실패) · 시크릿은 유무·스코프만 출력.
- **복사할 원본**: nexa-coffee `publish-windows-packages.yml`(가장 성숙 — channels·force·IsApproved·exit 40·포크 동기화·재시도) + `packaging/winget/*.yaml` · nexa-beep `packaging/`(render · winget/choco 템플릿 · README) · nexa-dir2 `resubmit-chocolatey.yml`·`packaging/av-false-positive.md`(오탐 신고).

### 5-4. 남은 확인

- 정적 CRT로 링크한 Oracle 드라이버(ODPI-C · 런타임에 `oci.dll` 적재)의 **Windows 실서버 접속 확인** — SQLite·GUI 자체 점검(`--smoke`)은 ✓(09-28), 실서버는 사용자 PC에서.
- 서명(DR-20)이 없어 SmartScreen·Gatekeeper 경고는 남는다 — 무료 근본 해결은 Store(MSIX)뿐(dir2·viewer 조사).

> 10-04(맥 111차): DBMS 아이콘 PNG 173종(`include_bytes` · 자산 3.2 MB)을 GUI에 내장 → Release `nexa-sql`(aarch64 · 실제는 Homebrew rustc 빌드) 16,607,800 B · 전 기록 15.0 MB(10-02) 대비 **약 +0.8~0.9 MB 추정**(PNG 이미 압축 · SVG 13 제거분 상쇄 · 정밀 전 값 미측정) · `nsql` 8,008,152 B(변화 없음 · 아이콘은 GUI만) · [journal 10-04 §5](journal/2026-10-04.md).
> 10-04 후속(T-277): DBMS 아이콘 중 devicon(MIT) 27종의 MIT 고지와 상표 문구(`crates/nexa-sql/assets/dbms/NOTICE.md` §1·§2-1)를 **설치본(3-OS)에도 제3자 고지로 넣어야 한다** — 지금은 저장소의 NOTICE까지. 후보 = 설치본 안 `THIRD-PARTY-NOTICES` 파일 + 앱 도움말 ▸ 정보.
> 10-04 후속([journal 10-04 §22](journal/2026-10-04.md)): ① 설치본 `THIRD-PARTY-NOTICES.txt`에 DBMS 로고 고지(`crates/nexa-sql/assets/dbms/NOTICE.md` 전문 · `scripts/third-party-notices.py`) 포함 = T-277 설치본 항목 해소 ② `release.yml`이 형제 저장소(nexa-ui · nexa-license)를 **태그 생성 시각 이전의 main 커밋으로 고정**(`fetch-depth: 0` + 고정 단계 · 실패해도 빌드는 진행 · 고정 커밋을 로그) — v0.1.4까지는 실행 시점 main 최신이었다 · 실제 검증 = 다음 태그.

### 5-5. Linux 패키지 저장소 채널 — pkg.sosomlab.com(APT + RPM · 사용자 10-05)

> 상태(10-05): 앱 쪽 워크플로 `.github/workflows/linux-repo.yml` · 릴리스 노트의 설치 표/등록 명령(`release.yml`) · 형제 저장소 `../linux-repo/apps/nexa-sql.toml`(미커밋)까지 작성. **미검증 = 실제 dispatch · linux-repo publish · `apt`/`dnf` 실기**(TODO T-280).

**배포 채널 요약**: brew(맥 · 열림) · **APT/RPM 저장소(Linux · 개시 준비)** · winget·choco(닫힘 · §5-3) · 그 밖 = GitHub Release 설치본 직접 내려받기.

**사용자 설치 명령**(linux-repo README와 같다 · 한 번 등록하면 `apt upgrade`/`dnf upgrade`로 갱신):

```sh
# APT(Debian · Ubuntu)
sudo curl -fsSLo /usr/share/keyrings/sosomlab-archive-keyring.gpg https://pkg.sosomlab.com/sosomlab-archive-keyring.gpg
sudo curl -fsSLo /etc/apt/sources.list.d/sosomlab.sources https://pkg.sosomlab.com/apt/sosomlab.sources
sudo apt update && sudo apt install nexa-sql
# RPM(Fedora · RHEL 계열)
sudo curl -fsSLo /etc/yum.repos.d/sosomlab.repo https://pkg.sosomlab.com/rpm/sosomlab.repo
sudo dnf install nexa-sql
```

**구조**: 저장소(`SosomLab/linux-repo`)는 패키지 파일을 싣지 않는다 — 서명된 색인의 파일 경로를 이 저장소(nexa-sql)의 GitHub Release 자산으로 302 보낸다 · 최신 정식 릴리스 하나만 싣는다 · 등록 파일 = linux-repo `apps/nexa-sql.toml`(package `nexa-sql` · `[deb] amd64 = nexa-sql_{version}_amd64.deb` · `[rpm] x86_64 = nexa-sql-{version}-1.x86_64.rpm` — v0.1.4 릴리스 자산 이름과 대조함).

**규칙 셋**:

1. **rpm은 배포에 포함한다** — `.deb`와 함께 저장소 채널의 필수 자산이다. 릴리스마다 둘 다 있는지 확인한다(`linux-repo.yml`이 `.deb` 없음 = 실패 · `.rpm` 없음 = 경고로 알린다).
2. **자산 이름 규칙을 바꾸면 두 곳을 함께** — `linux-repo.yml`의 자산 확인 줄과 linux-repo `apps/nexa-sql.toml`.
3. **한 번 공개한 Release 자산은 지우거나 다시 올리지 않는다** — 서명된 색인의 해시가 그 파일을 가리킨다. 다시 올려야 하면 버전을 올린다.

**공개 뒤 확인(릴리스마다 · `homebrew.yml` ✓ 옆에)**: `linux-repo.yml` ✓ → linux-repo `publish` ✓ → <https://pkg.sosomlab.com/repo.json>의 nexa-sql 버전이 새 태그와 같은지.

**지금 없는 것**: arm64 자산(deb `arm64` · rpm `aarch64`) · RPM 생성 경로는 linux-repo에서도 아직 실행된 적 없음(linux-repo `docs/SETUP.md` 8단계 진행 중).
