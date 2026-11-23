# Koma

Makepad 2.0 製の `chromeless canvas` 型・ブラウザ統合ターミナル（macOS / Linux / Windows 対象）。Den Browser の設計思想をベースに、Rust + Makepad で構築している。

実装の依存関係とプロセス構成は [ARCHITECTURE.md](./ARCHITECTURE.md) 、主要な設計判断は [docs/adr/](./docs/adr/) を参照。
同梱コードの出典とライセンスは [vendor/README.md](./vendor/README.md) を参照。

## セットアップ・ビルド・起動

```bash
# 初回のみ: CEF ランタイム（約 300 MB）の取得と browser_host のビルド
just setup

# テスト・ビルド（koma-app と koma CLI の両方をビルド）
just test
cargo build --offline

# 起動（MAKEPAD_CEF_DIST_DIR を自動解決して cargo run を実行）
just run
```

Makepad の GUI をビルドせずに Core をテストしたり、CLI をビルドしたりする場合は、パッケージを指定する。

```bash
cargo nextest run -p koma-core --offline
cargo build -p koma-cli --offline
```

`just check` は Koma の4クレートの整形と lint に加え、依存境界と外部依存の取得元を検査する。
依存更新で CLI・IPC にクレートが増えたら、GUI に依存しないことを確かめてから [許可リスト](checks/deny-headless.toml) を更新する。
テストコードも lint の対象だが、実行は `just test` で行う。

```bash
mise install # just, cargo-deny, cargo-nextest, lefthook を導入
lefthook install # クローンごとに、コミット前に just check を実行するよう設定
```

設定・状態・IPC の既定パス:

- 設定: `~/.config/koma/config.toml`（`$KOMA_CONFIG` で上書き）
- Profile の保存ルート: `~/.local/state/koma/`（`$KOMA_STATE_DIR` で上書き）
- IPC: `/tmp/koma.sock`（非 Unix は一時ディレクトリの `koma.port`、`--socket` / `$KOMA_SOCKET` で上書き）

### Koma 設定（`config.toml`）

`koma settings` を実行すると、`~/.config/koma/config.toml`（または `$KOMA_CONFIG`）を `$VISUAL` / `$EDITOR`（未設定時は `vi`、Windows は `notepad`）で開く（`--path` でパス表示のみ）。ファイルが存在しない場合は既定値で起動し、変更は `koma settings reload` または Palette の `> Reload config`（もしくは再起動）で反映できる。

```toml
[layout]
width_presets = [0.333, 0.5, 0.667, 1.0] # 0 より大きく 1 以下の比率配列
width_step = 80                           # 幅増減のステップ（px、正の数）
gap = 10                                  # Koma 間と外周の隙間（px、0 以上の数）
center_focused = "never"                  # "never"（既定）/ "always" / "on-overflow"

[web]
search_engine = "duckduckgo"              # "duckduckgo"（既定）/ "google" / "brave"

[aliases]
repo = ":terminal ~/projects"
zmx = ":terminal ~ zmx attach {}"          # {} で :<alias> <arg> の引数を展開
gh = "https://github.com/{}"

[terminal]
session_prefixes = ["zmx attach "]        # Duplicate koma 時に末尾へ -2, -3 を付与
session_list_command = "zmx list --short" # 外部セッション名との重複を回避（1行1名）

[[sheet_presets]]
name = "Dev"
komas = [":terminal", "https://example.com"]

[keybindings.global]
"alt+left" = "focus_prev_koma"
"alt+right" = "focus_next_koma"
"alt+up" = "prev_sheet"
"alt+down" = "next_sheet"

[keybindings.koma]
"y" = "duplicate_koma"
```

### ターミナル設定（Ghostty）

Terminal Koma の外観は、アプリの TOML ではなく、起動時に読み込む Ghostty 設定で指定する。Ghostty 本体は起動不要で、設定ファイルも書き換えない。`KOMA_GHOSTTY_CONFIG` を指定すると、標準パスの代わりに指定ファイルを読む。

- `$XDG_CONFIG_HOME/ghostty/`（未指定時は `~/.config/ghostty/`）、macOS では続けて `~/Library/Application Support/com.mitchellh.ghostty/` を読む。各ディレクトリでは旧名の `config`、次に `config.ghostty` を読む。相対 `config-file` は記述元のファイルを基準に解決する。追加ファイルは基本ファイルの後に読み、入れ子の追加指定は待ち行列の末尾に加える。
- 対応項目: `font-family`、`font-family-bold`、`font-size`、`font-variation-bold` の `wght`、`adjust-cell-height`（ピクセル・百分率）、`foreground`、`background`、`cursor-color`、`palette`、`background-opacity`、`copy-on-select`（`true` / `clipboard` / `false`）、`font-feature`（`-calt` / `-liga` によるリガチャ無効化、既定は有効）。色は6桁のRGB値（`#` は任意）。
- `theme` は絶対パス、設定ディレクトリ内の `themes/`、またはインストール済み Ghostty のテーマを参照できる。明示した設定はテーマより優先される。`light:名前,dark:名前` の切り替えは Koma 起動時の外観に従う。
- フォント名は指定順に試し、空指定で候補をリセットする。Bold は `font-family-bold`、`font-family` の順に試す。見つからないフォントは飛ばし、Makepad 標準の等幅フォント（Liberation Mono）と標準の欠字フォールバックを使う。Liberation Mono の同梱書体は固定ウェイトのため、`wght` は可変フォントを指定したときだけ効く。
- Italic 用のフォントファミリ・端末キーバインドなど、未対応項目は無視する。Ghostty 設定がなければ MpTerm の既定値を使う。変更の反映には Koma の再起動が必要となる。

## 操作概要

- **グローバル**: `Cmd+Shift+O`（Overview）、`Cmd+T`（Palette）、`Cmd+Shift+T`（新規 Terminal）、`Cmd+L`（URL 編集）、`Cmd+W`（Koma 閉じる）、`Cmd+Shift+W`（Sheet 閉じる）、`Cmd+R` / `Cmd+[` / `Cmd+]`（Web 操作）、`Cmd+Opt+I`（Web DevTools）、`Cmd+Opt+←/→`（隣接 Koma）、`Cmd+Opt+↑/↓`（前後 Sheet）、`Cmd+Opt+1..9` / `Ctrl+Tab`（Sheet 切替）※非 macOS は `Cmd` を `Ctrl` に読み替え
- **Koma Mode**（`Ctrl+,` でトグル、`i` / `Esc` / `Enter` で Content Input 復帰）:
  - `h` / `l`（`←` / `→`）: フォーカス移動、`Shift+H` / `Shift+L`: Koma 並べ替え
  - `j` / `k`（`↓` / `↑`） / `1..9`: Sheet 切替、`Shift+J` / `Shift+K`: Sheet 間移動、`Shift+N`: Sheet 追加、`Shift+X`: Sheet 閉じる
  - `w`: 幅調整モード（`1..9` でプリセット指定後 Content Input 復帰、`w`/`r` で循環、`-`/`=` で微調整、`f`/`c` で最大化・中央寄せ）
  - `f`: 最大化トグル、`c`: 中央寄せ、`-` / `=`: 幅ステップ増減
  - `t`（Terminal）、`n` / `Space`（Palette）、`e`（URL 編集）、`r`（Koma リネーム）、`Shift+R`（Sheet リネーム）、`Shift+I`（DevTools）、`d`（複製）、`x`（Koma 閉じる）、`Tab`（Drawer）、`a`（Drawer 退避）、`u`（閉じた Koma を開き直す）、`m`（Notification）、`o`（Overview）
- **Drawer**: `u` で最後に廃棄した Drawer Item を戻す。
- **Palette**: 新規作成・既存 Koma・コマンド（`>` 接頭辞でコマンド絞り込み）、`Aliases` / `Recent` / `Sheet Preset` を統合。`Save sheet as preset` / `Duplicate koma` / `Close sheet` / `New private workspace`（`--private` 独立プロセス、非永続・一時 CEF プロファイル）を起動可能
- **Live Overview**: `h`/`j`/`k`/`l`・`1..9` で選択、`Shift+H/J/K/L` で移動・並べ替え、`-`/`=`/`0` でズーム、`Enter`/`o`/`Esc` でズームイン復帰（クリック・ドラッグ・パン・`Cmd+Scroll` 対応）
- **`koma` CLI**: コマンド一覧は `koma --help`、個別の使い方は `koma terminal send --help` のように確認できる。`KOMA_KOMA_ID` / `KOMA_PROFILE` / `KOMA_SOCKET` による Ambient Targeting に対応する。

Koma を開き直すと、元の Sheet と位置に戻り、その Koma にフォーカスが移る。
元の Sheet がなければ、現在の Sheet のフォーカスの右側に戻る。
Terminal は最後に保存した作業ディレクトリから新しいシェルを開き、元のコマンドは再実行しない。
Web は最後に保存した URL を開く。
閉じた Koma の履歴はアプリの再起動で消える。

### Profile の作成と切り替え

Palette で `> New Profile` を選んで名前を入力すると、空の Workspace が開く。
`> Switch Profile` で別の Profile を選ぶと、起動済みなら OS にウィンドウの前面表示を依頼し、未起動なら保存済みの Workspace を開く。
切り替え前の Workspace は閉じない。

CLI ではアプリの起動中に `koma profile list`、`koma profile new work`、`koma profile open work` を使える。
GUI を起動する実行ファイルに `--profile work` を渡すと、既存 Profile を直接開ける。
Profile 名には ASCII 英数字とハイフンを使い、小文字で保存する。

起動時に名前を指定しない場合は `Default` を開く。
すべての永続 Profile のレイアウトとブラウザデータは、状態ディレクトリ内の `profiles/<名前>/` に保存する。
`Private Workspace` は一覧に出ず、終了後に復元しない。

### Web Koma のスクリプト実行

`koma web script --koma koma-3 task.mjs` は、開いている Web Koma を Node.js から操作する。
`--koma` を省略すると、ほかの Web コマンドと同じ規則で対象を選ぶ。
Node.js 20 以上と、スクリプトのプロジェクトに `playwright-core` 1.60 以上が必要。
Playwright 用のブラウザは別途インストール不要。

```js
// task.mjs
export default async ({ page, snapshot }) => {
  await page.getByRole("button", { name: "詳細" }).click();
  await page.getByRole("heading", { name: "詳細情報" }).waitFor();
  return await snapshot();
};
```

戻り値は標準出力に JSON で、`console` のログは標準エラー出力に出る。
ファイル名の代わりに `-` を渡すと標準入力から自己完結したモジュールを読み、依存パッケージは作業ディレクトリから探す。
Node.js ランナーの実行上限は既定で30秒。 `--timeout <秒>` で変更できる。
スクリプトは CLI を実行したユーザーの権限で動く。

## `vendor/` パッチと既知の制約

- **`vendor/` パッチ**:
  - `makepad-cef`: 日本語 IME（`ime_set_composition` / `ime_cancel_composition` / `on_ime_composition_range_changed`）、`remote_debugging_port`（DevTools 用）、`MAKEPAD_WM_ROOT` 配下の CEF 探索対応
  - `makepad-app-browser-host`: `ImeComposition` / `ImeCancelComposition` / `ImeCaret` / `DevToolsReady` の接続
  - `makepad-browser-view`: 複数 Profile 起動時の `<tmp>` 競合回避（`<PID>-<instance_id>` 分離）、任意プロファイル指定、IME キャレット・DevTools ポートプロトコル追加
  - `makepad-app-terminal`: インライン IME preedit 表示と `Notification`（Bell / OSC 9・777）公開
  - `makepad-hal`: AppKit 日本語 IME 変換確定時の `ReturnKey` 漏れ修正
- **残る検証・制約（Phase 3）**:
  - **CEF 日本語 IME**: `ime_set_composition` とキャレット位置追従を接続済み（macOS 実機での対話的変換確認待ち）
  - **CEF JS 評価制約**: `evaluate_javascript` は式を `return ({expr}\n);` で包むため末尾 `;` は構文エラーになり、`Promise` も待機しない
  - **Linux / Windows**: 非 Unix TCP ポートファイル IPC と非 macOS `Ctrl` 修飾キー抽象化は実装済みだが、実機ビルド・基本操作は未検証
