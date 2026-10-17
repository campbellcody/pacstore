# Package transactions

## Build and install

Build on Arch Linux with Rust 1.95+, pacman/libalpm 16, pkg-config, and polkit.
The GUI can be built without libalpm; the installed helper is a separate optional
binary. A working polkit agent must be registered with your desktop session.

```sh
# Build as your normal user.
make build
# Install the already-built binaries and policy as administrator.
sudo make install
# Run the GUI as your normal user.
pacstore
```

`make install DESTDIR=/path/to/staging` supports packaging without changing the
running system. The files are `/usr/bin/pacstore`,
`/usr/lib/pacstore/pacstore-helper`, and
`/usr/share/polkit-1/actions/org.pacstore.transactions.policy`. The helper and its
parent directories must be owned by root and not writable by other users. There
is no setuid binary. Missing or unsafe helper installations produce a GUI error;
they never fall back to running the entire GUI as root.

## Review and execute

1. Choose installation, removal, or a full system upgrade. For installation,
   optionally select additional dependencies; for removal, explicitly choose
   whether packages depending on the target may also be removed.
2. Authenticate through your desktop's polkit agent. Authorization starts a
   narrowly scoped helper, which loads the root-owned `/etc/pacman.conf` using
   `/usr/bin/pacman-conf` and independently validates the structured request.
3. For installations and upgrades, the helper refreshes repositories and includes
   a full system upgrade. It resolves dependencies, providers, replacements, and
   conflicts, asking for decisions where needed. Unresolvable dependencies cannot
   be skipped. Removal uses the existing local database without refreshing it.
4. Review the prepared additions/version changes/removals, download bytes, net
   disk-space change, free space, and recorded conflict/replacement decisions.
   Pacman's transaction lock remains held while this exact plan awaits approval.
5. Confirm package changes to commit, or cancel preparation to release the lock.
   Downloads, verification, hooks, and file changes report progress and logs.
   Signing-key questions remain explicit and use libalpm's configured verification.
6. After the helper exits, the application rebuilds the catalog even on failure,
   since a failed commit may have changed some packages. Installed and Updates
   views use the refreshed state.

The GUI offers no individual-package update operation. Installation also includes
system updates, preventing a refresh followed by a partial upgrade. Repository-
qualified installation targets preserve the selected repository. IgnorePkg,
IgnoreGroup, repository usage, signatures, cache paths, hooks, and download sandbox
settings come from system configuration. Space checking is always enabled.
HoldPkg removals are refused and directed to pacman for manual review. Custom
XferCommand configurations are currently refused; use pacman for those systems.

## Orphan cleanup

Installed offers a separate **Review orphan cleanup** action. The GUI shows
`pacman -Qdtq` results, but the helper independently recomputes the orphan set under
the database lock. It selects only packages installed as dependencies with no
required or optional reverse dependencies. Explicitly installed packages are never
selected. No repositories are refreshed, and no recursive/cascade flags are used.
The resulting plan is reviewed and confirmed through the same transaction dialog.
New orphans created by removal can be reviewed in a subsequent cleanup.

Rebuild and reinstall both binaries when updating. Protocol version 2 adds the
orphan operation; an older helper rejects the new version before changing packages.

## Cancellation, closure, and failures

Cancellation is available before commit. During repository refresh or preparation,
the request is honored at the next question or final-plan boundary. If an
authorization dialog remains open, dismiss it too. Refreshing databases and
explicitly importing a key can leave metadata changes after cancellation, but no
package changes start without final confirmation.

Once commit starts, cancellation is disabled. Normal window closure waits for
completion, keeping questions answerable. Before commit, closing requests
cancellation. If the GUI crashes or its pipes disappear, the helper finishes an
already-started commit rather than being killed; unattended questions default to
decline. Before confirmation, EOF cancels. The helper owns and releases its
libalpm transaction and never removes another process's lock file.

The GUI retains the latest 500 log lines with a Copy log action. Progress events
may be coalesced/dropped when the GUI cannot keep up; transaction plans, questions,
and completion messages are retained. Existing pacman logging remains configured
through the system LogFile. No passwords enter the protocol or application logs.

Authorization cancellation and failure are distinguished. Network/mirror refresh,
keyring/signature, dependency/conflict, disk-space, and helper failures are shown
with logs. On failure, review the transaction log and `/var/log/pacman.log` (or the
configured LogFile), address the specific cause, and reload before retrying. Do
not delete a database lock merely because an operation failed; first establish
that no package manager is running. No automatic rollback or force-overwrite is
attempted.

## Protocol and verification

The protocol is versioned JSON-lines over inherited pipes with bounded frames,
a typed operation allowlist, name validation, and no shell, user-selected config,
arbitrary command, or signature-bypass option. Polkit authorizes the fixed helper
path with `auth_admin` for active desktop sessions; inactive/other sessions are
denied by its policy. The helper also requires root and PKEXEC_UID.

```sh
cargo test --locked --features system-helper --all-targets
cargo clippy --locked --features system-helper --all-targets -- -D warnings
```

Tests use disposable roots and tiny packages, including a file:// repository for
full-upgrade resolution, plus isolated subprocesses for GUI/protocol behavior.
Signature verification is disabled only for generated unsigned fixture packages;
a separate test verifies that requiring signatures rejects such a package. No
host packages or host pacman databases are changed. CI runs native helper tests
inside an Arch Linux container in addition to the GUI checks on Ubuntu.

Live desktop authentication, driver rendering, real remote mirror failures, and
large production upgrades still require acceptance testing. This implementation
has not been installed or used to transact against the development host.

References: [libalpm bindings](https://docs.rs/alpm/5.0.2/alpm/),
[configuration adapter](https://docs.rs/alpm-utils/5.0.0/alpm_utils/),
[pkexec authorization](https://polkit.pages.freedesktop.org/polkit/pkexec.1.html).
