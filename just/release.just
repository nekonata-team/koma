# レシピ一覧を表示する
default:
    @just --list release

# Apple Silicon 向け配布用 Koma.app をビルド・同梱・署名する
[macos]
[no-cd]
macos:
    cargo run --locked -p xtask -- macos

# バージョン更新・検査・ビルド・署名・公証・zip 化を行う
[macos]
[no-cd]
prepare version:
    cargo run --locked -p xtask -- prepare "{{version}}"

# prepare 済みのリリースをコミット・タグ付け・GitHub Releases へ公開する
[macos]
[no-cd]
[confirm("Publish prepared release to GitHub Releases?")]
publish version:
    cargo run --locked -p xtask -- publish "{{version}}"
