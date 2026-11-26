set shell := ["zsh", "-cu"]

_target_dir := env("CARGO_TARGET_DIR", "target")
_dist_dir := _target_dir + "/release/dist"
_app_dir := _dist_dir + "/Koma.app"
_dmg_path := _dist_dir + "/Koma-arm64.dmg"
_entitlements := "crates/koma-app/macos/entitlements.plist"
_codesign_identity := env("CODESIGN_IDENTITY", "-")

# レシピ一覧を表示する
default:
    @just --list release

# Apple Silicon 向け配布用 Koma.app をビルド・同梱・署名する
[macos]
[no-cd]
[script("zsh", "-eu")]
macos:
    if [[ -n "${MAKEPAD_CEF_DIST_DIR:-}" ]]; then
        cef_dist="$MAKEPAD_CEF_DIST_DIR/Release/Chromium Embedded Framework.framework"
    else
        common_root="$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")"
        cef_root=""
        for candidate in "local/cef-prebuilt" "$common_root/local/cef-prebuilt" "${CARGO_HOME:-$HOME/.cargo}"/git/checkouts/makepad-*/*/local/cef-prebuilt(N); do
            if [[ -f "$candidate/current-macosarm64.txt" ]]; then
                cef_root="$candidate"
                break
            fi
        done
        test -n "$cef_root" || { echo "Missing CEF prebuilt (run 'just cef-fetch' first)" >&2; exit 1; }
        cef_dist="$cef_root/$(tr -d '\r\n' < "$cef_root/current-macosarm64.txt")/Release/Chromium Embedded Framework.framework"
    fi
    test -d "$cef_dist" || { echo "Missing CEF framework: $cef_dist" >&2; exit 1; }

    cargo build --release --locked --target aarch64-apple-darwin -p koma-app -p koma-cli -p koma-browser-host
    arch_bin="{{_target_dir}}/aarch64-apple-darwin/release"

    rm -rf "{{_app_dir}}"
    contents="{{_app_dir}}/Contents"
    macos_dir="$contents/MacOS"
    frameworks="$contents/Frameworks"
    mkdir -p "$macos_dir" "$frameworks"

    install -m 644 crates/koma-app/macos/Info.plist "$contents/Info.plist"
    printf 'APPL????' > "$contents/PkgInfo"
    install -m 755 "$arch_bin/koma-app" "$macos_dir/koma-app"
    install -m 755 "$arch_bin/koma" "$macos_dir/koma"
    install -m 755 "$arch_bin/makepad-app-browser-host" "$macos_dir/makepad-app-browser-host"

    ditto "$cef_dist" "$frameworks/Chromium Embedded Framework.framework"

    typeset -A helpers=(
        [""]=".helper"
        [" (GPU)"]=".helper.gpu"
        [" (Plugin)"]=".helper.plugin"
        [" (Renderer)"]=".helper.renderer"
    )
    for suffix id_suffix in "${(@kv)helpers}"; do
        name="makepad-app-browser-host Helper${suffix}"
        h_contents="$frameworks/${name}.app/Contents"
        mkdir -p "$h_contents/MacOS" "$h_contents/Resources"
        install -m 755 "$arch_bin/makepad-app-browser-host" "$h_contents/MacOS/$name"
        printf 'APPL????' > "$h_contents/PkgInfo"
        printf '%s\n' \
            '<?xml version="1.0" encoding="UTF-8"?>' \
            '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "https://www.apple.com/DTDs/PropertyList-1.0.dtd">' \
            '<plist version="1.0">' \
            '<dict>' \
            '    <key>CFBundleExecutable</key>' \
            "    <string>${name}</string>" \
            '    <key>CFBundleIdentifier</key>' \
            "    <string>dev.nekonata.koma${id_suffix}</string>" \
            '    <key>CFBundleName</key>' \
            "    <string>${name}</string>" \
            '    <key>CFBundlePackageType</key>' \
            '    <string>APPL</string>' \
            '    <key>LSBackgroundOnly</key>' \
            '    <true/>' \
            '    <key>LSUIElement</key>' \
            '    <true/>' \
            '</dict>' \
            '</plist>' > "$h_contents/Info.plist"
    done

    sign_plain() {
        local target="$1"
        if [[ "{{_codesign_identity}}" == "-" ]]; then
            codesign --force --sign - "$target"
        else
            codesign --force --options runtime --timestamp --sign "{{_codesign_identity}}" "$target"
        fi
    }

    sign_entitled() {
        local target="$1"
        if [[ "{{_codesign_identity}}" == "-" ]]; then
            codesign --force --sign - "$target"
        else
            codesign --force --options runtime --timestamp --entitlements "{{_entitlements}}" --sign "{{_codesign_identity}}" "$target"
        fi
    }

    while IFS= read -r -d '' lib; do
        sign_plain "$lib"
    done < <(find "$frameworks/Chromium Embedded Framework.framework" -type f -name '*.dylib' -print0)
    sign_plain "$frameworks/Chromium Embedded Framework.framework"
    for suffix in "${(@k)helpers}"; do
        sign_entitled "$frameworks/makepad-app-browser-host Helper${suffix}.app"
    done
    sign_plain "$macos_dir/koma"
    sign_entitled "$macos_dir/makepad-app-browser-host"
    sign_entitled "{{_app_dir}}"
    codesign --verify --deep --strict "{{_app_dir}}"

# 配布用 DMG を作成する
[macos]
[no-cd]
[script("zsh", "-eu")]
dmg: macos
    stage="{{_dist_dir}}/dmg-stage"
    rm -rf "$stage" "{{_dmg_path}}"
    mkdir -p "$stage"
    ditto "{{_app_dir}}" "$stage/Koma.app"
    ln -s /Applications "$stage/Applications"
    hdiutil create -volname "Koma" -srcfolder "$stage" -ov -format UDZO "{{_dmg_path}}"
    rm -rf "$stage"
    if [[ "{{_codesign_identity}}" != "-" ]]; then
        codesign --force --timestamp --sign "{{_codesign_identity}}" "{{_dmg_path}}"
    fi

# 作成した DMG を Apple 公証へ提出してステープルする
[macos]
[no-cd]
notarize profile:
    xcrun notarytool submit "{{_dmg_path}}" --keychain-profile "{{profile}}" --wait
    xcrun stapler staple "{{_dmg_path}}"
