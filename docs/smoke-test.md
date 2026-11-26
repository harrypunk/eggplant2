# First-edition Linux smoke test

Run the automated checks first:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p tui
cargo clippy -p tui --all-targets -- -D warnings
```

The update/effect suite includes an explorer → open → edit → automatic save → switch → close → dirty quit workflow. The manual checks below cover the real terminal lifecycle, which `TestBackend` cannot verify.

## Setup

```sh
workdir="$(mktemp -d)"
printf 'first\n' > "$workdir/a.txt"
printf 'second\n' > "$workdir/b.txt"
mkdir "$workdir/nested"
printf 'nested\n' > "$workdir/nested/c.txt"
printf 'ignored.txt\n' > "$workdir/.gitignore"
printf 'ignored\n' > "$workdir/ignored.txt"
cargo run -p tui -- "$workdir"
```

## Interactive checks

1. **Explorer/open:** select `a.txt` with the arrows or `j`/`k`, then press Enter. Confirm that it opens and becomes selected in the explorer. Use `gg`/`G` to jump to the first/last visible entry and confirm that digits do not start a count. Select `nested` and use `l`/`h` to unfold/fold it, press `Shift-I` twice to show then hide `ignored.txt`, and press `r` to refresh. Select `nested/c.txt` and press `h`; its parent should collapse and become selected. On a collapsed child, `h` should fold the next visible parent, while `h` on a root-level file or collapsed root folder must do nothing. Press `?` to open the explorer keymap popup and `?` or Escape to close it without activating the tree underneath. Press `a`, confirm the selected folder path is prefilled, enter `added.txt`, and confirm that an empty file opens and becomes selected. Select a file and confirm Add prefills its parent directory. Use a trailing `/` to create a directory and confirm it is revealed without opening a buffer. Verify that absolute paths, `..`, existing targets, and symlink-parent escapes are rejected without changing files. Press `d` and Escape to confirm cancellation preserves the selected path. Use `d` then `y` on `added.txt`; the file must disappear and its clean open buffer must close. Confirm that a modified affected buffer blocks deletion without losing changes. Delete a test directory and confirm recursive removal, including closure of clean buffers for nested files. Use `m` to move `added.txt` to a new nested destination; the source buffer must close and the destination must be revealed without overwriting existing paths. Confirm directory moves close clean nested buffers and that modified source buffers block the move. Use `c` to copy the moved file and confirm both paths exist, the source buffer stays open even when modified, and the destination is revealed. Confirm directory copies are recursive and do not follow links. Confirm that input remains responsive, expansion changes never alter the workspace root, and selection survives each completed operation.
2. **Edit/auto-save/cursor:** press `i`, insert `A`, then press Escape. Confirm that `a.txt` now starts with `Afirst`. The terminal cursor should be a steady bar in Insert mode and a steady block after returning to Normal; Visual modes retain the block. Confirm the cursor's logical line has a themed background across its gutter and text, without changing syntax foregrounds, and Visual selection remains distinct. `Space u c` toggles this background. Open a long Unicode line and use `Space u w`: text wraps with syntax/selection styling and first-segment line numbers, every screen row of the cursor's logical line retains its background, and a second `Space u w` restores the original horizontal-scroll view.
3. **Switch synchronization:** open `b.txt`, then use `Space b p` and `Space b n`, followed by `[b` and `]b`. Confirm that the active tab and explorer selection stay synchronized in both directions. Confirm the bracket prefix appears as pending input and Escape/invalid input cancels without switching.
4. **Resize/status bar:** make the terminal narrow and short, then enlarge it. Confirm that content clips without a panic and that the cursor never appears outside the editor. Confirm the full-width status bar shows mode, the current Git branch (or abbreviated detached HEAD, with `--` outside a repository), a project-relative file path, and a right-aligned local `HH:MM` clock; the path should receive the remaining width.
5. **Popups:** open `Space Space` (or `Space f f`), `Space u t`, and the leader hints. Confirm that the file picker places its input cursor after the query, including after a long Unicode query, while non-input overlays hide the cursor. Verify Down/Up and Ctrl-N/Ctrl-P navigation, including first-to-last and last-to-first wrapping; the selected row must remain visible as the result window scrolls. Escape must close each popup.
6. **Theme:** use `Space u t`, Down/Up, and Enter to apply each built-in theme. Restart Eggplant and confirm the selected built-in theme is restored from `~/.eggplant/config.toml`. Type mixed-case text in the query field and confirm case-insensitive filtering and navigation limited to matching entries. Enter a query with no matches and confirm Apply is rejected without changing the active theme. Escape on a later visit must preserve the applied theme. When local Ghostty themes are installed, open the picker and confirm a `ghostty` section lists user files before system files without duplicate names. Apply one, restart Eggplant, and confirm the provider/path is restored while fallback syntax highlighting remains. Configure a valid Ghostty file with a different built-in fallback and confirm that startup resolves the Ghostty UI while retaining fallback syntax highlighting. Replace it with an invalid file and confirm that the previous theme remains active and a load error is reported.
7. **Visible-buffer jump:** open a buffer containing `hello`, then press `s` in Normal mode (or `Space s j`). Type `e` and confirm the character following each visible `e` is highlighted immediately—for example, the first `l` in `hello`—while other text dims and the typed `e` itself is not highlighted. Type `l`; the earlier hints must disappear and only jump labels remain. Enter the displayed label for `el` and confirm the cursor lands on `el`. Restart and confirm that Escape cancels without moving the cursor. Assign labels again, modify the file externally and let it reload; the next label input must cancel the stale session rather than jump using old positions.
8. **Current-file grep:** press `Space s c`, enter a smart-case regex such as `ell`, and confirm live results, normal buffer syntax colors in preview, yellow match blocks in results and preview, and a selected preview-line style. Select a result with Down/Up or Ctrl-N/Ctrl-P, verify wraparound, and press Enter; the editor must jump to that line. Reopen and confirm that Escape cancels without moving the cursor.
9. **Project grep:** press `Space s p`, enter a regex present in multiple workspace files, and confirm path/line results, yellow match blocks, and syntax-highlighted previews. Select a match with Down/Up or Ctrl-N/Ctrl-P, verify wraparound, and press Enter; the file must open at that line. Confirm that binary and ignored files do not appear.
10. **External synchronization:** while a clean buffer is open, modify its file with another process; the open buffer must reload automatically with valid cursor and syntax state. Create, delete, and rename workspace files and confirm that the explorer refreshes while preserving a still-present selection. Repeat with a locally modified buffer and confirm that local text is preserved with a conflict status.
11. **Word motions:** in a buffer containing multiple Unicode words, verify that `w` moves to the next word start, `b` returns to the previous start, and `e` moves to the word end. Confirm that repeated motions are safe at document boundaries and never change the text.
12. **Counted line jumps:** verify `gg` and `G` at document boundaries, then `3gg` and `3G` for the third line. While entering a count/prefix, confirm that the status bar shows it without hiding other messages; Escape and invalid input must clear it without unexpected movement.
13. **Viewport alignment:** with a scrollable buffer, verify `zz`, `zt`, and `zb` center/top/bottom-align the cursor line without changing the text or cursor position. Confirm that `z` appears in the status bar and that Escape/invalid input cancels safely, including in tiny terminals.
14. **Insert variants:** verify `a`, `A`, and `I` by typing after each command. Verify `o`/`O` create correctly indented lines and enter Insert on empty and populated documents; leaving Insert must auto-save, and one undo must remove the open-line plus typed content.
15. **Line boundaries:** verify `0` and `$` on blank, Unicode, and populated lines. Confirm that `0` after starting a count continues the count instead of moving the cursor, and that these motions never change text.
16. **Visual selection:** press `v` and extend the selection with character, word, line-boundary, and counted line-jump motions in both directions. Press `V` and verify whole-line selection, including line endings when yanking. Confirm visible selection styling and `VISUAL`/`V-LINE` status, then press Escape to return to Normal without changing text.
17. **Delete/change:** verify Normal `x`, `dd`, `dw`, `cc`, and `cw`, plus Visual `x`/`d`/`c`. Confirm `dw`/`cw` stop at the existing next-word-start boundary, including Unicode and empty/final-word cases. Confirm pending `d`/`c` status and Escape/invalid-key cleanup, immediate auto-save for deletes, save on Insert exit for changes, and one-step undo for each workflow.
18. **Yank/clipboard:** verify Visual `y` and Normal `yy`, including pending `y` status and Escape. Confirm that yanks do not change text, deletes/changes update the same register, and an available system clipboard contains the yanked text; clipboard-unavailable environments must preserve the internal register and show non-blocking feedback.
19. **Save safety:** close a clean buffer with `Space b c`. Make another buffer dirty and close it with the same shortcut; confirm that it saves first. With multiple tabs, verify `Space b o` keeps the active tab and `Space b r` closes only tabs to its right, saving dirty targets before closing. Verify `Space b a` saves and closes every buffer, including the active one, and leaves the empty editor. Finally, make a buffer dirty and quit with `Space q q`; confirm that the application waits for the write and exits with the edited content persisted.
20. **Transient notifications:** perform `yy` or Visual `y` and confirm a top-right `Yank` popup appears without moving focus and expires after four seconds. Trigger a series of notifications and confirm the newest appears at the top, older messages shift down with one-row gaps, and no more than eight are retained. Let individual timers expire and confirm only the matching message disappears while the others remain. Resize to a tiny terminal and confirm bounded stacking without a panic. Trigger a command rejection and Git stage/commit/switch feedback to verify `Rejected`, `Git`, and error styling.
21. **LSP navigation/diagnostics (optional Rust):** with `rust-analyzer` on `PATH`, open a small Cargo project and use `Space c d` on a function call, `Space c r` on a multiply referenced symbol, and `Space c i` on an implementation. Confirm one result opens directly and multiple results use Up/Down, Enter, and Escape in the shared location popup. Navigate into a dependency or standard-library file outside the workspace and confirm `[read-only]` appears; insert/delete/paste must not change it, while movement and yanking still work. Introduce an error and a warning, then use `Space c l` to inspect them in the non-modal diagnostics panel. Edit and close documents and confirm diagnostics update or clear without stale entries. For JS/TS or Python, install `typescript-language-server` or `pyright-langserver` and verify the community defaults with the correct TypeScript/React language IDs; copy `languages.example.toml` to `~/.eggplant/languages.toml` when customizing providers such as Basedpyright or Ruff. Remove the server from `PATH` and confirm a notification rather than a panic or loss of editing behavior.
22. **Agent workflows:**
   - **Inline quick prompt:** open a workspace Rust file, move to another line, and press `Ctrl-I`. Confirm the editable `relative/path:line` prefill, long-input wrapping, and visible cursor. Submit and confirm the fake response streams in the dialog and Agent panel. `Esc` hides without cancelling; `Ctrl-F7` interrupts; `fail` becomes session error data. Open the inline dialog in a project with no active buffer and submit a prompt successfully. Confirm that an external buffer supplies no prefill and does not prevent chatting; opening without a project still reports `Open a project to use the agent`.
   - **Project panel chat:** open a folder without opening a file and confirm Welcome is focused by default, with the Eggplant logo and command buttons visible. Use Up/Down or Ctrl-N/Ctrl-P and Enter to activate a button; the selected button changes with focus. Return to the welcome view, press `Space a t`, and confirm the focused panel has a double border and mode title. Start in Normal, use `i` to enter Insert, type `explain @main.rs`, and submit with Enter. Confirm project-file text works without an active buffer and assistant messages stay above the auto-height input. `Esc` returns to Normal without moving focus. In Normal, `?` opens a separate help popup; `?` or `Esc` closes it without activating the panel underneath. Use `Ctrl-H` and `Ctrl-L` to move between editor and panel.
   - **Provider login:**
     - Press `Space a p` and navigate with Up/Down or Ctrl-N/Ctrl-P. Confirm account lists `openai`, `qwen.com`, and `kimi.com` as placeholders; API key lists `openai`, `kimi-for-coding`, `qwen-tokenplan-individual-cn`, and `deepseek`.
     - For OpenAI, confirm the default endpoint, long-endpoint wrapping, masked key, and Tab looping between Endpoint and API key.
     - Paste a custom endpoint and key with Ctrl-V. Confirm multiline input is rejected and a slow clipboard result never moves to another field after Tab or editing.
     - With the default SQLite storage, save disposable test keys for two providers. Confirm keys never appear in notifications and both credentials plus the OpenAI endpoint persist. Saving a non-OpenAI provider does not change the active provider/model.
     - After an OpenAI save, confirm the endpoint's `/models` response appears in a bounded list and is cached in SQLite. Select an available model with Up/Down and Enter, verify activation and persisted selection, then restart and confirm the same model is restored.
     - Confirm catalog failures preserve the endpoint/key form for retry, and activation failures leave the model list available. Empty-key and missing-database errors preserve the draft; Escape steps back through the menus before closing.
     - In Agent panel Insert mode, type `/`, `/p`, and `/PRO` to filter `/provider`; `/unknown` should have no matches. Enter before selection does nothing.
     - Use Tab to quick-fill the first match, then Enter to open provider login without starting an agent run. Ctrl-N/Ctrl-P should quick-fill `/model`, `/thinking`, and `/logout`; `/model` opens the cached selector while `/thinking` and `/logout` show placeholder notifications.
     - Open `/model` and confirm cached rows remain navigable during refresh. Select an available model, verify activation and persisted selection, then restart and confirm the same model is restored. Simulate refresh failure and confirm cached rows remain with a warning.
     - Closing provider login returns to the same Insert input. Repeat `/provider` in the inline Agent dialog and confirm the same menu and prompt cursor remain available after closing.
   - **Real provider:** configure `provider = "openai"`, model/base URL, and `OPENAI_API_KEY`. Verify streaming and cancellation without putting the key in TOML, prompts, or logs. In both inline and panel chat, confirm the bottom footer shows `provider · model` and cumulative `↑input ↓output` after responses. With `context_window` set to the model's real capacity, confirm the latest-round percentage/capacity; without it, confirm an explicit unknown marker rather than a guessed percentage.
   - **Read/grep:** with a tool-capable model, read a file and search for a symbol. Confirm bounded calls, results, and follow-up history in the Agent panel.
   - **Approved writes:** confirm no edit occurs before approval. `Esc` preserves the request, `n` rejects without mutation, and `y` applies one undoable edit with explicit-ID save/language sync. A revision conflict preserves user text; `Ctrl-F7` cancels without applying or resuming. Success means an applied edit, not an unconditional save claim.
   - **Persistence:** confirm migration and completed-run storage at the default `~/.eggplant/ai/data.sqlite` location; set an absolute `session_db` to override it. Startup database errors are clear; later lock/write failures are bounded warnings that do not break editing.
23. **Terminal restoration after normal exit:** confirm that the shell prompt returns to the original screen, raw-mode input is disabled, and the cursor/keyboard behave normally.

## Git status checklist

Use a repository containing both staged and unstaged changes, then open the Git page with `Space g g`.

### Layout and navigation

- [ ] Common key hints and a one-line HEAD reference appear above the file sections.
- [ ] No Git log or editor status bar appears on the page.
- [ ] Staged files precede unstaged files in one natural selection order.
- [ ] Empty sections are hidden and consume no space.
- [ ] `j`/`k` moves selection using a bold `>` marker without a reversed background.
- [ ] Enter opens the selected file through the normal safe flow.
- [ ] With no changed files, the area below HEAD uses the dark theme background.

### Stage and unstage

- [ ] `s` on an unstaged file moves it to the staged section.
- [ ] `u` on a staged file returns it to the unstaged section without changing file contents.
- [ ] `Ctrl-s` stages every supported unstaged file and deletion. `U` unstages everything without changing worktree contents.
- [ ] `?` toggles a bordered bottom help panel; Escape dismisses only the panel, while `q` retains its normal page-close behavior.
- [ ] Repeated operations preserve a usable selection without a full-page loading flash.
- [ ] While a mutation is running, another stage/unstage command is rejected with `Git operation in progress`. After completion, the sections match `git status` and another operation can start.
- [ ] `x` on an unstaged tracked modification or deletion restores the index version, preserves any staged version of the path, refreshes status, and reloads a clean open buffer. Staged, untracked, conflicted, and renamed selections are rejected without mutation.
- [ ] `r`, then `s`, opens Soft reset with local `main`/`master`, existing remote-tracking refs, and five recent commits. Filter and select a ref with Down/Up or Ctrl-N/P, then repeat with a typed commit hash. Confirm reset moves HEAD/current branch without changing index/worktree bytes, refreshes HEAD/history/status/diffs, and makes the undone commit's changes ready to recommit. An invalid hash returns to the picker with an error.
- [ ] `c` opens the commit-message page when staged changes exist.

### Inline diffs

- [ ] `Tab` expands and collapses a diff beneath the selected file.
- [ ] Additions, deletions, and hunk headers use readable theme colors.
- [ ] Closing and reopening the toggle reuses the cached diff.

### Commit page

- [ ] The header summarizes only staged files; the scratch editor supports multiline messages, Normal/Insert modes, undo/redo, and `x`/`dd`/`dw` deletion and `cc`/`cw` change without auto-saving a message file or changing the main editor register. `yy` and `p`/`P` share the register intentionally across editor and commit pages. When a system clipboard provider exists, `p`/`P` also read external clipboard text in the background and paste it only into the unchanged target; local-register paste remains available without a provider.
- [ ] `q` in Normal mode closes and preserves the draft; reopening with `c` restores it. `Ctrl-c` explicitly discards it.
- [ ] Empty messages and empty staging do not start repository work.
- [ ] `Ctrl-s` saves modified file-backed buffers, waits for saves, and commits exactly the staged content without including unstaged or untracked files.
- [ ] Backend or save failures preserve the draft. Success clears it, returns to status, and refreshes HEAD, changes, history, and diff indicators.
- [ ] While commit saving or Git work is active, close/quit is rejected with `Commit in progress; try again after it finishes`. While close/quit is pending, commit submission is rejected without changing the draft or deferred action.

### Branch switching and creation

- [ ] `b` opens the bottom Branch action bar without overloading page-local commit/stage keys. `s` lists existing local branches except the current one; filtering and Up/Down selection work, Enter switches, and Escape cancels before checkout.
- [ ] Switch between divergent branches without moving either branch tip. Clean added/modified/deleted files follow the destination tree, while compatible staged, unstaged, and untracked changes remain intact. A conflicting local or untracked path rejects the switch with an error and leaves HEAD/worktree content unchanged.
- [ ] `b`, then `c`, shows HEAD first followed by all local branches. Selecting HEAD or a branch advances to the new-name field. Typing a short or full commit hash also advances with that exact start point; invalid revisions and invalid/duplicate branch names report an error without changing HEAD.
- [ ] Successful switches/creations close the popup, update HEAD/history/status/explorer/diff indicators, and reload eligible active and inactive buffers. Dirty buffers are preserved. Escape cannot cancel an in-flight mutation.

### Fast-forward merge

- [ ] `m` opens a picker with existing local target branches. Typing filters, Up/Down selects, Enter switches to the target and fast-forwards it to the source branch, and Escape cancels. Backend errors remain in the popup.
- [ ] Pending Git changes or unsaved/saving buffers prevent opening or submitting a merge; no files are saved automatically. Dirty, untracked, conflicted, detached, unborn, divergent, and locked repositories fail without changing HEAD or worktree contents.
- [ ] A successful switch leaves HEAD on the target, advances that target when required, and updates index, added/modified/deleted worktree files, history, and diff indicators. All clean open buffers reload after every switch, including already-up-to-date targets and inactive buffers; dirty buffers are preserved.

### History page

- [ ] `l` opens a separate history page without embedding a log in Git status.
- [ ] Commit rows show short ID, subject, author, relative time, and labels for local/remote refs pointing exactly at that commit, all flowing left-to-right without wide right alignment. The Git status HEAD reference uses the same commit-message component and also shows relative time.
- [ ] `j`/`k` selection wraps and keeps the selected commit visible while scrolling.
- [ ] `gg`/`G` jumps to the first/last commit or detail file; digits do not start a count.
- [ ] History loads at most 20 commits per page. When available, a bold accent-colored `n load more` appears in the header; `n` appends the next page without moving selection. Loading/retry feedback appears there and disappears at the end.
- [ ] `Tab` toggles a commit's changed-file list without opening a diff.
- [ ] Enter opens a right-side detail with full hash, complete message, author, full date/time/offset, and changed files. Enter on a selected file row closes history and opens the working-tree file in the editor with correct focus; deleted entries remain in history and show a rejection notification.
- [ ] In a ready detail, `c` copies the seven-character hash and `C` copies the full hash to the system clipboard without changing the unnamed yank register. A transient `Git` notification reports the copied value without duplicating it in the history header; unavailable clipboards produce a visible error. List/loading/failed states do not start copy work.
- [ ] While an uncached detail is loading, open a previously cached commit. The older completion must not replace the visible cached detail or pollute its cache entry.
- [ ] `Tab` on a selected detail file toggles its diff and reuses the cached result on reopening.
- [ ] Long messages and expanded diffs keep the selected detail file visible in the right-side viewport.
- [ ] A commit touching multiple files lists every changed path without raw-object or encoding garbage.
- [ ] `q` or Escape returns to Git status rather than losing that page's context.

### Editor gutter and repository states

- [ ] Added, modified, and deleted lines show a colored vertical `│` in the editor gutter against HEAD.
- [ ] Untracked files use an empty base and show additions appropriately.
- [ ] Non-repositories remain unmarked and show a concise status message on the Git page.
- [ ] Committing outside the editor updates the branch/status state and resets active-buffer indicators to the new HEAD.
- [ ] `q` or Escape returns to the previous editor/explorer context.

## Failure and panic review

- For a save failure, make a file readonly before editing and quitting. The application must remain open with the dirty buffer and an error instead of exiting or overwriting the file.
- For an external modification, edit the same file with another process before Eggplant saves. Eggplant must report the conflict without overwriting either version.
- Review `crates/tui/src/terminal.rs`: initialization cleans up partial failures, normal restoration attempts cursor-shape reset, alternate-screen exit, and raw-mode disable even if an earlier operation fails, and the panic hook ignores cleanup errors before delegating to the original hook. A panic during cleanup must not cause a second panic.

If a development build ever panics inside a real terminal, the expected result is the same shell/terminal restoration as normal exit. Report any reproduction that leaves the terminal in raw mode or the alternate screen.

## Cleanup

```sh
rm -rf "$workdir"
```
