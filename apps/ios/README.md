# Codync for iPhone

The iPhone app (SwiftUI). The app's code is in `App/`, `Views/` and the `Kit/` package; the widgets in `Widgets/`; the
Xcode project comes from `apps/project.yml`. Building, tests and releases: [development](../../docs/guides/development.md).

## Your own copy on TestFlight

A copy of this repository (a fork, with its own changes) can ship its iPhone app to its own TestFlight, as a separate
app that installs next to the App Store Codync: its own name, bundle id, App Group and Apple team. Every push to the
copy's `dev`, catching up with upstream included, builds it, and once an App Store Connect key is set, uploads it with
a higher build number: the number of commits on `dev`, so merge upstream in, never force-push `dev` to fewer commits.

`own-build.sh` does the build; the `testflight` job of `.github/workflows/ios-submit.yml` runs it on each push to
`dev`. Nothing about your Apple account is committed: it all comes from the repository's settings.

### Set it up

1. In the copy's GitHub settings → Secrets and variables → Actions → **Variables**, add `OWN_IOS_BUNDLE_ID`, a bundle
   id of your own (e.g. `com.example.codync`). Optional: `OWN_IOS_APP_NAME` (the name under the icon, default
   `Codync Own`).
2. Push to `dev`. The job builds the app unsigned and checks its bundle ids, App Group, name and build number. This needs
   no Apple account. Locally: `OWN_IOS_BUNDLE_ID=com.example.codync apps/ios/own-build.sh check`.
3. Link the Apple account (a paid Apple Developer membership):
   - In App Store Connect → Users and Access → Integrations, create an API key with the **Admin** role (automatic
     signing registers the bundle ids, the App Group and the certificates with it).
   - In App Store Connect → Apps, create the app once with your bundle id (the API can't create apps). If the bundle id
     isn't listed yet, register it first in Certificates, Identifiers & Profiles.
   - Add the variable `OWN_IOS_TEAM` (your team id, Membership details) and the secrets `OWN_IOS_ASC_KEY_ID`,
     `OWN_IOS_ASC_ISSUER_ID` and `OWN_IOS_ASC_PRIVATE_KEY` (the whole `.p8` file).
4. Push to `dev` again: the job now archives, signs and uploads. The build shows in TestFlight after Apple's processing;
   add yourself as an internal tester once, and later builds arrive on their own. Each runner starts with an empty
   keychain, so if an upload fails on the team's certificate limit, revoke the unused certificates Xcode made in
   Certificates, Identifiers & Profiles.

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
