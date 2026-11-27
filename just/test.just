# ワークスペースの単体テストを実行する
[no-cd]
default:
    cargo nextest run --workspace --lib --bins --offline

# ワークスペースの単体テストを実行する
[no-cd]
unit: default

# koma-core のテストのみを実行する
[no-cd]
core:
    cargo nextest run -p koma-core --offline

# Headless E2E テスト（e2e_stdin_loop）を実行する
[no-cd]
e2e:
    cargo nextest run -p koma-app --test e2e_stdin_loop --offline

# Web / CEF を含む E2E テストを実行する（要 just setup）
[no-cd]
e2e-web:
    cargo nextest run -p koma-app --test e2e_stdin_loop --run-ignored ignored-only --offline

# 統合テスト・Headless E2E を含む全テストを実行する
[no-cd]
all:
    cargo nextest run --workspace --offline

# 任意の引数で nextest を実行する
[no-cd]
[positional-arguments]
filter *args:
    cargo nextest run --workspace --offline "$@"
