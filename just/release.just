_target_dir := env("CARGO_TARGET_DIR", "target")
_dist_dir := _target_dir + "/release/dist"
_app_dir := _dist_dir + "/Koma.app"
_entitlements := "crates/koma-app/macos/entitlements.plist"
_codesign_identity := env("CODESIGN_IDENTITY", "Developer ID Application: nekonata, Inc. (F79C2UJ844)")
_notary_profile := env("NOTARY_PROFILE", "koma-notary")

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

# バージョン更新・検査・ビルド・署名・公証・zip 化を行う
[macos]
[no-cd]
[script("zsh", "-eu")]
prepare version:
    ver="{{version}}"
    [[ "$ver" =~ '^[0-9]+\.[0-9]+\.[0-9]+$' ]] || { echo "Invalid version: $ver (expected X.Y.Z)" >&2; exit 1; }
    [[ -z "$(git status --porcelain)" ]] || { echo "Working tree is not clean" >&2; exit 1; }
    [[ "{{_codesign_identity}}" != "-" ]] || { echo "Set Developer ID CODESIGN_IDENTITY before prepare" >&2; exit 1; }

    perl -0777 -i -pe "s/(\[workspace\.package\]\nversion = \")[^\"]+(\")/\${1}${ver}\${2}/" Cargo.toml
    /usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $ver" crates/koma-app/macos/Info.plist
    /usr/libexec/PlistBuddy -c "Set :CFBundleVersion $ver" crates/koma-app/macos/Info.plist
    cargo metadata --offline --format-version 1 >/dev/null

    just check
    just release macos

    zip="{{_dist_dir}}/Koma-${ver}-arm64.zip"
    rm -f "$zip"
    ditto -c -k --sequesterRsrc --keepParent "{{_app_dir}}" "$zip"
    xcrun notarytool submit "$zip" --keychain-profile "{{_notary_profile}}" --wait
    xcrun stapler staple "{{_app_dir}}"
    xcrun stapler validate "{{_app_dir}}"
    spctl --assess --type execute --verbose=2 "{{_app_dir}}"
    rm -f "$zip"
    ditto -c -k --sequesterRsrc --keepParent "{{_app_dir}}" "$zip"

    sri="sha256-$(openssl dgst -sha256 -binary "$zip" | openssl base64 -A)"
    perl -i -pe "s/^(\s*version = \")[^\"]+(\";)/\${1}${ver}\${2}/; s|^(\s*hash = \")[^\"]+(\";)|\${1}${sri}\${2}|" flake.nix
    echo "Prepared: $zip"
    echo "SRI hash: $sri"

# prepare 済みのリリースをコミット・タグ付け・GitHub Releases へ公開する
[macos]
[no-cd]
[confirm("Publish prepared release to GitHub Releases?")]
[script("zsh", "-eu")]
publish version:
    ver="{{version}}"
    tag="v${ver}"
    zip="{{_dist_dir}}/Koma-${ver}-arm64.zip"
    [[ "$(git branch --show-current)" == "main" ]] || { echo "Must be on main branch" >&2; exit 1; }
    [[ -f "$zip" ]] || { echo "Missing $zip (run 'just release prepare $ver' first)" >&2; exit 1; }
    ! git rev-parse -q --verify "refs/tags/${tag}" >/dev/null || { echo "Tag ${tag} already exists" >&2; exit 1; }

    git add Cargo.toml Cargo.lock crates/koma-app/macos/Info.plist flake.nix
    git commit -m "chore: ${tag} をリリースする"
    git tag "$tag"
    git push origin main --tags
    gh release create "$tag" "$zip" --title "Koma ${tag}" --generate-notes
