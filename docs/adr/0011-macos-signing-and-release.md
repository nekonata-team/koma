# macOS の署名・公証とローカルリリースフロー

Koma は App Store 外で macOS 向けバイナリ（`Koma.app` と同梱 `koma` CLI）を配布する。
Xcode プロジェクトを持たない Rust プロジェクトであり、App Store 用のプロビジョニングプロファイルも不要なため、Fastlane や `match` は導入せず、`just/release.just` から `xtask` に委譲し、`xtask` が `codesign`・`ditto`・`xcrun notarytool` を呼び出す。

成果物は `.dmg` ではなく、`Koma.app` を公証・`staple` したあとに `ditto` で固め直す `.zip` 形式に統一する。
`.zip` は Nix のパッケージ化、Homebrew Cask、手動ダウンロードのすべてでそのまま扱え、Developer ID 署名により更新後もカメラ・マイク等の権限（TCC）が維持される。

リリース作業はローカルの 2 段階（`just release prepare` → `just release publish`）で実行し、公証の認証には `xcrun notarytool` の `--keychain-profile` を使う。
これにより CI 側の証明書や秘密鍵管理を持たずに済み、`.zip` のハッシュ確定とタグ付け・GitHub Releases 公開を順序よく進められる。

Linux・Windows 向けの自動ビルドや CI による自動リリースが必要になった段階で、GitHub Actions への移行を再検討する。
