# Homebrew Cask — Nexa SQL(설치본 채널: .dmg 안의 Universal 2 .app · docs/33 §2)
#
# 이식 원천: nexa-clip `packaging/homebrew/nexa-clip.rb`. 버전·체크섬 자리는 워크플로(`homebrew.yml`)가
# **공개된 릴리스의 실제 dmg 해시로** 채운다 — 손으로 적는 해시는 언젠가 틀리고, 틀린 해시는 사용자 기기에서 설치 실패로 나타난다.
#
# ★ 서명·공증이 없는 빌드(DR-20)는 quarantine이 붙으면 macOS가 실행을 막는다(nexa-clip 실측 08-11 — SIGKILL).
#   Homebrew Cask는 기본으로 quarantine을 붙이므로 postflight에서 떼고, 무엇을 왜 했는지 caveats에 밝힌다.
# ★ CLI `nsql`은 번들 안(`Contents/MacOS/nsql`) — `binary`가 brew의 bin에 링크한다(pkg 설치본의 /usr/local/bin 링크와 같은 역할).
cask "nexa-sql" do
  version "@VERSION@"
  sha256 "@SHA_MAC_UNIVERSAL_DMG@"

  # `verified:`는 넣지 않는다(홈페이지와 같은 도메인 · 형제 저장소 brew style 경고 기록 — docs/33 §5).
  url "https://github.com/SosomLab/nexa-sql/releases/download/v#{version}/nexa-sql-#{version}-macos-universal.dmg"
  name "Nexa SQL"
  desc "Lightweight cross-platform SQL client (IDE) with the nsql CLI"
  homepage "https://github.com/SosomLab/nexa-sql"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: ">= :big_sur"

  app "Nexa SQL.app"
  binary "#{appdir}/Nexa SQL.app/Contents/MacOS/nsql"

  postflight do
    system_command "/usr/bin/xattr",
                   args: ["-dr", "com.apple.quarantine", "#{appdir}/Nexa SQL.app"],
                   sudo: false
  end

  caveats <<~EOS
    This build is not code-signed or notarized yet (DR-20).
    The installer removed the macOS quarantine flag (com.apple.quarantine) so the app can start.
    Verify the download against sha256sums.txt on the release page:
      https://github.com/SosomLab/nexa-sql/releases

    이 앱은 아직 코드 서명·공증이 되어 있지 않습니다(DR-20).
    설치 과정에서 macOS 격리 표식을 제거해 바로 실행되도록 했습니다.
  EOS

  # 사용자 데이터(설정·연결 프로필·로그)는 번들 밖 — brew upgrade가 번들째 바꿔도 남는다. `brew uninstall --zap`에서만 지운다.
  zap trash: [
    "~/Library/Application Support/nexa-sql",
    "~/Library/Preferences/com.sosomlab.nexa-sql.plist",
    "~/Library/Saved Application State/com.sosomlab.nexa-sql.savedState",
  ]
end
