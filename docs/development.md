# 開発環境

ソースから起動するには Rust（rustup）と `just` が必要です。
リポジトリのルートで次のコマンドを実行します。
`just setup` は約 300 MB の CEF ランタイムを取得します。

```sh
cargo fetch --locked
just setup
just run
```

次回からは `just run` で起動できます。
macOS では開発用の `Koma.app` をビルドして起動し、起動を依頼したらコマンドは終了します。
実行中のアプリを CLI で操作するには `just koma --help` を参照してください。

## macOS の外部リンク受信を試す

`just bundle-macos` は `Koma.app` をビルドするだけで、起動はしません。
同梱した Browser Host を検証する際は、Koma を終了し、`env -u MAKEPAD_WM_ROOT just run` で起動します。
`MAKEPAD_WM_ROOT` が設定されていると、Makepad のチェックアウトにある Browser Host が使われる場合があります。
`open -a` でアプリを指定すると、既定ブラウザを変更せずに外部リンクを渡せます。

```sh
just bundle-macos
open -a "${CARGO_TARGET_DIR:-target}/debug/Koma.app" 'https://example.com/'
```
