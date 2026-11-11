# Updates

> **2026-11-11.** The winit + softbuffer shell (`bins/mdv-app`) has no automatic update check and
> does not call `mdview_core::update`. Its only network use besides loopback (the single-instance
> hand-over in `crates/core/src/single.rs`, 127.0.0.1 only) is `mdview-fetch`, which downloads a
> document's remote images through WinHTTP, and only after the reader allows them (2026-11-12). Updates are manual by decision: a
> "Check for updates" menu item is future work, and the flow below describes it, not shipped
> behaviour. The Qt AppImage's daily check is legacy and goes away with the Qt shell.

Design for checking, downloading and applying updates. The shared logic (version ordering, manifest
parsing, SHA-256) lives in `crates/core/src/update.rs` and is pure: it does no network I/O. The
shells do the downloading.

## Flow (both platforms)

1. At startup, unless the user turned it off, and at most once per 24 hours (timestamp kept in the
   session/settings file), fetch the manifest on a background thread. Failure is silent.
2. Parse with `UpdateManifest::parse`; any error means "no update". Ask `update::newer(current,
   manifest, Platform::current())`.
3. If there is an asset, tell the user (a non-modal notice with the notes). Nothing downloads until
   they agree.
4. Download `asset.url` (https only; the manifest parser already enforced that) to a temp file,
   cap the size at a sane limit (for example 200 MB), then `verify_sha256_file`. On mismatch delete
   the file and report failure; never run it.
5. Apply (platform sections below).

Setting: `check_for_updates` (default on). Opt out disables step 1 entirely; a menu item "Check for
updates" still performs it manually and ignores the 24 hour limit.

## Manifest format

Hosted as a release asset named `mdview-update.txt` on the latest GitHub release of
`Kaan88/markdown-viewer-rs`, fetched from
`https://github.com/Kaan88/markdown-viewer-rs/releases/latest/download/mdview-update.txt`.
The full grammar and limits are in the module docs of `crates/core/src/update.rs`; in short:

```text
manifest: 1
version: 0.2.0
notes: One line of release notes.
windows-x86_64.asset: mdview-setup-0.2.0.exe
windows-x86_64.url: https://github.com/Kaan88/markdown-viewer-rs/releases/download/v0.2.0/mdview-setup-0.2.0.exe
windows-x86_64.sha256: <64 hex digits>
linux-x86_64.asset: ...
linux-x86_64.url: ...
linux-x86_64.sha256: ...
```

Limits: 8 KiB, 64 lines, https URLs only (<= 512 bytes, no userinfo), 64-hex SHA-256, asset names
limited to `[A-Za-z0-9._-]`, unknown or duplicate keys reject the whole manifest. A pre-release
version is only offered to a build that is older by SemVer precedence.

The hash in the manifest is the integrity check for the download, not an authenticity check: the
manifest's own authenticity comes from TLS to GitHub. (Signing the manifest is a possible later step.)

## Windows

* Artifact: `mdview-setup-<version>.exe`, built by `packaging/windows/build-installer.ps1`
  (Inno Setup, `packaging/windows/mdview.iss`), which also writes `mdview-setup-<version>.exe.sha256`.
  Publishing a release means uploading the installer and a `mdview-update.txt` whose
  `windows-x86_64.sha256` equals that file's hash.
* Apply: after the user confirms, the shell starts the downloaded installer with
  `/SILENT /CLOSEAPPLICATIONS` and exits. The installer closes any running viewer (Restart Manager),
  replaces the files, and `[Run]` does not relaunch when silent. Install mode (per-user or all users)
  is the one chosen originally: Inno reuses it from the existing uninstall entry for the same `AppId`.
  The installer file is deleted by the shell on its next start (or left in the temp folder).
* Dependencies: the installer itself checks for the Windows App Runtime 2.6.4 or newer and, if
  missing, downloads the official Microsoft installer, verifies that its Authenticode signer is
  Microsoft Corporation, and runs it with `--quiet`.
* File associations are an optional installer task and only add mdview to "Open with"; the default
  handler is never overwritten.

## Linux (AppImage)

<!-- PLACEHOLDER: the Linux packaging agent adds its section here (AppImageUpdate / zsync delta). -->
