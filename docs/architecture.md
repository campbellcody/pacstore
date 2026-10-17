# Application structure

| Module | Responsibility |
| --- | --- |
| `main` | Initialize logging and preferences before creating the native window. |
| `ui` | Navigation, search/filter state, loading/empty/error views, details, and confirmations. |
| `packages` | Owned package/catalog models, search indexing, and optional fixtures. |
| `backend` | Read-only pacman queries, AppStream enrichment, installed files, image decoding, and catalog caching. |
| `transaction` | Owned install/remove/full-upgrade requests and demo execution. |
| `helper` | Validate in-memory demo requests. |
| `protocol` | Versioned commands/events and bounded framing shared with the helper. |
| `system_transaction` | Own the polkit subprocess and stream plans, questions, progress, and results. |
| `bin/pacstore-helper` | Independently validate requests and prepare/commit with configured libalpm. |
| `task` | Run fallible work outside the UI thread and deliver an owned result over a channel. |
| `error` | Typed configuration, I/O, invalid-request, and worker-failure errors. |
| `config` | Versioned preferences, XDG path selection, validation, and atomic writes. |
| `artwork` | Lazy worker decoding, 96 px thumbnails, and a bounded 128-entry texture cache. |
| `recovery` | Contextual next steps for failed operations. |
| `i18n` | English message IDs, compiled catalogs, and named placeholder substitution. |
| `logging` | Application-only stderr logging controlled by PACSTORE_LOG. |

## State and work

The GUI owns navigation, catalog, selection, transaction preview, notifications,
and the current loading/transaction/save tasks. At most one catalog or transaction
task can run at a time. Reset is disabled during those operations. Transaction
requests carry package names across the worker boundary; row indexes stay inside
the UI. The worker changes an owned catalog snapshot, so a failure leaves the
visible package state untouched. Successful results replace the snapshot and
refresh installed/update views together.

Workers return typed results and request a repaint on completion. The GUI polls
channels without waiting. Worker errors and panics become visible errors rather
than an endless spinner. Preference writes also run in a worker; the preference
control is disabled until the write completes. Initial configuration loading runs
before the event loop starts. Closing waits for active work to finish; a failed
save leaves the window open so its error can be read.

The existing loading, empty-result, success-notice, and error views remain part of
the shared shell. Errors can be dismissed and catalog loads retried. A failed
preference write keeps the current choice for this session and reports that it
could not be saved. Reloading the catalog does not repair an invalid preferences
file.

## Privileged helper boundary

The GUI's `system_transaction` session launches only the installed fixed helper
through polkit. `protocol` defines bounded structured commands and events; the
helper independently validates requests, resolves a locked plan, asks questions,
and waits for final confirmation before committing with libalpm. The GUI stays
unprivileged. `helper` validates only the optional in-memory demo operations.

See [transactions.md](transactions.md) for installation, cancellation, closure,
signature and lock behavior, and acceptance-test limitations.

## Verification

Tests cover catalog and filter policy, demo transaction state, full upgrades that
include hidden libraries, stale/invalid requests, worker failure and panic results,
configuration path selection, round trips, invalid-file preservation, and headless
rendering. CI uses the committed lockfile for lint, tests, and builds. Desktop and
GPU testing remains necessary on supported systems before release.

## Package data

The default backend reads actual local databases, validates/rebuilds a persistent
catalog snapshot, and enriches records using local application metadata. Details
load installed files and decode optional icon images in a separate worker; only
texture upload happens on the GUI thread. Package lists render visible rows only.
See [package-backend.md](package-backend.md) for the CLI decision and source policy.

Integration tests launch the application with isolated fake pacman executables,
metadata, and cache directories. They exercise real subprocess/parse/cache code
without depending on host packages or changing the host database.

## Store experience

Installed scopes use pacman's installation reason and orphan query. Orphan cleanup
is a separate protocol operation; the helper recomputes reverse dependencies under
the transaction lock. Launching is unprivileged and checks installed desktop-entry
ownership immediately before delegating to gio. Artwork requests are limited to
visible rows and at most eight workers; missing images are negatively cached.
AccessKit exposes named controls, and shortcuts complement egui's focus traversal.
Theme and scale preferences are applied through the egui context and persisted.
See [store-experience.md](store-experience.md) for user and translator guidance.
