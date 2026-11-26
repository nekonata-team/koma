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

## Makepad を更新する

Koma は [Makepad フォーク](https://github.com/tyPhoon-collab/makepad) のコミットをルートの `Cargo.toml` に固定します。
Browser Host と CEF 取得ツールも同じコミットを参照し、Koma の `Cargo.lock` で解決します。
`just setup` で取得した CEF は `local/cef-prebuilt/` に入ります。

1. 上流のリリースタグを取得し、フォークの `koma/main` にマージして push します。マージ後、フォーク側で IME 回帰テスト（`cargo test -p makepad-platform --lib hal_intake`、`cargo test -p makepad-hal --lib ime`）を実行します。
2. Koma の `[workspace.dependencies]` にある Makepad の `rev` を揃え、`cargo metadata --format-version 1` でロックファイルを更新します。
3. macOS・Windows・Linux で `just build`、`just check`、`just test` を実行します。
4. `just setup` と `just private` で、同梱 Host による Web 表示、Profile 分離、Terminal・Palette での日本語 IME 変換確定を確認します。

Makepad 側の独自変更は、同時に更新が必要なクレートを1コミットにまとめます。
上流を取り込んだマージコミットと、Koma の固定先を更新する変更を履歴に残します。

push 前に手元の Makepad チェックアウトで動作確認する場合は、`MAKEPAD_DIR=/path/to/makepad just makepad check` のように実行します。
実行時は `Cargo.lock` が一時変更されるため、確認後に `git checkout -- Cargo.lock` で戻します。

## 依存ライブラリの出典

- [Makepad](https://github.com/makepad/makepad) は [MIT ライセンス](https://github.com/tyPhoon-collab/makepad/blob/koma/main/LICENSE) で提供されています。
- Terminal の VT 実装は Ghostty 由来のコードを含みます。[Ghostty](https://github.com/tyPhoon-collab/makepad/blob/koma/main/apps/terminal/LICENSE-Ghostty) と [Unicode](https://github.com/tyPhoon-collab/makepad/blob/koma/main/apps/terminal/LICENSE-Unicode) のライセンスを参照してください。
- [Chromium Embedded Framework](https://github.com/chromiumembedded/cef) のインターフェース定義については [CEF のライセンス](https://github.com/tyPhoon-collab/makepad/blob/koma/main/libs/cef/LICENSE-CEF) を参照してください。
