set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

mod makepad 'just/makepad.just'
mod release 'just/release.just'
mod test 'just/test.just'

_koma_packages := "-p koma-app -p koma-cli -p koma-core -p koma-ipc -p koma-browser-host -p koma-cef-fetch -p xtask"

# レシピ一覧を表示する
default:
    @just --list --list-submodules

# CEF ランタイムを取得・展開する
cef-fetch:
    cargo run --locked -p koma-cef-fetch -- --fetch

# browser_host をビルドする
cef-host:
    cargo build --locked -p koma-browser-host

# CEF の取得と browser_host のビルドをまとめて行う
setup: cef-fetch cef-host

# ワークスペースをビルドする
build:
    cargo build --workspace

# 起動する
[macos]
run: bundle-macos
    open "${CARGO_TARGET_DIR:-target}/debug/Koma.app"

# 起動する
[linux]
[windows]
run:
    cargo build --locked -p koma-browser-host
    cargo run -p koma-app

# Private Workspace で起動する
[macos]
private: bundle-macos
    env -u KOMA_SOCKET open -n "${CARGO_TARGET_DIR:-target}/debug/Koma.app" --args --private

# Private Workspace で起動する
[linux]
private:
    cargo build --locked -p koma-browser-host
    env -u KOMA_SOCKET cargo run -p koma-app -- --private

# Private Workspace で起動する
[windows]
private:
    cargo build --locked -p koma-browser-host
    Remove-Item Env:KOMA_SOCKET -ErrorAction SilentlyContinue; cargo run -p koma-app -- --private

# macOS の開発用 Koma.app を作成する（CEF は just setup で取得）
[macos]
bundle-macos:
    cargo build --locked -p koma-app -p koma-browser-host
    bin="${CARGO_TARGET_DIR:-target}/debug"; contents="$bin/Koma.app/Contents"; \
        mkdir -p "$contents/MacOS" && \
        install -CS -m 644 crates/koma-app/macos/Info.plist "$contents/" && \
        install -CS -m 755 "$bin/koma-app" "$bin/makepad-app-browser-host" "$contents/MacOS/"

# koma CLI を実行する
[positional-arguments]
koma *args:
    @cargo run --quiet -p koma-cli -- "$@"

# Koma のコードを整形する
[positional-arguments]
fmt *args:
    cargo fmt {{_koma_packages}} "$@"

# Koma のコード整形を検査する
fmt-check:
    cargo fmt {{_koma_packages}} -- --check

# Koma の clippy を実行する
[positional-arguments]
clippy *args:
    cargo clippy {{_koma_packages}} --locked --all-targets --no-deps "$@" -- -D warnings

# 依存境界とライセンス・ソースを検査する
deny:
    cargo deny --locked --all-features --manifest-path crates/koma-core/Cargo.toml --config checks/deny-core.toml check bans
    cargo deny --locked --all-features --manifest-path crates/koma-cli/Cargo.toml --config checks/deny-headless.toml check bans
    cargo deny --locked --all-features --manifest-path crates/koma-ipc/Cargo.toml --config checks/deny-headless.toml check bans
    cargo deny --locked --all-features --manifest-path crates/koma-app/Cargo.toml --config checks/deny-app.toml check bans
    cargo deny --locked --all-features --manifest-path crates/koma-browser-host/Cargo.toml --config checks/deny-browser-host.toml check bans
    cargo deny --locked --all-features check sources

# Koma の整形、lint、依存境界を検査する
check: fmt-check clippy deny

# 検査とテストを一括実行する
ci: check test::all
