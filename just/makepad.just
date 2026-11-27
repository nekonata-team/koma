makepad_dir := env("MAKEPAD_DIR", "")

_repo := '"https://github.com/tyPhoon-collab/makepad"'
_patch := "--config 'patch." + _repo + ".makepad-app-browser-host.path=\"" + makepad_dir + "/apps/browser_host\"' " + \
          "--config 'patch." + _repo + ".makepad-app-host.path=\"" + makepad_dir + "/libs/app_host\"' " + \
          "--config 'patch." + _repo + ".makepad-app-terminal.path=\"" + makepad_dir + "/apps/terminal\"' " + \
          "--config 'patch." + _repo + ".makepad-apple-sys.path=\"" + makepad_dir + "/libs/apple_sys\"' " + \
          "--config 'patch." + _repo + ".makepad-browser-view.path=\"" + makepad_dir + "/libs/browser_view\"' " + \
          "--config 'patch." + _repo + ".makepad-cef-fetch.path=\"" + makepad_dir + "/tools/cef_fetch\"' " + \
          "--config 'patch." + _repo + ".makepad-micro-serde.path=\"" + makepad_dir + "/libs/micro_serde\"' " + \
          "--config 'patch." + _repo + ".makepad-platform.path=\"" + makepad_dir + "/platform\"' " + \
          "--config 'patch." + _repo + ".makepad-studio-protocol.path=\"" + makepad_dir + "/platform/studio\"' " + \
          "--config 'patch." + _repo + ".makepad-widgets.path=\"" + makepad_dir + "/widgets\"'"

# レシピ一覧を表示する
default:
    @just --list makepad

[no-cd]
_require_dir:
    @test -n "{{makepad_dir}}" || { echo "MAKEPAD_DIR is required (e.g. MAKEPAD_DIR=/path/to/makepad)" >&2; exit 1; }
    @test -d "{{makepad_dir}}" || { echo "MAKEPAD_DIR does not exist: {{makepad_dir}}" >&2; exit 1; }

# ローカルの makepad を参照して任意の cargo コマンドを実行する（Cargo.lock は一時変更される）
[no-cd]
[positional-arguments]
cargo *args: _require_dir
    cargo "$@" {{_patch}}

# ローカルの makepad を参照してワークスペースをビルドする
[no-cd]
build: (cargo "build" "--workspace")

# ローカルの makepad を参照して clippy を実行する
[no-cd]
[positional-arguments]
check *args: _require_dir
    cargo clippy -p koma-app -p koma-cli -p koma-core -p koma-ipc -p koma-browser-host -p koma-cef-fetch -p xtask --all-targets --no-deps {{_patch}} "$@" -- -D warnings

# ローカルの makepad を参照してテストを実行する
[no-cd]
[positional-arguments]
test *args: _require_dir
    cargo nextest run --workspace --offline {{_patch}} "$@"
