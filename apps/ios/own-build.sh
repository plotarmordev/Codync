#!/usr/bin/env bash
# Builds this checkout as an iPhone app of your own, next to the App Store Codync: README.md.
#   own-build.sh check   unsigned build, checks the app's identity and where it keeps its data (no Apple account)
#   own-build.sh upload  archives this checkout, signed, and uploads it to TestFlight through your signer
#   own-build.sh follow  uploads the followed branch's newest commit once (run it on a schedule)
# Settings come from the environment or $OWN_BUILD_SETTINGS (default ~/.config/codync/own-build.env), outside the
# repository; own-build.env.example lists them. The App Store Connect key stays with the signer: this script never
# sees it.
set -euo pipefail

settings_file=${OWN_BUILD_SETTINGS:-$HOME/.config/codync/own-build.env}
if [[ -f $settings_file ]]; then
    set -a
    source "$settings_file"
    set +a
fi

mode=${1:?usage: own-build.sh check|upload|follow}
root=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
bundle=${OWN_IOS_BUNDLE_ID:?set OWN_IOS_BUNDLE_ID, e.g. com.example.codync}
name=${OWN_IOS_APP_NAME:-Codync Own}
# Every new commit on the followed branch, upstream catch-up merges included, raises the count.
build=$(git -C "$root" rev-list --count HEAD)
out=$root/build/dd/own

# The App Store app's group belongs to its team; this app gets its own. Keychain groups are already
# separate: they start with the team's prefix.
identity=(
    CODYNC_IOS_BUNDLE_ID="$bundle"
    CODYNC_APP_GROUP="group.$bundle"
    CODYNC_APP_NAME="$name"
    CURRENT_PROJECT_VERSION="$build"
)
project=(-project "$root/apps/Codync.xcodeproj" -derivedDataPath "$out/derived" -quiet)

expect() { # plist key value
    local got
    got=$(/usr/libexec/PlistBuddy -c "Print :$2" "$1")
    [[ $got == "$3" ]] || { echo "$1 $2: expected $3, got $got" >&2; exit 1; }
}

check() {
    xcodebuild build "${project[@]}" -scheme iOS -destination generic/platform=iOS -configuration Release \
        CODE_SIGNING_ALLOWED=NO "${identity[@]}"
    local app="$out/derived/Build/Products/Release-iphoneos/$name.app"
    expect "$app/Info.plist" CFBundleIdentifier "$bundle"
    expect "$app/Info.plist" CFBundleDisplayName "$name"
    expect "$app/Info.plist" CFBundleVersion "$build"
    expect "$app/Info.plist" CodyncAppGroup "group.$bundle"
    expect "$app/Info.plist" CodyncURLScheme codync
    for extension in LiveActivity:CodyncWidgets NotificationService:CodyncNotificationService; do
        local plist="$app/PlugIns/${extension#*:}.appex/Info.plist"
        expect "$plist" CFBundleIdentifier "$bundle.${extension%%:*}"
        expect "$plist" CodyncAppGroup "group.$bundle"
    done

    # The running app keeps its pairings and settings in its own group, not the App Store app's.
    local simulator
    simulator=$(xcrun simctl list devices available iPhone | grep -Eom1 '[0-9A-F]{8}(-[0-9A-F]{4}){3}-[0-9A-F]{12}') ||
        { echo "no iPhone simulator available" >&2; exit 1; }
    xcodebuild test "${project[@]}" -scheme "iOS Dev" -destination "id=$simulator" \
        -only-testing:iOSAppTests/AppGroupTests "${identity[@]}"
    echo "$name ($bundle) build $build: ready to sign"
}

# The signer is the only part that holds the App Store Connect key: it runs the xcodebuild command it's given with the
# key's options added (README.md).
upload() {
    : "${OWN_IOS_TEAM:?set OWN_IOS_TEAM to the Apple team id}"
    local signer=${OWN_IOS_SIGNER:?set OWN_IOS_SIGNER to the command that runs xcodebuild with your App Store Connect key}
    local archive=$out/Codync.xcarchive
    rm -rf "$archive"
    "$signer" xcodebuild archive "${project[@]}" -scheme iOS -destination generic/platform=iOS -archivePath "$archive" \
        DEVELOPMENT_TEAM="$OWN_IOS_TEAM" "${identity[@]}"
    cat >"$out/ExportOptions.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>method</key><string>app-store-connect</string>
    <key>destination</key><string>upload</string>
    <key>teamID</key><string>$OWN_IOS_TEAM</string>
    <key>manageAppVersionAndBuildNumber</key><false/>
</dict>
</plist>
EOF
    "$signer" xcodebuild -exportArchive -archivePath "$archive" -exportOptionsPlist "$out/ExportOptions.plist" \
        -exportPath "$out/export"
    echo "$name ($bundle) build $build: uploaded to TestFlight"
}

# Builds in a worktree of its own, so the checkout it runs from is never touched; records each commit it uploaded.
# One run at a time: a scheduled run that finds the last one still building leaves (exit 75).
follow() {
    if [[ -z ${OWN_BUILD_LOCKED:-} ]]; then
        mkdir -p "$out"
        OWN_BUILD_LOCKED=1 exec lockf -k -t 0 "$out/follow.lock" "$root/apps/ios/own-build.sh" follow
    fi
    local branch=${OWN_IOS_FOLLOW:-origin/dev}
    local tree=$out/follow state=$out/followed
    git -C "$root" fetch --quiet "${branch%%/*}" "${branch#*/}"
    local head
    head=$(git -C "$root" rev-parse "$branch")
    if [[ -f $state && $(<"$state") == "$head" ]]; then
        echo "$branch at ${head:0:12}: already uploaded"
        return
    fi
    if [[ -d $tree ]]; then
        git -C "$tree" checkout --quiet --detach "$head"
    else
        git -C "$root" worktree add --quiet --detach "$tree" "$head"
    fi
    "$tree/apps/ios/own-build.sh" upload
    echo "$head" >"$state"
}

case $mode in
check | upload | follow) "$mode" ;;
*)
    echo "usage: own-build.sh check|upload|follow" >&2
    exit 2
    ;;
esac
