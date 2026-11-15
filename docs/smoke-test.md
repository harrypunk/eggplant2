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

1. **Explorer/open:** select `a.txt` with the arrows or `j`/`k`, then press Enter. Confirm that it opens and becomes selected in the explorer.
2. **Edit/auto-save:** press `i`, insert `A`, then press Escape. Confirm that `a.txt` now starts with `Afirst`.
3. **Switch synchronization:** open `b.txt`, then use `Space b p` and `Space b n`. Confirm that the active tab and explorer selection stay synchronized in both directions.
4. **Resize:** make the terminal narrow and short, then enlarge it. Confirm that content clips without a panic and that the cursor never appears outside the editor.
5. **Popups:** open `Space Space` (or `Space f f`), `Space u t`, and the leader hints. Confirm that the file picker places its input cursor after the query, including after a long Unicode query, while non-input overlays hide the cursor. Escape must close each popup.
6. **Theme:** use `Space u t`, Down/Up, and Enter to apply each built-in theme. Escape on a later visit must preserve the applied theme.
7. **Visible-buffer jump:** open a buffer containing `hello`, then press `Space c j`, type `el`, and enter the displayed label for `e`. Confirm that only visible matches receive labels, other text dims, and the cursor lands on `e`. Restart and confirm that Escape cancels without moving the cursor.
8. **Current-file grep:** press `Space s c`, enter a smart-case regex such as `ell`, and confirm live results, normal buffer syntax colors in preview, yellow match blocks in results and preview, and a selected preview-line style. Select a result with Down/Up and press Enter; the editor must jump to that line. Reopen and confirm that Escape cancels without moving the cursor.
9. **Project grep:** press `Space s p`, enter a regex present in multiple workspace files, and confirm path/line results, yellow match blocks, and syntax-highlighted previews. Select a match and press Enter; the file must open at that line. Confirm that binary and ignored files do not appear.
10. **External synchronization:** while a clean buffer is open, modify its file with another process; the open buffer must reload automatically with valid cursor and syntax state. Create, delete, and rename workspace files and confirm that the explorer refreshes while preserving a still-present selection. Repeat with a locally modified buffer and confirm that local text is preserved with a conflict status.
11. **Word motions:** in a buffer containing multiple Unicode words, verify that `w` moves to the next word start, `b` returns to the previous start, and `e` moves to the word end. Confirm that repeated motions are safe at document boundaries and never change the text.
12. **Counted line jumps:** verify `gg` and `G` at document boundaries, then `3gg` and `3G` for the third line. While entering a count/prefix, confirm that the status bar shows it without hiding other messages; Escape and invalid input must clear it without unexpected movement.
13. **Viewport alignment:** with a scrollable buffer, verify `zz`, `zt`, and `zb` center/top/bottom-align the cursor line without changing the text or cursor position. Confirm that `z` appears in the status bar and that Escape/invalid input cancels safely, including in tiny terminals.
14. **Insert variants:** verify `a`, `A`, and `I` by typing after each command. Verify `o`/`O` create correctly indented lines and enter Insert on empty and populated documents; leaving Insert must auto-save, and one undo must remove the open-line plus typed content.
15. **Line boundaries:** verify `0` and `$` on blank, Unicode, and populated lines. Confirm that `0` after starting a count continues the count instead of moving the cursor, and that these motions never change text.
16. **Visual selection:** press `v` and extend the selection with character, word, line-boundary, and counted line-jump motions in both directions. Press `V` and verify whole-line selection, including line endings when yanking. Confirm visible selection styling and `VISUAL`/`V-LINE` status, then press Escape to return to Normal without changing text.
17. **Delete/change:** verify Normal `x`, `dd`, and `cc`, plus Visual `x`/`d`/`c`. Confirm pending `d`/`c` status and Escape/invalid-key cleanup, immediate auto-save for deletes, save on Insert exit for changes, and one-step undo for each workflow.
18. **Yank/clipboard:** verify Visual `y` and Normal `yy`, including pending `y` status and Escape. Confirm that yanks do not change text, deletes/changes update the same register, and an available system clipboard contains the yanked text; clipboard-unavailable environments must preserve the internal register and show non-blocking feedback.
19. **Save safety:** close a clean buffer with `Space b c`. Make another buffer dirty and close it with the same shortcut; confirm that it saves first. With multiple tabs, verify `Space b o` keeps the active tab and `Space b r` closes only tabs to its right, saving dirty targets before closing. Verify `Space b a` saves and closes every buffer, including the active one, and leaves the empty editor. Finally, make a buffer dirty and quit with `Space q q`; confirm that the application waits for the write and exits with the edited content persisted.
20. **Terminal restoration after normal exit:** confirm that the shell prompt returns to the original screen, raw-mode input is disabled, and the cursor/keyboard behave normally.

## Failure and panic review

- For a save failure, make a file readonly before editing and quitting. The application must remain open with the dirty buffer and an error instead of exiting or overwriting the file.
- For an external modification, edit the same file with another process before Eggplant saves. Eggplant must report the conflict without overwriting either version.
- Review `crates/tui/src/terminal.rs`: initialization cleans up partial failures, normal restoration attempts both alternate-screen exit and raw-mode disable even if the first operation fails, and the panic hook ignores cleanup errors before delegating to the original hook. A panic during cleanup must not cause a second panic.

If a development build ever panics inside a real terminal, the expected result is the same shell/terminal restoration as normal exit. Report any reproduction that leaves the terminal in raw mode or the alternate screen.

## Cleanup

```sh
rm -rf "$workdir"
```
