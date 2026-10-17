# Read-only package backend

## CLI choice

Use `pacman` and `pacman-conf` for Part 3. `pacman-conf` resolves Includes,
repository order, variables, DBPath, and RootDir using pacman's own configuration
parser. `pacman -Si`, `-Qi`, and `-Qu` provide repository metadata, installed state,
and version-aware update availability. `-Qdtq` identifies currently unneeded dependency packages. `-Ql` maps installed metadata files to
package owners; `-Qlq -- <name>` retrieves installed files on demand. Arguments are
passed directly without a shell. All queries run with `LC_ALL=C` for stable
labels, with continuation lines preserved.

Compared with libalpm bindings, the CLI avoids native ABI/build coupling and works
with the installed pacman version. Its tradeoffs are subprocess cost, human-size
strings rather than exact byte counts, and dependence on C-locale field labels.
Parser tests and a live read-only check guard the current format. A future pacman
format change may require an adapter update. The separate Part 4 helper uses libalpm for real transaction resolution and
progress events; the catalog backend retains this read-only CLI adapter.

The backend never calls `-Sy`, downloads packages, refreshes databases, or acquires
root privileges. Update results reflect the existing database snapshot, including
pacman's version ordering; a differing version alone does not imply an update.
These are not full transaction plans, replacements, or remote freshness checks.

## Package identity and details

Keep repository/name identities separate. The first configured repository wins
when multiple repositories provide a name; alternate rows remain browsable but
do not duplicate Installed or Updates entries. Installed packages absent from all
configured repositories appear under `local`. Install reason is read from the
local record. Updates ignores display filters so libraries cannot be hidden.

Details expose architecture, versions, repository, description, license expression,
upstream HTTP(S) links, dependencies and optional dependency descriptions, human
readable download/installed sizes, and install reason. File lists load lazily and
reflect pacman's installed-file database, rather than verifying disk contents.

## Application metadata

Read XML and XML.gz catalogs from system `swcatalog/xml` and legacy `app-info/xmls`
locations, plus installed `metainfo`, `appdata`, and desktop-entry directories.
Honor the configured RootDir for those paths. Map AppStream `pkgname` values to
package identities. When pkgname is absent, use installed metadata-file or
launchable desktop-entry ownership. Do not guess ownership from application names.

Desktop application components and displayable desktop entries classify packages
as apps. Hidden/NoDisplay entries do not. AppStream names, summaries, descriptions,
categories, cached/stock/local icons, and screenshot sources enrich package data.
Base-language text is preferred, with English as fallback; broader localization
is future work. Multiple components may contribute categories/screenshots to one
package, with one display name/summary shown.

Look up local raster icons in AppStream icon caches, hicolor, and pixmaps, and
load PNG/JPEG artwork in visible package rows, featured cards, and details. Unsupported/missing artwork falls back to text.
Screenshot HTTP(S) links are available in details, with relative catalog sources
resolved against `media_baseurl`; no images are downloaded automatically. SVG
rendering, remote image previews, YAML catalogs, and additional user icon themes
are not implemented.

Malformed/unreadable metadata produces notices and does not discard package
records. Missing repository application catalogs produces an explicit notice:
installed desktop metadata provides partial application coverage, and **Show all
packages** exposes everything else.

## Cache and offline operation

The versioned JSON snapshot lives at `$XDG_CACHE_HOME/pacstore/catalog-v1.json`,
falling back to `$HOME/.cache/pacstore/`. Writes use a temporary file and atomic
rename. The snapshot includes metadata and warnings, but no credentials or full
configuration contents. Failed writes leave browsing usable and report a notice.

Before reuse, compare a fingerprint of resolved configuration, the pacman binary,
sync database file stats, local package/desc stats, and application metadata file
stats. External repository refreshes and successful package transactions change
these inputs and invalidate the snapshot on the next load. **Reload catalog**
forces queries and metadata rebuilding even when fingerprints match. Inputs are
checked again after loading; concurrent changes reject an inconsistent result.
Icon artwork is decoded afresh when opening details.

Local database browsing works without a network connection. Missing repositories
produce notices while readable repositories and installed packages remain usable.
If queries fail entirely, retain the last snapshot with an explicit stale-state
notice and unknown update availability. Unknown updates must not be presented as
an up-to-date system. Without any snapshot, show a recoverable error. Invalid or
unsupported cache files are ignored and rebuilt from local sources.

## References

- [pacman command reference](https://man.archlinux.org/man/pacman.8)
- [pacman-conf configuration parser](https://man.archlinux.org/man/pacman-conf.8)
- [AppStream specification source](https://github.com/ximion/appstream/tree/main/docs)

## Featured front page

Explore uses the explicit collections in `src/featured.rs`: browsers, utilities,
productivity, development, graphics/design, music/video, communication, and games.
Only named packages actually present in the loaded `core` or `extra` repositories
appear. Missing packages and empty sections are omitted. Package details, versions,
and installed state come from the catalog; a curated entry never creates a fake
package or changes its application classification.

These collections work without AppStream, independently of Search/Installed
filters and the all-packages preference. Local, custom-repository, and multilib
packages stay off the front page. Search still exposes the full catalog through
its filters and **Show all packages** option. To change the editorial selection,
edit the named packages and headings in `src/featured.rs`.

Installed desktop-entry paths are retained only for locally installed packages in
the host root. Launching rechecks package ownership, accepts only canonical system
application desktop entries, and delegates to gio in an unprivileged worker.
Display strings and launcher metadata are retained in the schema-2 snapshot;
older schema snapshots are rebuilt automatically.
