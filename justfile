set shell := ["zsh", "-cu"]
set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

_koma_packages := "-p koma-app -p koma-cli -p koma-core -p koma-ipc"

# レシピ一覧を表示する
default:
    @just --list

# CEF ランタイムを取得・展開する
[unix]
cef-fetch:
    rev=$(awk '/^name = "makepad-widgets"/{f=1;next} f && /^source = /{sub(/.*#/,""); print substr($0,1,7); exit}' Cargo.lock) && cargo run --manifest-path "${CARGO_HOME:-$HOME/.cargo}"/git/checkouts/makepad-*/"$rev"/Cargo.toml -p makepad-cef-fetch -- --fetch

[windows]
cef-fetch:
    $rev = (Select-String -Path Cargo.lock -Pattern 'name = "makepad-widgets"' -Context 0,2 | ForEach-Object { if ($_.Context.PostContext -join "`n" -match '#([0-9a-f]{7})') { $Matches[1] } } | Select-Object -First 1); $cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE ".cargo" }; $manifest = Get-Item (Join-Path $cargoHome "git\checkouts\makepad-*\$rev\Cargo.toml") | Select-Object -First 1 -ExpandProperty FullName; cargo run --manifest-path $manifest -p makepad-cef-fetch -- --fetch

# browser_host をビルドする
cef-host:
    cargo build -p makepad-app-browser-host

# CEF の取得と browser_host のビルドをまとめて行う
setup: cef-fetch cef-host

# ワークスペースをビルドする
build:
    cargo build --workspace

# ワークスペースのテストを実行する
[positional-arguments]
test *args:
    cargo nextest run --workspace --offline "$@"

# 起動する
[macos]
run: bundle-macos
    open "${CARGO_TARGET_DIR:-target}/debug/Koma.app"

# 起動する
[linux]
[windows]
run:
    cargo build -p makepad-app-browser-host
    cargo run -p koma-app

# macOS の開発用 Koma.app を作成する（CEF は just setup で取得）
[macos]
bundle-macos:
    cargo build --locked -p koma-app -p makepad-app-browser-host
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
    cargo deny --locked --all-features --manifest-path vendor/makepad-app-browser-host/Cargo.toml --config checks/deny-browser-host.toml check bans
    cargo deny --locked --all-features check sources

# Koma の整形、lint、依存境界を検査する
check:
    just fmt -- --check
    just clippy
    just deny
