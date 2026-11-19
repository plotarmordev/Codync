#!/usr/bin/env bash
# Archives this checkout, unsigned, as an iPhone app of your own next to the App Store Codync (README.md), checks the
# app's identity and runs it in a simulator to check where it keeps its data. Needs no Apple account; signing the
# archive and uploading it to TestFlight are left to you.
# Settings come from the environment or $OWN_BUILD_SETTINGS (default ~/.config/codync/own-build.env), outside the
# repository; own-build.env.example lists them.
set -euo pipefail
(($# == 0)) || { echo "usage: own-build.sh (takes no arguments)" >&2; exit 2; }

settings_file=${OWN_BUILD_SETTINGS:-$HOME/.config/codync/own-build.env}
if [[ -f $settings_file ]]; then
    source "$settings_file"
fi

root=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
bundle=${OWN_IOS_BUNDLE_ID:?set OWN_IOS_BUNDLE_ID, e.g. com.example.codync}
name=${OWN_IOS_APP_NAME:-Codync Own}
# Every new commit, upstream catch-up merges included, raises the count.
build=$(git -C "$root" rev-list --count HEAD)
out=$root/build/dd/own
archive=$out/Codync.xcarchive

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

rm -rf "$archive"
xcodebuild archive "${project[@]}" -scheme iOS -destination generic/platform=iOS -archivePath "$archive" \
    CODE_SIGNING_ALLOWED=NO "${identity[@]}"
app="$archive/Products/Applications/$name.app"
expect "$app/Info.plist" CFBundleIdentifier "$bundle"
expect "$app/Info.plist" CFBundleDisplayName "$name"
expect "$app/Info.plist" CFBundleVersion "$build"
expect "$app/Info.plist" CodyncAppGroup "group.$bundle"
expect "$app/Info.plist" CodyncURLScheme codync
for extension in LiveActivity:CodyncWidgets NotificationService:CodyncNotificationService; do
    plist="$app/PlugIns/${extension#*:}.appex/Info.plist"
    expect "$plist" CFBundleIdentifier "$bundle.${extension%%:*}"
    expect "$plist" CodyncAppGroup "group.$bundle"
done

# The running app keeps its pairings and settings in its own group, not the App Store app's.
simulator=$(xcrun simctl list devices available iPhone | grep -Eom1 '[0-9A-F]{8}(-[0-9A-F]{4}){3}-[0-9A-F]{12}') ||
    { echo "no iPhone simulator available" >&2; exit 1; }
# Booted first: launching the test host into a simulator that is still starting fails.
xcrun simctl bootstatus "$simulator" -b >/dev/null
xcodebuild test "${project[@]}" -scheme "iOS Dev" -destination "id=$simulator" \
    -only-testing:iOSAppTests/AppGroupTests "${identity[@]}"
echo "$name ($bundle) build $build: unsigned archive in $archive"
