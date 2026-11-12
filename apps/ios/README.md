# Codync for iPhone

The iPhone app (SwiftUI). The app's code is in `App/`, `Views/` and the `Kit/` package; the widgets in `Widgets/`; the
Xcode project comes from `apps/project.yml`. Building, tests and releases: [development](../../docs/guides/development.md).

## Your own copy on TestFlight

A copy of this repository (a fork, with its own changes) can ship its iPhone app to its own TestFlight, as a separate
app that installs next to the App Store Codync: its own name, bundle id, App Group and Apple team. A Mac runs
`own-build.sh follow` on a schedule: each time the copy's `dev` changes, catching up with upstream included, it builds
the newest commit and uploads it with a higher build number, the number of commits on `dev` (so merge upstream in,
never force-push `dev` to fewer commits).

Nothing about your Apple account is committed. The script reads its settings from `~/.config/codync/own-build.env`
(or the file `OWN_BUILD_SETTINGS` names; `own-build.env.example` lists them), and it never touches the App Store
Connect key: it signs and uploads through your **signer**, a command of your own that alone holds the key and runs
the `xcodebuild` command it's given with the key's options added.

### Set it up

1. Copy `own-build.env.example` to `~/.config/codync/own-build.env` and set `OWN_IOS_BUNDLE_ID`, a bundle id of your
   own (e.g. `com.example.codync`), and `OWN_IOS_APP_NAME`, the name under the icon (default `Codync Own`).
2. `apps/ios/own-build.sh check` builds the app unsigned, checks its bundle ids, App Group, name and build number, and
   runs it in an iPhone simulator to check it keeps its pairings and settings in its own App Group. This needs no
   Apple account.
3. Link the Apple account (a paid Apple Developer membership):
   - In App Store Connect → Users and Access → Integrations, create an API key with the **Admin** role (automatic
     signing registers the bundle ids, the App Group and the certificates with it).
   - In App Store Connect → Apps, create the app once with your bundle id (the API can't create apps). If the bundle id
     isn't listed yet, register it first in Certificates, Identifiers & Profiles.
   - Set `OWN_IOS_TEAM` (your team id, Membership details) and `OWN_IOS_SIGNER`, the signer's path. It's called as
     `<signer> xcodebuild archive …` and `<signer> xcodebuild -exportArchive …` and must pass on xcodebuild's exit
     status. Keep the key where only it can read it; for example, with the key next to it:

     ```bash
     #!/bin/bash
     # codync-asc-xcodebuild xcodebuild archive|-exportArchive …: runs it with the App Store Connect key.
     set -euo pipefail
     [[ $1 == xcodebuild && ($2 == archive || $2 == -exportArchive) ]] || { echo "refused: $*" >&2; exit 64; }
     here=$(dirname "$0")
     exec "$@" -allowProvisioningUpdates -authenticationKeyPath "$here/AuthKey.p8" \
         -authenticationKeyID "$(<"$here/key-id")" -authenticationKeyIssuerID "$(<"$here/issuer-id")"
     ```
4. `apps/ios/own-build.sh upload` archives this checkout and uploads it once by hand. To follow `dev`, run
   `own-build.sh follow` on a schedule, e.g. every 15 minutes with `crontab -e`:

   ```
   */15 * * * * /path/to/codync/apps/ios/own-build.sh follow >>/tmp/codync-own-build.log 2>&1
   ```

   It fetches `OWN_IOS_FOLLOW` (default `origin/dev`), builds a new commit in its own worktree (`build/dd/own/follow`,
   leaving your checkout alone) and remembers what it uploaded; a failed upload is tried again on the next run, and a
   run that starts while the last one still builds leaves. The build shows in TestFlight after Apple's processing;
   add yourself as an internal tester once, and later builds arrive on their own.

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
