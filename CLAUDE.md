# Working in this repository

Several sessions work here in parallel, each in its own git worktree. A build
folder shared between branches makes cargo treat crates of another branch as
up to date: false compile errors (E0063 and the like) and the wrong xtask.exe.
So every worktree builds only into its own `target` folder.

## Worktrees

- Create a worktree only with `scripts/worktree.ps1 new <name> [-Base main] [-Branch <branch>]`.
  It makes `target/wt-<name>` (branch defaults to `feat/<name>`) and prints the
  `CARGO_TARGET_DIR` line for it. `scripts/worktree.ps1 env <name>` prints it again.
- Do all work for a task inside that worktree, not in the main checkout.

## Building and testing

- Build/test only with `CARGO_TARGET_DIR=<worktree>/target`, from inside the worktree
  (bash: `export CARGO_TARGET_DIR='C:/.../target/wt-<name>/target'`).
  Never point it at the shared `aurelius/target`, never build from the main checkout
  for a worktree's branch.
- Run xtask as `cargo run -p xtask -- …` (or `cargo xtask …`) from the worktree with
  that same `CARGO_TARGET_DIR`.
- Run builds/tests only once, at the end of a task (cargo build/check/test use too much
  CPU when several sessions run them). `scripts/test.ps1` builds into `target/work`
  of the folder it runs from, so run it from the worktree.
- Never run `cargo clean` in the main checkout or with `CARGO_TARGET_DIR` pointing at
  the shared target; never delete another session's `target/wt-*` folder.

## Build-cache

- On the Linux dev machine sccache is on for every cargo build through the user-level
  `~/.cargo/config.toml` (`rustc-wrapper`); the cache is `~/.cache/sccache` (100 GiB,
  set in `~/.config/sccache/config`). Each worktree keeps its own `target`; sccache
  shares compiled dependencies between them, so a fresh worktree does not rebuild them.
- Check that it works with `sccache --show-stats` (cache hits go up after a build).
- Do not set `RUSTC_WRAPPER`, `CARGO_INCREMENTAL` or a `rustc-wrapper` of your own,
  and do not run `sccache --zero-stats` or `--stop-server`: other sessions share the server.
- On a machine without sccache, do nothing: the repo does not need it.

## Cleaning up

- When the task is done: `scripts/worktree.ps1 remove <name>` (deletes the worktree's
  `target`, then the worktree, then `git worktree prune`; the branch stays).
  If the worktree must stay (e.g. someone still tests it), at least delete `<worktree>/target`.
