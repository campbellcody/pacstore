# Store experience

## Discover, search, and launch

Explore retains the curated core/extra collections. Search offers the full catalog
through its application/all-packages switch, repository/category filters, and
installation filter. Package rows show names, repository, summary, installation
status, and icons, with a letter fallback for missing or unsupported images.

Rows and prepared transaction plans are virtualized. Only visible rows create
widgets. PNG/JPEG icons decode in workers and are reduced to 96 px thumbnails;
the UI uploads textures into a 128-entry cache, with at most eight concurrent
loads. Missing artwork is cached too. Reload clears the artwork cache. Details
retain metadata, screenshots, and installed-file lists.

**Install…** and **Remove…** start the reviewed transaction flow. Installed
applications with a displayable, owned desktop entry offer **Launch application**
(or multiple named launch actions for packages containing several applications).
Launch revalidates ownership with pacman, restricts paths to system application
directories, and delegates to `/usr/bin/gio launch` as the desktop user. It never
parses Exec itself or invokes a shell. It does not launch apps automatically during
catalog loads or tests. Missing gio, desktop sessions, or launchers produce a
recoverable error. Custom-root packages and packages without desktop entries have
no launch action. A successful launch request does not monitor the application's
subsequent lifetime.

## Installed and Updates

Installed defaults to **Manually installed**, with separate **Dependencies**,
**All installed**, and **Orphans** scopes. These include command-line packages and
libraries, independently of Search's application-only switch or install-status
filter. Repository and category filters still apply; installed text search is
separate from catalog search. Packages with unknown install reasons remain visible
under All installed.

**Review orphan cleanup** is independent of display filters. Its preflight explains
that the helper will recompute all unused dependency packages under the lock.
Manually installed packages and required/optional reverse dependencies are
preserved. Review the actual list and confirm before removal. New orphans created
by cleanup can be reviewed with another pass. This requires the version-2 helper
installed from the same build as the GUI; see [transactions.md](transactions.md).

Updates includes every upgradable installed package regardless of display filters.
Reviewing a full upgrade refreshes repositories and resolves the actual plan;
final confirmation is always required. Transaction results remain in the dialog,
and the catalog reloads after success or failure. Contextual recovery guidance
appears beside errors and can be copied along with the diagnostic or transaction
log. No retry silently reuses an unreviewed old transaction plan.

## Keyboard and accessibility

- **Ctrl+F:** open Search and focus its input.
- **Ctrl+1 / 2 / 3 / 4:** Explore / Search / Installed / Updates.
- **F5:** reload the catalog when no transaction is active.
- **Tab / Shift+Tab:** traverse visible interactive controls.
- **Enter / Space:** activate the focused control.
- **Escape:** close package details or an unstarted preflight. It does not dismiss
  the active transaction or confirm/cancel package changes.

The Keyboard shortcuts menu exposes these controls in the app. Native AccessKit
support is enabled, including Linux AT-SPI integration when assistive technology
requests it. Search fields and filter controls have labels, repeated Details
buttons include package names in their accessible labels, and icons carry
application text alternatives. Selection/focus markers remain visible and status
colors are paired with text. Theme tests enforce 4.5:1 normal-text contrast on
supported surfaces and 3:1 meaningful control boundaries.

**Theme** offers System, Light, and Dark. **Scale** offers 85%, 100%, 125%, and
150%, independently of the monitor's native DPI scaling. Both persist in
settings.conf, with `scale=100%` as the backwards-compatible default. Native screen
reader behavior, actual focus traversal, and window layout still need acceptance
testing on supported desktops; headless tests validate focus, accessibility tree
labels, contrast policy, and rendering behavior rather than claiming that audit.

## Translation preparation

UI message IDs are English strings used through `t!` and `trf!`, with named
placeholders allowing translators to reorder values. `locales/en.json` is the
source catalog. Only English ships today; no partial translation is advertised.

```sh
python3 scripts/update-translations.py
python3 scripts/update-translations.py --check
```

The extractor reads UI macros and featured/recovery tables; CI checks that the
catalog is current. To add a language, copy the English JSON to a locale file,
translate values while preserving placeholder names, and register it in
`src/i18n.rs`'s CATALOGS table. Select a compiled language using PACSTORE_LANGUAGE,
then LC_ALL, LC_MESSAGES, or LANG. Unsupported languages fall back to English.
Missing keys fall back to their English message IDs. Invalid placeholder sets are
ignored; tests verify every shipped catalog.

Package descriptions, desktop metadata, versions, repository names, helper output,
and raw package-manager diagnostics remain source data. The catalog backend's
current base/English metadata policy remains unchanged. Display number formatting,
plurals, and broader metadata localization are follow-up work before shipping
additional languages.

## Verification

Tests cover Installed scopes and demo cleanup, rejection of arbitrary desktop
launchers, large-list virtualization, labeled keyboard search focus with an
accessibility tree, contrast in both themes, named placeholder safety, and native
orphan cleanup in a disposable package database. The helper test proves that
manual, required, and optional packages remain installed. No host applications are
launched and no host packages are changed by these tests.
