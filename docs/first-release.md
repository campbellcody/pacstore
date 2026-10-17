# First-release design

## Scope and supported-system targets

The first release targets x86_64 Arch Linux with a current, fully upgraded pacman
installation. Other pacman-based distributions are compatibility candidates,
subject to testing their repository configuration and authorization setup; they
are not initially supported. AUR, Flatpak, Snap, repository editing, mirror
management, and unattended upgrades are outside this release.

Target GNOME and KDE Plasma sessions on Wayland and X11, with Sway as an
additional Wayland test target. No desktop-specific services should be required
for browsing. Real transactions will require a working polkit agent. These are
acceptance targets, not a claim that those environments have already been tested.

Minimum functional requirements:

- A Wayland or X11 graphical session, a wgpu-compatible GPU/driver, and a window
  area of at least 760 × 520 logical pixels.
- Read access to pacman's configuration and databases; network access for
  repository refreshes and downloads. Cached browsing should work offline.
- Permission to authenticate through the planned privileged helper for changes;
  the graphical application itself runs as the desktop user.
- Enough free disk space for each proposed transaction, checked before changes.
- Rust 1.95 or newer and a native C build toolchain when building from source.

CPU, RAM, and catalog-size baselines must be measured with the real backend
before publishing hardware recommendations. macOS, Windows, and web deployment
are not release targets.

## Core user flows

| Flow | First-release behavior |
| --- | --- |
| Browse | Open Explore for notable apps from core/extra grouped by purpose; use Search to browse the full catalog and open details. |
| Search | Search package names and descriptions; enrich results with application names and categories where metadata exists. Keep the all-packages switch available. |
| Inspect | Show name, summary, repository, available/installed versions, license, upstream URL, dependencies, sizes, and available application artwork. Missing metadata must not hide a package. |
| Install | Prepare a transaction, review dependencies/conflicts/replacements and sizes, confirm, authenticate, follow progress, then refresh installed state. |
| Remove | Preview the removal and any dependent packages; require explicit confirmation before authentication and execution. Dependency cleanup is a separate reviewed action. |
| Update | Show all available system updates, including libraries. Prepare a full system upgrade, review, confirm, authenticate, and show progress/results. Do not offer individual package updates. |

Canceling a preview changes nothing. Failed or canceled authentication leaves the
transaction unstarted. Report backend failures with actionable detail. During a
transaction, allow cancellation only where the backend can stop safely. Never
refresh package databases and then offer an unsupported partial upgrade.

## Screen sketches

Shared shell:

```text
Pacstore                             [status / active operation]
[Explore] [Search] [Installed] [Updates]
---------------------------------------------------------------
Page heading and controls
Scrollable package list or page-specific empty/loading/error state
```

```text
Explore                         Search
Featured app collections        [Search name, description, category...]
[Search all packages]           [ ] Show all packages
Name / category / summary       Matching names / summaries / status
[Details]                       [Details]

Installed                       Updates
Installed applications          Full system upgrade
[ ] Show all packages           [Review full upgrade (count)]
Name / installed status         Every update, including libraries
[Details]                       Current version -> available version

Package details                 Transaction review
Name / summary                  Proposed install / remove / full upgrade
Repository / versions           Dependencies / removals / conflicts
Metadata / dependencies         Download / disk changes
[Install or Remove]             [Confirm and authenticate] [Cancel]
[View system updates if needed]  Progress / result after confirmation
```

The executable is an interactive version of these sketches. It supplies fixture
versions, installed state, and categories, and lets users confirm **demo** state
changes. Real dependency previews, authentication, progress, metadata retrieval,
and package operations belong to subsequent TODO sections.

## Desktop applications versus all packages

Use AppStream desktop-application components mapped to repository package names
as the primary application classification. Where AppStream is missing, use
available desktop-entry metadata. A displayable desktop entry is evidence of an
application; a package category or name alone is not. Unclassified packages remain
available under **Show all packages**, with the package name and description as
fallbacks. Multiple application components may refer to one package; transactions
operate on package identity.

Explore uses explicit featured collections from core/extra, independent of general
filters and metadata classification. Search and Installed default to applications.
The all-packages switch applies across those two pages. Updates always includes every upgradable installed package,
regardless of this switch, to preserve a full system upgrade. The prototype uses
an explicit fixture flag to exercise that policy until metadata is integrated.

## Toolkit decision

Visual styling follows the [blue–cyan color palette](color-palette.md), with dark
and light theme tokens and shared interaction and status rules.

Use **eframe 0.37 with egui and the wgpu renderer**. eframe owns the native window,
event loop, and rendering integration; its wgpu feature supplies egui-wgpu, so a
separate direct egui-wgpu dependency is unnecessary. Enable Wayland, X11, and
bundled default fonts. Keep graphics/render-loop code out of package management.

The prototype exercises navigation, scrolling package lists, live search,
filtering, details, confirmation/cancellation, and an asynchronous catalog load.
A worker sends owned catalog data through a channel and requests a repaint; the
UI polls without blocking. Reuse that boundary for later package queries, with
transaction events and errors added when the backend exists.

This keeps the first interface small and entirely Rust. Acceptance work still
includes large-catalog performance, desktop integration, accessibility, and
rendering on the target drivers. Those must be tested before release.

API references: [eframe](https://docs.rs/eframe/0.37.0/eframe/),
[App lifecycle](https://docs.rs/eframe/0.37.0/eframe/trait.App.html),
[NativeOptions](https://docs.rs/eframe/0.37.0/eframe/struct.NativeOptions.html).
