# Codync for iPhone

The iPhone app (SwiftUI). The app's code is in `App/`, `Views/` and the `Kit/` package; the widgets in `Widgets/`; the
Xcode project comes from `apps/project.yml`. Building, tests and releases: [development](../../docs/guides/development.md).

## Your own copy

A copy of this repository (a fork, with its own changes) can build its iPhone app as a separate app that installs
next to the App Store Codync: its own name, bundle id and App Group. Its build number is the number of commits, so it
grows with every change, upstream catch-ups included (merge upstream in, never force-push to fewer commits).

1. Copy `own-build.env.example` to `~/.config/codync/own-build.env` (or the file `OWN_BUILD_SETTINGS` names) and set
   `OWN_IOS_BUNDLE_ID`, a bundle id of your own (e.g. `com.example.codync`), and `OWN_IOS_APP_NAME`, the name under
   the icon (default `Codync Own`). Nothing about your copy is committed.
2. `apps/ios/own-build.sh` archives the app unsigned to `build/dd/own/Codync.xcarchive`, checks its bundle ids, App
   Group, name and build number, and runs it in an iPhone simulator to check it keeps its pairings and settings in its
   own App Group. It needs no Apple account.

The archive is not an installable `.ipa`: installing it on an iPhone or sending it to TestFlight means signing and
exporting it with an Apple Developer membership of your own (Xcode → Organizer → Distribute App), which the script
doesn't do.

### What works in it

The app uses the same Codync cloud as the App Store app, but the maintainer's services don't know your copy:

- **Works:** pairing a computer that isn't in a Codync account by scanning its QR code (desktop app → **Pair iPhone**,
  or `codync-host pair`), then its bots and chats over the same Wi-Fi or Tailscale; widgets, Live Activities while the
  app runs, voice calls and the remote screen of those computers.
- **Doesn't work: push notifications.** The push relay sends only for the App Store app's bundle id, with the
  maintainer's Apple key, so your copy gets no "needs you / done" notifications and no Live Activity updates while
  it's closed.
- **Doesn't work: signing in to a Codync account** (Apple, Google or GitHub). Sign-in goes through the maintainer's
  account service, which accepts only the apps registered with it; use **Continue without an account**. A signed-out
  phone can't use the Codync relay, and a computer in an account refuses it: take the computer out of its account
  first (desktop app → Settings → Computers → **Remove from account**, or `codync-host cloud --leave`), then pair.
- Pairing links opened from another app (`codync://`) may open either app when both are installed; the in-app scanner
  always pairs the app it's in.
- The update prompt and the review link follow the App Store Codync's version, and usage analytics go to the
  maintainer's project like the App Store app's (Settings → **Share usage analytics** turns them off).
