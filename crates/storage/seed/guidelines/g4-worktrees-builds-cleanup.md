The repository's CLAUDE.md says how to make a worktree and how to build in it; follow it. On top of that:
- One worktree and one branch per task (`feat/<task>` or `fix/<task>`), from the base your task names. Never work in the main checkout.
- While you work, run only targeted tests (`cargo test -p <crate> <filter>`). Run the full suite once at the end, in the background (`nohup … > target/<task-id>/suite.log 2>&1 &`), and poll it: it takes longer than 10 minutes.
- Commit every finished sub-fix with its tests right away, so an interruption loses nothing. Never merge or push without the Owner's approval.
- Commit messages are English: what changed and why, ending with the task id, e.g. "vault: fail closed when the redactor cannot be filled (AUR-MU-04-F6)".
- Never touch another session's worktree or target folder, `~/.claude`, the system keychain, the installed daemon, or the live data folder (`~/.local/share/Aurelius`, read only).
- When the task is done: reports, logs and screenshots go to `target/<task-id>/`; remove review and QA worktrees (only with a clean `git status`) and run `git worktree prune`; delete your own build folder; put `du -sh target` in your report.
