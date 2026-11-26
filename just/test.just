# ワークスペースのテストを実行する
[no-cd]
default:
    cargo nextest run --workspace --offline

# ライブラリの単体テストのみを実行する
[no-cd]
unit:
    cargo nextest run --workspace --lib --offline

# koma-core のテストのみを実行する
[no-cd]
core:
    cargo nextest run -p koma-core --offline

# E2E テスト（e2e_stdin_loop）を実行する
[no-cd]
e2e:
    cargo nextest run -p koma-app --test e2e_stdin_loop --offline

# 任意の引数で nextest を実行する
[no-cd]
[positional-arguments]
filter *args:
    cargo nextest run --workspace --offline "$@"
