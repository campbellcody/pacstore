# Pacstore

A Rust graphical app store prototype for pacman-based Linux systems, built with
[eframe/egui](https://docs.rs/eframe/0.37.0/eframe/) and its wgpu renderer.

The default catalog reads your configured pacman repositories and installed
packages. Explore features curated apps available in `core` and `extra`, grouped
into Browsers, Utilities, Productivity, Development, Graphics & design, Music &
video, Communication, and Games. Collections work even when AppStream metadata is
missing. Search supports repository, category, and installation filters; Installed
separates manual packages, dependencies, all installed packages, and orphans.
Package details show metadata, dependencies, files, and launch actions for installed
applications with desktop entries. System package changes use a separate polkit-authorized libalpm helper, with
prepared plans and explicit confirmation. Installations include a full system
upgrade. Reload catalog only rereads existing local databases; transaction
preparation performs the required repository refresh. See
[transaction setup and behavior](docs/transactions.md) to build and install the
helper. The GUI runs as your normal user.

Use `cargo run -- --demo` for the isolated sample catalog and demo confirmations.
Demo changes disappear on exit. The all-packages, theme, and display-scale
preferences persist. Installed offers reviewed orphan cleanup, recomputed by the
helper so manually installed packages are preserved.

## Development

Use Rust 1.95 or later, a Linux desktop session (Wayland or X11), and a working
wgpu-compatible graphics driver. Both window-system backends are enabled. The blue–cyan palette follows the system
light/dark theme by default; use the Theme selector to choose System, Light, or Dark. Install `fontconfig` to use its configured system sans-serif and
monospace fonts; bundled fonts provide a fallback.
Building requires a C toolchain and pkg-config; install development headers for
Wayland/X11 and xkbcommon if the build reports missing system libraries.

```sh
cargo run
```

System browsing requires `pacman` and `pacman-conf`. Optional AppStream XML
catalogs enrich repository applications; installed desktop entries and metainfo
provide additional application metadata. Without catalog metadata, use **Show all
packages** to find packages that cannot yet be classified as desktop apps.

```sh
# Query the catalog without a graphical session
cargo run -- --check-catalog
# Rebuild the snapshot from local databases
cargo run -- --check-catalog --reload
```

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

See [the first-release design](docs/first-release.md) for supported-system targets,
user flows, screen sketches, and the toolkit decision. [TODO.md](TODO.md) tracks
the remaining implementation.

## Configuration and logs

Preferences are stored in `$XDG_CONFIG_HOME/pacstore/settings.conf`, falling back
to `$HOME/.config/pacstore/settings.conf` when XDG_CONFIG_HOME is unset or relative.
The file is created when a preference changes:

```ini
version=1
all_packages=false
theme=system
scale=100%
```

Missing files use defaults. Invalid or unsupported files produce an error in the
app and are preserved; correct or move the file before saving preferences again.
Writes run in a worker and replace the file atomically. Searches are not saved. A separate read-only catalog snapshot is cached under
`$XDG_CACHE_HOME/pacstore/catalog-v1.json` (or `$HOME/.cache/pacstore/`). It is
validated against configuration, database, and metadata file changes before
reuse. If package queries fail, the previous snapshot remains browsable with a
stale-state notice; its updates are not presented as current.

Application logs go to stderr. Set `PACSTORE_LOG` to `off`, `error`, `warn`, `info`,
`debug`, or `trace` (default: `info`). Logs cover startup, catalog loading, demo
transaction completion, and errors; no credentials are requested or recorded.

[Architecture](docs/architecture.md) describes worker and helper boundaries.
GitHub Actions runs formatting, Clippy, tests, and a build on pushes and pull
requests using Rust 1.95.0. GUI tests render egui headlessly. Native helper tests
run separately in an Arch container with disposable package databases.

[Package backend](docs/package-backend.md) documents the CLI choice, cache
invalidation, metadata sources, and current limitations. Screenshots are exposed
as links from local AppStream metadata; following them requires network access.
Local PNG/JPEG icons load lazily in featured cards, package lists, and details;
unavailable or unsupported artwork falls back to a letter placeholder.

[Store experience](docs/store-experience.md) documents launch behavior, orphan
cleanup, keyboard controls, native accessibility, display scaling, and translation
preparation. Rebuild and reinstall the helper with the application when updating;
orphan cleanup uses protocol version 2. No additional translated language ships
yet; UI messages are centralized in `locales/en.json`.

```sh
python3 scripts/update-translations.py --check
```
