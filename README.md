# Fika

Fika is a lightweight Rust + Slint file manager prototype aimed at modern
Wayland desktops. The current implementation focuses on a small, usable core:

- browse local directories
- jump to home, parent, filesystem root, and directory history
- refresh the current directory, including debounced directory monitoring
- switch between light and dark UI modes
- resize the window with guarded minimum dimensions to avoid content overflow
- open files through the desktop default application without shelling out to
  `xdg-open`
- run file operations asynchronously, including copy, move, link, trash,
  rename, conflict handling, and one-step undo
- run as a lightweight chooser with `--chooser`, and as an experimental
  `xdg-desktop-portal` FileChooser backend

```sh
cargo run
cargo run -- --chooser ~/Downloads
```

Packaged desktop integration installs D-Bus, Polkit, and portal metadata in
addition to the binaries. The metadata can be staged and checked without root:

```sh
DESTDIR=/tmp/fika-root PREFIX=/usr BINDIR=/usr/lib/fika scripts/install-data.sh
DESTDIR=/tmp/fika-root PREFIX=/usr BINDIR=/usr/lib/fika \
  scripts/check-runtime-integration.sh --metadata-only
```

After a real install, run `scripts/check-runtime-integration.sh` to verify the
installed system-bus helper, Polkit action, and portal backend metadata. Add
`--activate-system-helper` to confirm D-Bus activation of the privileged helper
without invoking a privileged file-operation method.

The UI is defined in `ui/app.slint` and compiled from `build.rs` with
`slint-build`. Both `slint` and `slint-build` are pinned to `1.16.1`.
Default application launching is implemented locally: Fika guesses the MIME
type, reads XDG `mimeapps.list` files, resolves the matching `.desktop` file,
and expands its `Exec=` command.

In chooser mode, selecting an item and pressing `Choose` prints the selected
path to stdout and exits. The `fika-xdp-filechooser` binary exposes the
experimental `org.freedesktop.impl.portal.FileChooser` backend and launches
`fika --chooser` as its UI frontend.

## Architecture Notes

The GUI process is intentionally non-privileged. Protected operations go
through a constrained D-Bus helper on the system bus; the helper performs
per-method Polkit checks before running fixed file operations or protected
external-editor writeback.

Detailed planning lives in:

- `docs/DESIGN.md` for architecture and subsystem design
- `docs/TODO.md` for the implementation roadmap and acceptance criteria
