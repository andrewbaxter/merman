# merman3

**merman2 (in `merman2/`) is the model design.** merman3 is a port of it, not a
redesign. Before changing anything structural, read how merman2 does it and
follow that; if a deviation seems necessary, ask first rather than inventing an
alternative.

The part that matters most and is easiest to get wrong: merman2 is retained and
incremental. Every brick owns a display node for its whole life
(`BrickText.text = context.display.text()`), courses own groups that bricks are
spliced into (`Course.visual.add(...)`), and nothing is ever recollected or
redrawn wholesale - things move by `setConverse`, `setBaselineTransverse`,
`setTransverse` and `setText`. There is no snapshot, no per-frame walk of the
document, and no rebuilding of what's on screen. `core/src/display.rs` is the
port of merman2's `core/display` package and exists to keep it that way.

What is deliberately *not* aligned with merman2: merman2's `Display` holds
listeners that call back into `Context`, but here `Context` owns the `Display`,
so events go the other way - the host calls `context_resize`, `context_scrolled`,
`mouse_moved` and `key_resolve`. Everything else follows merman2.

Measure with the same thing you render with, as merman2 does - its JavaFX text
metrics come from `javafx.scene.text.Text`, the same node type it draws with, so
`DisplayWeb` measures with a hidden svg `<text>` rather than a canvas.

`core/src/display.rs` is the port of merman2's `core/display` package,
`core/src/stylist.rs` of its `Stylist`, `core/src/environment.rs` of its
`Environment`, and
`window`/`ellipsize_threshold` on the context is the port of merman2's document
windowing (`Context.windowAdjustMinimalTo`): out-of-window nested atoms render
as an ellipsis symbol instead of their body. Both are off by default, matching
merman2's `ellipsizeThreshold = MAX_VALUE` and `startWindowed = false`.

An AST viewer and editor. `core` is the platform independent engine (syntax,
matching, layout, cursors, input), `api` is the editor's client-server protocol,
`web` draws the panels in the browser as wasm, and `cli` serves the pages and
answers the api, in a webview window or a browser.

There are two entrypoints in `web`, each with its own page in `cli/static`:

- `demo` (`demo.html`) is the code panel on its own, showing a file baked into
  the page. It's a demo of the widget and is always read only.
- `editor` (`editor.html`) is the editor proper: a filesystem panel, and beside
  it a code panel once a file is opened. It's what the webview window shows, and
  it reads directories and files through the api.

The panels live in `web/src/panels`. The code panel lays its document out with
the engine and draws it as svg; the filesystem panel is plain dom, because a
directory listing has no syntax and there's nothing for the engine to do with
it. Both take the same actions from the same keymap, and hand back the ones they
couldn't use so the editor can move focus on them.

The api is defined with `glove` in `api`, but it's served over the same http
server the pages come from rather than glove's unix socket. The server only
listens on loopback, but it will read any file the process can - it's the
editor's filesystem access, not a sandbox.

## Linting

After making changes, stage them and run the linter, and fix everything it
reports before considering the work done:

```
git add ...
cd "$(mktemp -d)" && nix-shell /path/to/merman3/default.nix --run 'cd /path/to/merman3 && rust-ai-lint'
```

`rust-ai-lint` is on the PATH already; the shell is only there for the C
libraries the `cli` crate links, without which nothing compiles and the linter
refuses to check. Start the shell from a temp directory, because its setup hooks
drop empty `postBuildInstallFromCargoBuildLogOutTemp*` directories in the
working directory. The linter also runs as a pre-commit hook
(`hooks/pre-commit`), so a commit fails until it passes. None of its checks can
be turned off, and silencing a warning (`#[allow(...)]`) counts as a failure -
change the code instead.

Staging first matters: the comment check only looks at what is staged, and it
deletes comments that weren't in the committed version. Unstaged work passes it
silently and then loses those comments later. Another rule that comes up often:
a function or constant used exactly once has to be inlined into its caller.

## Building and testing

- `cargo test -p merman3_core` runs the engine tests; they use a fixed-width
  fake measurer so layout is exact.
- `cargo check -p merman3_web --target wasm32-unknown-unknown` checks the
  browser side.
- `cli` links gtk and webkit, so it only builds inside
  `nix-shell default.nix --run '...'`.
- `nix-build` builds the cli with the wasm viewer embedded. Anything the build
  reads has to be listed in `stagedSources` in `default.nix`, including files in
  `cli/static`.

## Conventions

Match the surrounding code: explicit `return` at the end of every function,
functions named after the thing they act on (`cursor_*`, `visual_*`, `wall_*`,
`keymap_*`, `code_*`, `filesystem_*`, `editor_*`), and config types named
`Spec*` with serde `deny_unknown_fields`.

`syntaxes/` holds syntaxes that files are opened with, keyed by extension in
`merman.json`. Nothing that isn't a file format belongs there.
