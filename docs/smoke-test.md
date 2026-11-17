# First-edition Linux smoke test

Run the automated checks first:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The update/effect suite includes an explorer → open → edit → automatic save → switch → close → dirty quit workflow. The manual checks below cover the real terminal lifecycle, which `TestBackend` cannot verify.

## Setup

```sh
workdir="$(mktemp -d)"
printf 'first\n' > "$workdir/a.txt"
printf 'second\n' > "$workdir/b.txt"
cargo run -p tui -- "$workdir"
```

## Interactive checks

1. **Explorer/open:** select `a.txt` with the arrows or `j`/`k`, then press Enter. Confirm that it opens and becomes selected in the explorer. Use `gg`/`G` to jump to the first/last visible entry and confirm that digits do not start a count.
2. **Edit/auto-save/cursor:** press `i`, insert `A`, then press Escape. Confirm that `a.txt` now starts with `Afirst`. The terminal cursor should be a steady bar in Insert mode and a steady block after returning to Normal; Visual modes retain the block.
3. **Switch synchronization:** open `b.txt`, then use `Space b p` and `Space b n`, followed by `[b` and `]b`. Confirm that the active tab and explorer selection stay synchronized in both directions. Confirm the bracket prefix appears as pending input and Escape/invalid input cancels without switching.
4. **Resize/status bar:** make the terminal narrow and short, then enlarge it. Confirm that content clips without a panic and that the cursor never appears outside the editor. Confirm the full-width status bar shows mode, the current Git branch (or abbreviated detached HEAD, with `--` outside a repository), a project-relative file path, and a right-aligned local `HH:MM` clock; the path should receive the remaining width.
5. **Popups:** open `Space Space` (or `Space f f`), `Space u t`, and the leader hints. Confirm that the file picker places its input cursor after the query, including after a long Unicode query, while non-input overlays hide the cursor. Verify Down/Up and Ctrl-N/Ctrl-P navigation, including first-to-last and last-to-first wrapping; the selected row must remain visible as the result window scrolls. Escape must close each popup.
6. **Theme:** use `Space u t`, Down/Up, and Enter to apply each built-in theme. Escape on a later visit must preserve the applied theme.
7. **Visible-buffer jump:** open a buffer containing `hello`, then press `Space c j`, type `el`, and enter the displayed label for `e`. Confirm that only visible matches receive labels, other text dims, and the cursor lands on `e`. Restart and confirm that Escape cancels without moving the cursor. Assign labels again, modify the file externally and let it reload; the next label input must cancel the stale session rather than jump using old positions.
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
20. **Terminal restoration after normal exit:** confirm that the shell prompt returns to the original screen, raw-mode input is disabled, and the cursor/keyboard behave normally.

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
- [ ] `x` shows a placeholder notice only and does not mutate the repository.
- [ ] `c` opens the commit-message page when staged changes exist.

### Inline diffs

- [ ] `Tab` expands and collapses a diff beneath the selected file.
- [ ] Additions, deletions, and hunk headers use readable theme colors.
- [ ] Closing and reopening the toggle reuses the cached diff.

### Commit page

- [ ] The header summarizes only staged files; the scratch editor supports multiline messages, Normal/Insert modes, undo/redo, and `x`/`dd`/`dw` deletion without auto-saving a message file or changing the main editor register. `yy` and `p`/`P` share the register intentionally across editor and commit pages. When a system clipboard provider exists, `p`/`P` also read external clipboard text in the background and paste it only into the unchanged target; local-register paste remains available without a provider.
- [ ] `q` in Normal mode closes and preserves the draft; reopening with `c` restores it. `Ctrl-c` explicitly discards it.
- [ ] Empty messages and empty staging do not start repository work.
- [ ] `Ctrl-s` saves modified file-backed buffers, waits for saves, and commits exactly the staged content without including unstaged or untracked files.
- [ ] Backend or save failures preserve the draft. Success clears it, returns to status, and refreshes HEAD, changes, history, and diff indicators.

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
- [ ] Enter opens a right-side detail with full hash, complete message, author, full date/time/offset, and changed files.
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
