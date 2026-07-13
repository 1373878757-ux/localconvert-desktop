# macOS Signing and Notarization

**LocalConvert Desktop**

by 田宸宇

This guide prepares the macOS Apple Silicon release workflow for distribution outside the Mac App Store. It does not contain credentials and does not make signing part of normal development builds. An unsigned local build is useful for development and testing, but it must not be described or published as a trusted release.

## Release Boundary

The current macOS deliverables are:

- `LocalConvert Desktop.app`
- `LocalConvert Desktop_<version>_aarch64.dmg`

The release path uses a **Developer ID Application** certificate to sign the app, its nested executable sidecars, and the disk image. A **Developer ID Installer** certificate is not required for the current `.app` and `.dmg` output. It is required only if a future release introduces a signed macOS Installer Package (`.pkg`).

Apple notarization is a separate release requirement. The submitted software must be signed with Developer ID, use a secure timestamp and hardened runtime where applicable, contain valid signatures for every nested executable, receive an `Accepted` result from Apple's notary service, and have its ticket stapled before distribution.

## Prerequisites

- An active paid Apple Developer Program membership. A free account cannot notarize a Developer ID release.
- A current Xcode or Command Line Tools installation that provides `codesign`, `spctl`, `notarytool`, and `stapler`.
- A Developer ID Application certificate and its private key installed under **My Certificates** in the local Keychain.
- The Apple Team ID associated with that certificate.
- One notarization authentication method: an App Store Connect API key or an Apple ID app-specific password.

Confirm that macOS can see the signing identity:

```bash
security find-identity -v -p codesigning
```

The expected identity starts with `Developer ID Application:` and includes the correct Team ID. Do not use an Apple Development, Mac Development, Apple Distribution, or ad-hoc identity for a public Developer ID release.

## Credential Safety

Never commit or attach any of the following to this repository:

- `.p12`, `.p8`, `.cer`, `.pem`, or private-key files
- Certificate or Keychain passwords
- Apple ID app-specific passwords
- App Store Connect API key contents
- Exported `.keychain` or `.keychain-db` files
- Provisioning profiles
- Populated `.env` files

Keep local certificate material in Keychain and keep API private keys outside the repository. CI values belong in the CI provider's encrypted secret store. The repository `.gitignore` is only a last line of defense; it is not a substitute for proper secret storage.

## Tauri Environment Variables

### Code signing

| Variable | Use |
| --- | --- |
| `APPLE_SIGNING_IDENTITY` | Exact Developer ID Application identity from `security find-identity`. This overrides `bundle.macOS.signingIdentity`. |
| `APPLE_CERTIFICATE` | Optional CI-only base64 encoding of an exported `.p12` certificate. Not needed when the identity is already in the local Keychain. |
| `APPLE_CERTIFICATE_PASSWORD` | Password for the CI `.p12` export. Required when `APPLE_CERTIFICATE` is used. |
| `KEYCHAIN_PASSWORD` | CI helper value for creating and unlocking a temporary Keychain. It is not a committed project setting. |

Do not set `TAURI_SKIP_SIDECAR_SIGNATURE_CHECK`. The bundled `qpdf` and `image-engine` executables must be signed and verified as nested code.

### Notarization with an App Store Connect API key

This is the preferred automation path:

| Variable | Use |
| --- | --- |
| `APPLE_API_ISSUER` | App Store Connect issuer ID. |
| `APPLE_API_KEY` | App Store Connect API key ID, not the private-key contents. |
| `APPLE_API_KEY_PATH` | Absolute path to the downloaded `AuthKey_<key-id>.p8` file outside the repository. |

### Notarization with an Apple ID

Use this as an alternative to the API-key variables:

| Variable | Use |
| --- | --- |
| `APPLE_ID` | Apple account email. |
| `APPLE_PASSWORD` | Apple ID app-specific password. Tauri also supports its documented `@keychain:` or `@env:` indirection. |
| `APPLE_TEAM_ID` | Apple Developer Team ID. |
| `APPLE_PROVIDER_SHORT_NAME` | Optional provider short name when the Apple ID belongs to multiple teams. |

Configure exactly one notarization authentication method for a release build. Do not put literal secret values in shell scripts, `.env` files, Tauri configuration, Git history, issue comments, or release notes.

## Signed Tauri Build

For local Keychain signing with App Store Connect API authentication, provide values only in the release shell:

```bash
export APPLE_SIGNING_IDENTITY="Developer ID Application: <name> (<team-id>)"
export APPLE_API_ISSUER="<issuer-id>"
export APPLE_API_KEY="<key-id>"
export APPLE_API_KEY_PATH="/absolute/path/outside/repository/AuthKey_<key-id>.p8"

npm run tauri build
```

When the signing and notarization variables are absent, the same command remains the existing unsigned local build. The project intentionally does not hardcode `signingIdentity` and does not make missing release credentials fail development builds.

Tauri's release build must continue to run the manifest-driven engine verification gate before bundling. Do not bypass asset SHA-256, executable permission, or license checks for a signed release.

## Sidecar Signing

The following nested Mach-O executables are security-sensitive release contents:

```text
LocalConvert Desktop.app/Contents/MacOS/qpdf
LocalConvert Desktop.app/Contents/MacOS/image-engine
```

Tauri receives them through `externalBin` and should sign them before signing the outer app bundle. Sign nested code from the inside out. Do not use `codesign --deep` as a replacement for correct signing; use `--deep` only for final verification.

Set artifact paths before running the checks below:

```bash
APP="src-tauri/target/release/bundle/macos/LocalConvert Desktop.app"
DMG="src-tauri/target/release/bundle/dmg/LocalConvert Desktop_<version>_aarch64.dmg"
```

Verify each sidecar independently:

```bash
codesign --verify --strict --verbose=2 "$APP/Contents/MacOS/qpdf"
codesign --verify --strict --verbose=2 "$APP/Contents/MacOS/image-engine"
codesign -d --verbose=4 "$APP/Contents/MacOS/qpdf"
codesign -d --verbose=4 "$APP/Contents/MacOS/image-engine"
```

Each detailed signature must show the intended Developer ID Application authority and Team ID. A valid outer app signature does not excuse an unsigned or incorrectly signed sidecar.

## Pre-Notarization Verification

Run these checks on the exact artifacts that will be submitted:

```bash
codesign --verify --deep --strict --verbose=2 "$APP"
codesign --verify --strict --verbose=2 "$DMG"
codesign -d --verbose=4 "$APP"
codesign -d --verbose=4 "$DMG"
```

Review the detailed output for:

- `Authority=Developer ID Application: ...`
- The expected `TeamIdentifier`
- A trusted timestamp
- Hardened runtime on the app executable
- No unsealed content, invalid resource envelope, or nested-code errors

Any failure is release-blocking. Do not repair a completed bundle by force-signing only the outer `.app`; rebuild with the correct identity so every nested component is signed in the correct order.

## Manual notarytool Fallback

Tauri can submit and staple automatically when its notarization environment variables are present. If a release needs a manual fallback, store credentials in Keychain rather than in the repository:

```bash
xcrun notarytool store-credentials "localconvert-notary" \
  --apple-id "$APPLE_ID" \
  --team-id "$APPLE_TEAM_ID" \
  --password "$NOTARY_APP_PASSWORD"
```

`NOTARY_APP_PASSWORD` is a temporary local environment value containing an app-specific password. Unset it after the Keychain profile is created.

Submit the signed DMG and wait for a terminal result:

```bash
xcrun notarytool submit "$DMG" \
  --keychain-profile "localconvert-notary" \
  --wait
```

The result must be `Accepted`. Save the submission ID and inspect the log, including warnings:

```bash
xcrun notarytool log "<submission-id>" \
  --keychain-profile "localconvert-notary"
```

Do not continue when the result is `Invalid`, the submission times out without a known final state, or the log reports signature problems.

## Stapling and Gatekeeper Validation

After an accepted submission, staple and validate the distributed DMG:

```bash
xcrun stapler staple "$DMG"
xcrun stapler validate "$DMG"
```

If the `.app` is distributed separately, staple and validate that separately submitted artifact as well. Do not modify or re-sign an artifact after stapling.

Run final Gatekeeper assessments:

```bash
spctl --assess --type execute --verbose=4 "$APP"
spctl --assess --type open \
  --context context:primary-signature \
  --verbose=4 "$DMG"
```

The expected result is accepted with a notarized Developer ID source. Test the DMG on a clean macOS user account or clean machine after downloading it through the intended distribution path so quarantine and first-launch behavior are exercised.

## Release Checklist

1. Confirm `main` is clean and the release version is correct.
2. Run `cargo test` from `src-tauri`.
3. Run `cargo clippy --all-targets -- -D warnings` from `src-tauri`.
4. Run `node src-tauri/scripts/verify-engines.mjs` from the repository root.
5. Confirm `security find-identity -v -p codesigning` lists the intended Developer ID Application identity.
6. Supply signing and exactly one notarization credential set outside the repository.
7. Run `npm run build` and `npm run tauri build`.
8. Verify `qpdf`, `image-engine`, the outer `.app`, and the DMG with `codesign`.
9. Confirm the notarization submission is `Accepted` and inspect its log.
10. Staple the ticket and run `xcrun stapler validate`.
11. Run both `spctl` assessments.
12. Install and launch from the DMG on a clean, offline-capable test environment.
13. Smoke-test qpdf and image conversion, confirm sources remain unchanged, and confirm no network is required.
14. Publish only the exact signed, notarized, stapled, and verified artifact.

## Official References

- [Tauri macOS code signing and notarization](https://v2.tauri.app/distribute/sign/macos/)
- [Tauri environment variables](https://v2.tauri.app/reference/environment-variables/)
- [Apple Developer ID certificates](https://developer.apple.com/help/account/certificates/create-developer-id-certificates)
- [Apple notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow)
- [Apple notarization overview](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)
