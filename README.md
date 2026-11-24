# Koma

Koma は、ターミナルと Web ページを同じキャンバスに並べて使うアプリです。
ターミナルや Web ページを表示する各パネルを **Koma** と呼び、それらを **Sheet** にまとめて切り替えられます。

## 起動

ソースから起動するには Rust（rustup）と `just` が必要です。
このリポジトリのルートで、初回は次のコマンドを実行してください。
`just setup` はブラウザ用の CEF ランタイムを取得します（約 300 MB）。

```sh
cargo fetch --locked
just setup
just run
```

次回からは `just run` で起動できます。

## 基本操作

| 操作 | キー |
| --- | --- |
| Palette を開く | `Cmd+T` |
| ターミナルを開く | `Cmd+Shift+T` |
| Web Koma の URL を編集 | `Cmd+L` |
| 隣の Koma に移動 | `Cmd+Opt+←` / `Cmd+Opt+→` |
| 前後の Sheet に移動 | `Cmd+Opt+↑` / `Cmd+Opt+↓` |
| Overview で全体を見渡す | `Cmd+Shift+O` |
| Koma を閉じる | `Cmd+W` |
| Koma Mode に入る／抜ける | `Ctrl+,` |

macOS 以外では `Cmd` を `Ctrl`、`Opt` を `Alt` に読み替えてください。

Palette から URL を開いたり、既存の Koma を探したりできます。
`>` を入力すると、複製や Sheet プリセットの保存などのコマンドに絞り込めます。

Koma Mode では `h` / `l` で隣の Koma にフォーカスを移し、`j` / `k` で Sheet を切り替えます。
`Esc` でターミナルや Web ページへの入力に戻ります。
`a` で Koma を Drawer に退避し、`Tab` で Drawer を開けます。

Koma の上でトラックパッドをピンチすると、Terminal は文字サイズが変わり、Web はページのレイアウトを保ったままポインタ付近を拡大・縮小します。
Koma の枠は変わりません。

Palette の `> Reopen closed Koma` または Koma Mode の `u` で、閉じた Koma を開き直せます。
端末は最後に保存した作業ディレクトリで新しいシェルを開きます。
Web Koma は最後に保存した URL を開きます。
閉じた Koma の履歴はアプリ終了時に消えます。

### Profile の作成と切り替え

Palette の `> New Profile` で空の Workspace を作り、`> Switch Profile` で別の Workspace を開けます。
切り替え前の Workspace は開いたままです。
既定の Profile は `Default` で、Profile ごとのレイアウトとブラウザデータは `~/.local/state/koma/profiles/` に保存されます（保存ルートは `KOMA_STATE_DIR` で変更できます）。
`> New private workspace` で開いた Workspace は、終了後に復元されません。

## 設定

`just koma settings` で `~/.config/koma/config.toml` をエディタで開けます。
設定ファイルの場所は `KOMA_CONFIG` で変更できます。
保存後、Palette の `> Reload config` または `just koma settings reload` で読み直せます。

```toml
[layout]
gap = 8

[aliases]
repo = ":terminal ~/projects"
zmx = ":terminal ~ zmx attach {}"

[terminal]
session_prefixes = ["zmx attach "]        # 複製時にセッション名へ -2、-3 などを付ける
session_list_command = "zmx list --short" # 使用中の外部セッション名を避ける（1行に1件）

[keybindings.global]
"alt+left" = "focus_prev_koma"
"alt+right" = "focus_next_koma"
"alt+up" = "prev_sheet"
"alt+down" = "next_sheet"

[keybindings.koma]
"y" = "duplicate_koma"
```

zmx が入っていれば、Palette に `:zmx work` と入力してセッションを開けます。
Koma Mode の `y` でその端末を複製すると、使用中の名前を避けて `work-2` などの名前を付けます。

設定項目の既定値、有効な値、Ghostty 設定の読み込み方は [設定リファレンス](./docs/configuration.md) を参照してください。

## CLI と自動化

アプリの起動中は、`just koma profile list` で Profile を一覧表示し、`just koma profile open work` で既存の `work` を開けます。
CLI を PATH に置いている場合は `koma` で直接実行できます。
コマンドの一覧は `just koma --help`、個別の使い方は `just koma web script --help` のように確認できます。

### Web Koma のスクリプト実行

`just koma web script --koma koma-3 task.mjs` は、開いている Web Koma で Node.js のモジュールを実行します。
Node.js 20 以上と、スクリプトのプロジェクトに `playwright-core` 1.60 以上が必要です。
Playwright 用ブラウザの追加インストールは不要です。

```js
// task.mjs
export default async ({ snapshot }) => {
  return await snapshot();
};
```

戻り値は標準出力に JSON で出力し、`console` のログは標準エラー出力に送ります。
スクリプトは CLI を実行したユーザーの権限で動きます。

`koma web eval` には末尾のセミコロンを省いた JavaScript 式を渡してください。
`Promise` の完了を待つ処理には `koma web script` を使えます。

同梱コードの出典とライセンスは [vendor/README.md](./vendor/README.md) を参照してください。
