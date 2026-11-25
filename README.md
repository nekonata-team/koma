# Koma

Koma は、ターミナルと Web ページを同じキャンバスに並べて使うアプリです。
ターミナルや Web ページを表示する各パネルを **Koma** と呼び、それらを **Sheet** にまとめて切り替えられます。

## 基本操作

| 操作 | キー |
| --- | --- |
| Palette を開く | `Cmd+T` |
| ターミナルを開く | `Cmd+Shift+T` |
| Web Koma の URL を編集 | `Cmd+L` |
| 隣の Koma に移動 | `Cmd+Opt+←` / `Cmd+Opt+→` |
| 前後の Sheet に移動 | `Cmd+Opt+↑` / `Cmd+Opt+↓` |
| Koma を左右に並べ替え | `Cmd+Opt+Shift+←` / `Cmd+Opt+Shift+→` |
| Koma を前後の Sheet へ移動 | `Cmd+Opt+Shift+↑` / `Cmd+Opt+Shift+↓` |
| Overview で全体を見渡す | `Cmd+Shift+O` |
| 表示を拡大・縮小 | `Cmd+=` / `Cmd+-` |
| 表示倍率をリセット | `Cmd+0` |
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
キー操作ではフォーカス中の Terminal の文字サイズ、または Web のページ全体の表示倍率が変わります。
Web はキーで倍率を変えると再レイアウトし、`Cmd+0` でページズームとピンチの両方が元に戻ります。

Palette の `> Reopen closed Koma` または Koma Mode の `u` で、閉じた Koma を開き直せます。
端末は最後に保存した作業ディレクトリで新しいシェルを開きます。
Web Koma は最後に保存した URL を開きます。
閉じた Koma の履歴はアプリ終了時に消えます。

### Profile の作成と切り替え

Palette の `> New Profile` で空の Workspace を作り、`> Switch Profile` で別の Workspace を開けます。
切り替え前の Workspace は開いたままです。
既定の Profile は `Default` で、Profile ごとのレイアウトとブラウザデータは `~/.local/state/koma/profiles/` に保存されます（保存ルートは `KOMA_STATE_DIR` で変更できます）。
`> New private workspace` で開いた Workspace は、終了後に復元されません。

### macOS でデフォルトのブラウザにする

Palette の `> Show default browser` で、現在の既定ブラウザを確認できます。
Koma に変えるには `> Set as default browser` を選び、macOS の確認画面で「Koma に変更」を選びます。
別のブラウザに戻すには、macOS の「システム設定」→「デスクトップと Dock」→「デフォルトのウェブブラウザ」で選び直します。

他のアプリから開いた Web リンクは、新しい Web Koma に表示されます。
Koma が起動していなければ Default Profile の復元先 Sheet、起動済みならリンクを受け取ったウィンドウの選択中の Sheet に追加します。

### Web ページからの通知

開いている Web Koma からの通知は、その Profile の通知一覧に入ります。
別の Sheet にある Web Koma の通知も一覧に入り、Koma のウィンドウを操作中はトーストにも表示されます。
通知一覧は Koma Mode の `m` で開き、通知を選んで `Enter` を押すと送信元の Web Koma に戻ります。

サイトごとの許可操作は不要です。
対象は、開いているページ自身が送る通知です。バックグラウンドの Service Worker や Web Push は対象外で、OS の通知センターにも表示されません。

## 設定

設定を変えるには、`~/.config/koma/config.toml` を編集します。
設定ファイルの場所は `KOMA_CONFIG` で変更できます。
保存後、Palette の `> Reload config` で読み直せます。

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
"alt+shift+left" = "move_prev_koma"
"alt+shift+right" = "move_next_koma"
"alt+shift+up" = "move_prev_sheet"
"alt+shift+down" = "move_next_sheet"

[keybindings.koma]
"y" = "duplicate_koma"
```

zmx が入っていれば、Palette に `:zmx work` と入力してセッションを開けます。
Koma Mode の `y` でその端末を複製すると、使用中の名前を避けて `work-2` などの名前を付けます。

設定項目の既定値、有効な値、Ghostty 設定の読み込み方は [設定リファレンス](./docs/configuration.md) を参照してください。

## CLI と自動化

CLI を PATH に置いている場合は、アプリの起動中に `koma` コマンドを使えます。
`koma profile list` で Profile を一覧表示し、`koma profile open work` で既存の `work` を開けます。
`koma workspace list` は、接続できる Workspace とそのソケットパスを一覧表示します。
Private Workspace も個別に表示されます。選んだパスを `koma --socket "$socket" sheet list` のように渡すと、その Workspace を操作できます。
スクリプトでは `koma workspace list --json` の `workspaces` 配列から `id`、`is_private`、`socket` を取得できます。
コマンドの一覧は `koma --help`、個別の使い方は `koma web script --help` のように確認できます。

### Web Koma のスクリプト実行

`koma web script --koma koma-3 task.mjs` は、開いている Web Koma で Node.js のモジュールを実行します。
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
