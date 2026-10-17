# Pacstore TODO

Build a graphical app store for Arch Linux and compatible systems, using pacman
for package management. Start with configured official repositories; keep AUR
support outside the initial release.

## 1. Define the first release

- [x] Document supported distributions, desktop environments, and minimum system requirements.
- [x] Define the core flows: browse, search, inspect, install, remove, and update packages.
- [x] Sketch the Explore, Search, Installed, Updates, and package details screens.
- [x] Decide how to distinguish desktop applications from libraries and other packages, while keeping an option to show all packages.
- [x] Choose a Rust GUI toolkit after prototyping package lists, navigation, and background task integration.

Design and scope: [docs/first-release.md](docs/first-release.md). The executable
implements an interactive fixture-based prototype; real pacman operations follow
in the sections below.

## 2. Set up the application

- [x] Organize the Rust project into UI, package data, transaction, and privileged helper modules.
- [x] Add application state, structured errors, logging, and configuration storage.
- [x] Build the application shell with navigation, loading indicators, empty states, and error messages.
- [x] Run package queries and transactions outside the UI thread.
- [x] Establish formatting, linting, and automated checks in CI.

The shell uses the demo backend. Real package queries and privileged transactions
remain in Parts 3 and 4. See [architecture](docs/architecture.md).

## 3. Read package data

- [x] Evaluate libalpm bindings versus invoking pacman; document the backend choice and maintenance implications.
- [x] Read configured repositories and installed packages using the system's pacman configuration.
- [x] Model package name, version, repository, description, architecture, size, dependencies, and install reason.
- [x] Implement search and filters for repository, installation status, and application category.
- [x] Populate details with licenses, upstream links, dependencies, optional dependencies, and installed files.
- [x] Integrate available AppStream metadata for application names, icons, categories, and screenshots; provide fallbacks when metadata is missing.
- [x] Cache catalog data and invalidate it after repository refreshes and successful transactions.
- [x] Handle unavailable repositories, missing metadata, and offline browsing gracefully.

Backend details: [docs/package-backend.md](docs/package-backend.md). System mode
queries are read-only; database changes made by package managers invalidate the cache.

## 4. Make package changes safely

- [x] Keep the GUI unprivileged; define a narrow privileged helper interface with polkit authorization.
- [x] Validate helper requests and pass structured arguments without shell interpolation.
- [x] Prepare transactions and show dependencies, replacements, conflicts, removals, download size, and disk-space changes before confirmation.
- [x] Implement package installation and removal, including optional dependency selection and explicit handling of dependent packages.
- [x] Implement full system upgrades; prevent workflows that refresh databases and then perform unsupported partial upgrades.
- [x] Respect pacman's database lock and report when another package manager is running; never delete a lock automatically.
- [x] Preserve package signature verification and surface keyring, signature, mirror, network, and disk-space errors.
- [x] Show download and transaction progress, package-manager questions, and readable transaction logs.
- [x] Support cancellation only at safe stages and clearly indicate when a transaction must finish.
- [x] Handle authorization cancellation, helper failures, and application closure without interrupting an active package transaction.
- [x] Refresh installed state and available updates after each completed transaction.

Helper setup, transaction behavior, and acceptance-test limits:
[docs/transactions.md](docs/transactions.md).

## 5. Complete the store experience

- [x] Build searchable package lists with icons, summaries, installation status, and responsive scrolling.
- [x] Add package details with clear install, remove, and launch actions where applicable.
- [x] Add an Installed view and an Updates view with explicit user confirmation before upgrading.
- [x] Separate manually installed packages from dependencies and preview orphan cleanup before removal.
- [x] Display operation results and actionable recovery guidance when a transaction fails.
- [x] Add keyboard navigation, accessible labels, readable contrast, scaling, and light/dark theme support.
- [x] Prepare UI strings for translation.

Store UI, launch policy, cleanup, accessibility, and translation notes:
[docs/store-experience.md](docs/store-experience.md).

## 6. Verify behavior

- [ ] Unit-test search, filtering, package state, and transaction request validation.
- [ ] Test UI loading, empty, offline, authorization-denied, and failure states with a mock backend.
- [ ] Test installs, removals, and full upgrades in disposable Arch Linux environments.
- [ ] Cover dependency conflicts, locked databases, invalid signatures, interrupted downloads, and insufficient disk space.
- [ ] Verify helper access controls and ensure logs do not capture credentials or other sensitive data.
- [ ] Check responsiveness and memory use with a full repository catalog.

## 7. Package and release

- [ ] Add a README with build, development, installation, and troubleshooting instructions.
- [ ] Choose a license and add application icons, a desktop entry, and AppStream metadata.
- [ ] Create a PKGBUILD that installs the application, helper, and authorization policy with appropriate permissions.
- [ ] Test installation, upgrade, and removal of Pacstore itself on a clean system.
- [ ] Document transaction limitations, recovery steps, and the initial release's repository scope.
- [ ] Publish a first release after the core browse/install/remove/upgrade flows pass end-to-end checks.

## Later

- [ ] Explore opt-in AUR support with explicit build review, unprivileged builds, and a separate trust model.
- [ ] Consider update notifications, package history, cache management, and mirror diagnostics.
- [x] Add curated front-page application collections from configured core/extra repositories.
