# Koma の設定リファレンス

## 設定ファイルと反映方法

`just koma settings` で `~/.config/koma/config.toml` を開けます。
エディタは `VISUAL`、`EDITOR` の順に選ばれ、両方を省略すると `vi`（Windows では `notepad`）を使います。
`KOMA_CONFIG` を指定すると設定ファイルの場所を変更できます。
読み込む場所だけを確認するには `just koma settings --path` を実行します。

設定ファイルや項目を省略すると、以下の既定値が使われます。
編集後は Palette の `> Reload config` または `just koma settings reload` で、起動中の Workspace に設定を読み直します。
シェルと Ghostty 設定の変更は、読み直したあとに開く端末に反映されます。

## レイアウトと検索

| 項目 | 型 | 既定値 | 有効な値と動作 |
| --- | --- | --- | --- |
| `layout.width_presets` | 数値の配列 | 画面の利用可能幅の 1/3、1/2、2/3、全幅 | 空でない配列。各値は 0 より大きく 1 以下。幅プリセットの順番を指定する。 |
| `layout.width_step` | 数値 | `80` | 正の数。Koma の幅を手動で増減するときの刻み幅（論理ピクセル）。 |
| `layout.gap` | 数値 | `10` | 0 以上の有限な数。Koma 間と外周の隙間（論理ピクセル）。 |
| `layout.center_focused` | 文字列 | `"never"` | `"never"` は画面内に収まる位置まで移動。`"always"` は可能な範囲で中央寄せ。`"on-overflow"` はフォーカスした Koma が画面外にはみ出した場合に中央寄せ。 |
| `web.search_engine` | 文字列 | `"duckduckgo"` | `"duckduckgo"`、`"google"`、`"brave"`。Palette に検索語を入力したときに使う。 |

レイアウトの隙間は再読み込み時に更新されます。
中央寄せ、幅プリセット、検索エンジンの設定は、対応する次の操作から使われます。

## 表示倍率

`[scale]` に、Content の基準倍率を指定できます。

| 項目 | 型 | 既定値 | 有効な値と動作 |
| --- | --- | --- | --- |
| `scale.web` | 数値 | `1.0` | `0.5` 以上 `2.0` 以下の数値。新規に開く Web Koma の初期倍率と、ズームリセット（`Cmd+0` / `Ctrl+0`）の戻り先になる。 |
| `scale.terminal` | 数値 | `1.0` | `0.5` 以上 `2.0` 以下の数値。新規に開く Terminal Koma の初期倍率（Ghostty の `font-size` に対する倍率）と、ズームリセットの戻り先になる。 |

```toml
[scale]
web = 0.9
terminal = 1.0
```

再読み込み後は、新しく開く Koma とズームリセット操作に反映されます。既に開いている Koma の倍率は自動では変わりません。

## Palette のエイリアス

`[aliases]` には、名前と空でない展開先文字列の組を指定します。
既定ではエイリアスは空です。
名前の途中に空白は使えず、`terminal` と `term` は組み込みコマンド用の予約名です。

```toml
[aliases]
repo = ":terminal ~/projects"
gh = "https://github.com/{}"
```

Palette に `:repo` と入力すると `~/projects` で端末を開きます。
`:gh owner/repo` では `{}` が `owner/repo` に置き換わります。
`{}` を含むエイリアスには引数が必要です。
`{}` を省いたエイリアスは引数を取らず、展開先には別のエイリアスも指定できます。
設定は再読み込み後の入力から反映されます。

## Sheet プリセット

`[[sheet_presets]]` を繰り返すと複数のプリセットを登録できます。
既定では設定ファイル由来のプリセットは空です。

| 項目 | 型 | 必須条件 |
| --- | --- | --- |
| `name` | 文字列 | 空でないプリセット名。 |
| `komas` | 文字列の配列 | 空でない配列。各要素に URL や `:terminal` などの空でない入力を指定する。 |

```toml
[[sheet_presets]]
name = "Dev"
komas = [":terminal ~/projects", "https://example.com"]
```

再読み込み後、Palette から新しいプリセットを選べます。
同名のプリセットは、Palette から保存したものが設定ファイル内のものより優先されます。
同じ保存元に同名の項目が複数ある場合は、最初の項目を使います。
名前の比較では ASCII の大文字と小文字を区別しません。

## ターミナルの起動と複製

| 項目 | 型 | 既定値 | 有効な値と動作 |
| --- | --- | --- | --- |
| `terminal.shell` | 文字列 | macOS と Linux では `SHELL`（未設定時は `/bin/zsh`）、Windows では `cmd.exe` | シェルの実行ファイル名またはパス。空でない文字列。再読み込み後に開く端末で使う。 |
| `terminal.session_prefixes` | 文字列の配列 | `[]` | 各要素に空でないコマンドの接頭辞を指定。再読み込み後、接頭辞に一致する端末のコマンドを複製するとき、セッション名に `-2`、`-3` などを付ける。 |
| `terminal.session_list_command` | 文字列 | 未指定 | 外部のセッション名を標準出力へ1行に1件ずつ出すコマンド。再読み込み後、上の接頭辞に一致する端末の複製時に実行し、使用中の名前を避ける。 |

```toml
[terminal]
session_prefixes = ["zmx attach "]
session_list_command = "zmx list --short"
```

接頭辞の末尾の空白は照合に使われます。
この例では `zmx attach work` を複製すると、使用中の名前を避けて `zmx attach work-2` などを実行します。
一覧コマンドの実行に失敗すると、エラーを表示して複製を中止します。

## キーバインド

`[keybindings.global]`、`[keybindings.koma]`、`[keybindings.overview]` に、キーとアクションの組を指定します。
既定では上書きは空で、組み込みのキーバインドが使われます。
変更は再読み込み後のキー入力に反映されます。

```toml
[keybindings.global]
"mod+alt+left" = "focus_prev_koma"

[keybindings.koma]
"y" = "duplicate_koma"

[keybindings.overview]
"space" = "exit_overview"
```

`global` は通常の入力、Koma Mode、Overview で優先され、`koma` と `overview` はそれぞれのモードで使われます。
Palette、Drawer、通知一覧、幅調整モードが開いている間は、その画面専用の操作が優先されます。
`none` を指定すると、その範囲の割り当てを無効にできます。

修飾キーは `ctrl`、`alt`（`opt`）、`cmd`（`super`）、`shift` を使い、最後のキーと `+` でつなぎます。
`mod`（`primary`）は macOS では `cmd`、ほかの OS では `ctrl` です。
英大文字1文字は Shift 付きの英字キーとして扱われます。
英数字のほか、`left`、`right`、`up`、`down`、`space`、`tab`、`esc`、`enter`、`comma`、`minus`、`equal`、`plus`、`semicolon`、`bracket_left`、`bracket_right` を指定できます。

| 操作 | アクション |
| --- | --- |
| モード | `toggle_mode`、`enter_content_input`、`toggle_overview`、`exit_overview` |
| Koma の選択と並べ替え | `focus_prev_koma`、`focus_next_koma`、`move_prev_koma`、`move_next_koma` |
| Sheet | `prev_sheet`、`next_sheet`、`prev_sheet_wrap`、`next_sheet_wrap`、`move_prev_sheet`、`move_next_sheet`、`sheet_1` から `sheet_9`、`new_sheet`、`close_sheet`、`rename_sheet`、`save_sheet_preset` |
| 幅と表示位置 | `narrow_koma`、`widen_koma`、`cycle_width`、`enter_width_mode`、`toggle_maximize`、`center_koma` |
| Overview のズーム | `overview_zoom_in`、`overview_zoom_out`、`overview_zoom_reset` |
| Koma の作成と編集 | `open_palette`、`new_terminal`、`rename_koma`、`duplicate_koma`、`close_koma`、`close_koma_focus_left`、`reopen_koma` |
| Web | `edit_url`、`reload_web`、`web_back`、`web_forward`、`open_devtools` |
| Drawer と通知 | `toggle_drawer`、`stow_drawer`、`restore_drawer`、`toggle_notifications` |
| Workspace と設定 | `new_private_workspace`、`reload_config` |
| 割り当ての無効化 | `none` |

## ターミナルの外観と Ghostty 設定

Koma は Ghostty の設定ファイルを読み込みます。
Ghostty 本体を起動する必要はありません。

- 最初に `$XDG_CONFIG_HOME/ghostty/`（未指定時は `~/.config/ghostty/`）を参照します。
  macOS では続けて `~/Library/Application Support/com.mitchellh.ghostty/` を参照します。
  各ディレクトリでは `config`、`config.ghostty` の順に読みます。
- `KOMA_GHOSTTY_CONFIG` を指定すると、設定の読み込みをそのファイルから始めます。
  `config-file` で追加したファイルは基本ファイルのあとに読み、相対パスは記述元のディレクトリを基準に解決します。
- テーマを先に適用し、続けて設定ファイルの指定を適用します。
  同じ色やサイズに複数の指定がある場合は後の値を使います。

| 項目 | 有効な値と動作 | 既定 |
| --- | --- | --- |
| `font-family`、`font-family-bold` | フォント名。繰り返し指定した場合は記述順に試す。空欄で候補をリセット。太字には通常フォントの候補も使う。 | Makepad の既定フォント |
| `font-size` | 1 から 256 までの数値。 | 端末の既定サイズ |
| `font-variation-bold` | `wght=700` のように指定。1 から 1000 までの値を可変フォントに適用する。 | 未指定（システムフォントの太字選択では `700`） |
| `adjust-cell-height` | ピクセルは −1000 から 1000 の整数、百分率は `25%` のように −99 から 1000 の整数に `%` を付ける。 | 補正なし |
| `foreground`、`background`、`cursor-color` | RGB 6桁の16進数。先頭の `#` は省略可。 | 端末の既定色 |
| `palette` | `1=#ff0000` のように色番号 0 から 255 と色を指定。繰り返し指定でき、空欄で上書きをリセット。 | 端末の既定パレット |
| `background-opacity` | 0 から 1 までの数値。 | 端末の既定値 |
| `copy-on-select` | `true` または `clipboard` で選択範囲をコピー。`false` で無効。 | `false` |
| `font-feature` | `-calt` または `-liga` でリガチャを無効にする。空欄、`+calt`、`+liga` で有効にする。複数指定では最後の対応項目を使う。 | 有効 |
| `theme` | テーマファイルの絶対パス、または設定ディレクトリや Ghostty の `themes/` 内のファイル名。`light:名前,dark:名前` で読み込み時の外観に応じて選択。 | 未指定 |

対応していない項目は読み飛ばします。
設定の変更は Palette の `> Reload config` で読み直し、そのあとに開く端末へ反映できます。
