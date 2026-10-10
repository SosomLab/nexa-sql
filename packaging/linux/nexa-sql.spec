# nexa-sql.spec — RPM 명세(rpmbuild · 이미 빌드된 바이너리를 스테이징에서 담는다 · docs/33 §2 FHS 레이아웃).
#   build-rpm.sh 가 넘기는 매크로: _version · _stagedir(build-deb.sh와 같은 FHS 스테이징 = target/packaging/linux/deb-root/usr)
#   소스 tarball 없이 %install 에서 스테이징을 복사한다 — 빌드는 cargo가 이미 했고(deb와 같은 바이너리), spec은 포장만.
#   debuginfo 추출은 끄고(strip=symbols 이미 적용) 바이너리 재strip도 막는다(Rust 실행 파일을 rpm이 다시 만지지 않게).
%global debug_package %{nil}
%global __strip /bin/true
%global __os_install_post %{nil}

Name:           nexa-sql
Version:        %{_version}
Release:        1%{?dist}
Summary:        Cross-platform lightweight SQL client + CLI (nsql)
License:        PolyForm-Noncommercial-1.0.0
URL:            https://github.com/SosomLab/nexa-sql
# 아키텍처는 build-rpm.sh의 `rpmbuild --target`(x86_64 · aarch64)이 정한다.
# 런타임 dlopen(Wayland/X11) — 빌드 시 링크 없음. 자동 의존성 탐지는 glibc만 잡는다.
Recommends:     libxkbcommon
Recommends:     google-noto-sans-cjk-fonts

%description
Nexa SQL is a lightweight SQL client (IDE) and a command-line tool (nsql) that look
and work the same on Windows, macOS and Linux. It is a statically linked executable
written entirely in Rust and drawn with its own rasterizer - no Qt, WebView or
Electron. Drivers for SQLite, Oracle, SQL Server and PostgreSQL are built in, and
SQL*Plus-style scripts (session variables, binds, EXEC) run on the same core in both
the GUI and the CLI.
User data lives in ~/.config/nexa-sql.

%prep
# 소스 없음 — 스테이징 복사만.

%build
# cargo가 이미 빌드했다(build-rpm.sh).

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}%{_prefix}
cp -a %{_stagedir}/. %{buildroot}%{_prefix}/

%post
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t -f %{_datadir}/icons/hicolor 2>/dev/null || :
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q %{_datadir}/applications 2>/dev/null || :

%postun
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t -f %{_datadir}/icons/hicolor 2>/dev/null || :
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q %{_datadir}/applications 2>/dev/null || :

%files
%{_bindir}/nexa-sql
%{_bindir}/nsql
%{_datadir}/applications/nexa-sql.desktop
%{_datadir}/icons/hicolor/*/apps/nexa-sql.png
%dir %{_datadir}/nexa-sql
%{_datadir}/nexa-sql/Packages
%license %{_docdir}/nexa-sql/LICENSE.md
%doc %{_docdir}/nexa-sql/LICENSE.ko.md
%doc %{_docdir}/nexa-sql/README.md
%doc %{_docdir}/nexa-sql/THIRD-PARTY-NOTICES.txt
%doc %{_docdir}/nexa-sql/copyright

%changelog
* Wed Sep 17 2026 Sangyong Bae <kiros33@gmail.com> - 0.0.1-1
- 첫 RPM 명세(T-72) — deb와 같은 FHS 스테이징을 포장.
