# GUI 設定画面を持たず、設定ファイルと `koma settings` に寄せる

Koma はアプリ内に GUI 設定画面を持たず、Koma 固有の設定は `~/.config/koma/config.toml`（`$KOMA_CONFIG`）、端末設定は既存の Ghostty 設定ファイルを起動時に読み込む。設定ファイルの編集導線として、`$VISUAL` / `$EDITOR` で設定を開く `koma settings` コマンドを提供する。

## Considered Options

- **GUI 設定画面をアプリ内に構築する**: フォーム UI・状態同期・バリデーション画面の実装と保守が増え、`chromeless canvas` の方針からも外れる。
- **ファイルベース設定 + `koma settings`（採用）**: 設定の正本をテキストファイルに一本化して差分管理・再現性を保ち、GUI 実装を持たずに編集導線だけを CLI で提供する。
