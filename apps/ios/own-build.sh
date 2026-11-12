#!/usr/bin/env bash
# Builds this checkout as an iPhone app of your own, next to the App Store Codync: README.md.
#   own-build.sh check   unsigned build; checks the app's identity (no Apple account needed)
#   own-build.sh upload  archives, signs and uploads to TestFlight
# Settings (environment): OWN_IOS_BUNDLE_ID (required), OWN_IOS_TEAM (required to upload),
# OWN_IOS_APP_NAME (default "Codync Own"), BUILD_NUMBER (default: commits on HEAD);
# upload also needs ASC_KEY_ID, ASC_ISSUER_ID and ASC_PRIVATE_KEY (an App Store Connect API key).
set -euo pipefail

mode=${1:?usage: own-build.sh check|upload}
root=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
bundle=${OWN_IOS_BUNDLE_ID:?set OWN_IOS_BUNDLE_ID, e.g. com.example.codync}
name=${OWN_IOS_APP_NAME:-Codync Own}
# Every push to dev, upstream catch-up merges included, adds commits, so the count only grows.
build=${BUILD_NUMBER:-$(git -C "$root" rev-list --count HEAD)}
out=$root/build/dd/own

# The App Store app's group belongs to its team; this app gets its own. Keychain groups are already
# separate: they start with the team's prefix.
settings=(
    CODYNC_IOS_BUNDLE_ID="$bundle"
    CODYNC_APP_GROUP="group.$bundle"
    CODYNC_APP_NAME="$name"
    CURRENT_PROJECT_VERSION="$build"
    ${OWN_IOS_TEAM:+DEVELOPMENT_TEAM="$OWN_IOS_TEAM"}
)
xcode=(-project "$root/apps/Codync.xcodeproj" -scheme iOS -destination generic/platform=iOS -derivedDataPath "$out/derived" -quiet)

expect() { # plist key value
    local got
    got=$(/usr/libexec/PlistBuddy -c "Print :$2" "$1")
    [[ $got == "$3" ]] || { echo "$1 $2: expected $3, got $got" >&2; exit 1; }
}

case $mode in
check)
    xcodebuild build "${xcode[@]}" -configuration Release CODE_SIGNING_ALLOWED=NO "${settings[@]}"
    app="$out/derived/Build/Products/Release-iphoneos/$name.app"
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
    echo "$name ($bundle) build $build: ready to sign"
    ;;
upload)
    : "${OWN_IOS_TEAM:?set OWN_IOS_TEAM to the Apple team id}" "${ASC_KEY_ID:?}" "${ASC_ISSUER_ID:?}" "${ASC_PRIVATE_KEY:?}"
    key=$(mktemp)
    trap 'rm -f "$key"' EXIT
    printf '%s\n' "$ASC_PRIVATE_KEY" >"$key"
    auth=(-allowProvisioningUpdates -authenticationKeyPath "$key" -authenticationKeyID "$ASC_KEY_ID" -authenticationKeyIssuerID "$ASC_ISSUER_ID")
    xcodebuild archive "${xcode[@]}" -archivePath "$out/Codync.xcarchive" "${auth[@]}" "${settings[@]}"
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
    xcodebuild -exportArchive -archivePath "$out/Codync.xcarchive" -exportOptionsPlist "$out/ExportOptions.plist" -exportPath "$out/export" "${auth[@]}"
    echo "$name ($bundle) build $build: uploaded to TestFlight"
    ;;
*)
    echo "usage: own-build.sh check|upload" >&2
    exit 2
    ;;
esac
