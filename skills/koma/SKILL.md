---
name: koma
description: >
  koma CLI で Koma の Workspace、Sheet、Web Koma、Terminal Koma、Drawer、Notification を操作するときに使う。
---

# Koma CLI (`koma`)

起動中の Koma を `koma` コマンドで操作する。疎通確認は `koma health`、全サブコマンドと引数は `koma --help` / `koma <subcommand> --help` で確認する。

## 出力形式とターゲティング

- **JSON 出力**: `--json` 指定時、および標準出力が非 TTY のときは自動で JSON（`{"Ok":{...}}` / `{"Err":"..."}`）を返す。スクリプトでは `--json` を明示して `Ok` を読む。
- **Ambient Targeting**: Terminal Koma 内では `KOMA_KOMA_ID`・`KOMA_PROFILE`・`KOMA_SOCKET` が自動設定され、対象指定を省略できる。
- **明示指定**: 別 Koma を操作する場合や `exists` / `focus` / `move` / `resize` / `close` には `--koma <ID>` を指定する。

## Web Koma

- **観測の使い分け**: 本文の Markdown は `koma web read`（`--within <selector|ref>` で範囲限定）、プレーンテキストや API 応答は `koma web text`、操作対象と `@eN` 参照の取得は `koma web snapshot` を使う。
- **JS 評価**: `koma web eval <expr>` には末尾セミコロンなしの式を渡す。
- **待機・複数ステップ操作**: 条件待機や Playwright 操作には `koma web script <file|->` を使う（Node.js 20+ と実行元プロジェクトの `playwright-core` 1.60+ が必要）。
  - `export default async ({ page, snapshot }) => ...` の戻り値が標準出力に JSON 出力され、`console` は標準エラー出力に出る。
- **KomaAPI**: `web new` / `web reload` / `drawer keep` に `--koma-api` を付けると `window.koma.komas.focus(id)` と `window.koma.toast.show(msg)` が使える。

## Terminal Koma

- **起動と委譲**: `koma terminal new [cwd] --run '<cmd>'` で別 Koma にコマンドやエージェントを起動する。大きな指示や結果の受け渡しは画面スクレイプではなくファイルパス経由にする。
- **入出力と制御**: 履歴確認は `koma terminal text --lines <N>`、キー送信は `koma terminal press <key>`（例: `Enter`, `Ctrl+C`）、停止は `koma terminal kill` を使う。
- **ハイフン対策**: `koma terminal send` で `-` 始まりの文字列を送るときは `--` を挟む（例: `koma terminal send -- -n`）。

## 制約

- Web Koma や Terminal Koma は作業対象としてのみ扱い、サンドボックスやアクセス制御の迂回に使わない。
