set shell := ["zsh", "-cu"]

_koma_packages := "-p koma-app -p koma-cli -p koma-core -p koma-ipc"

_resolve_root := 'python3 -c "import glob,os,pathlib,tomllib; lock=tomllib.loads(pathlib.Path(\"Cargo.lock\").read_text()); rev=next(p[\"source\"].split(\"#\")[-1][:7] for p in lock[\"package\"] if p[\"name\"]==\"makepad-widgets\"); print(pathlib.Path(glob.glob(os.path.expanduser(f\"~/.cargo/git/checkouts/makepad-*/{rev}/Cargo.toml\"))[0]).parent)"'

_resolve_cef_dist := 'python3 -c "import glob,os,pathlib,platform,tomllib; lock=tomllib.loads(pathlib.Path(\"Cargo.lock\").read_text()); rev=next(p[\"source\"].split(\"#\")[-1][:7] for p in lock[\"package\"] if p[\"name\"]==\"makepad-widgets\"); root=pathlib.Path(glob.glob(os.path.expanduser(f\"~/.cargo/git/checkouts/makepad-*/{rev}/Cargo.toml\"))[0]).parent; s={\"Darwin\":\"macos\",\"Linux\":\"linux\",\"Windows\":\"windows\"}[platform.system()]; a={\"arm64\":\"arm64\",\"aarch64\":\"arm64\",\"x86_64\":\"x64\",\"AMD64\":\"x64\"}[platform.machine()]; p=\"linuxarm64\" if s==\"linux\" and a==\"arm64\" else (\"linux64\" if s==\"linux\" else f\"{s}{a}\"); pb=root/\"local/cef-prebuilt\"; ptr=pb/f\"current-{p}.txt\"; name=ptr.read_text().strip() if ptr.is_file() else f\"current-{p}\"; print(os.environ.get(\"MAKEPAD_CEF_DIST_DIR\") or (pb/name))"'

# レシピ一覧を表示する
default:
    @just --list

# CEF ランタイムを取得・展開する
cef-fetch:
    root=$({{_resolve_root}}) && cargo run --manifest-path "$root/Cargo.toml" -p makepad-cef-fetch -- --fetch

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
run:
    cef_dir=$({{_resolve_cef_dist}}) && cargo build -p makepad-app-browser-host && MAKEPAD_CEF_DIST_DIR="$cef_dir" cargo run -p koma-app

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
