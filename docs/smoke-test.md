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
5. **Popups:** open `Space f f`, `Space u t`, and the leader hints. Confirm that the editor cursor is hidden and that Escape closes each popup.
6. **Theme:** use `Space u t`, Down/Up, and Enter to apply each built-in theme. Escape on a later visit must preserve the applied theme.
7. **Visible-buffer jump:** open a buffer containing `hello`, then press `Space c j`, type `el`, and enter the displayed label for `e`. Confirm that only visible matches receive labels, other text dims, and the cursor lands on `e`. Restart and confirm that Escape cancels without moving the cursor.
8. **Save safety:** make a buffer dirty and quit with `Space q q`; confirm that the application waits for the write and exits with the edited content persisted.
9. **Terminal restoration after normal exit:** confirm that the shell prompt returns to the original screen, raw-mode input is disabled, and the cursor/keyboard behave normally.

## Failure and panic review

- For a save failure, make a file readonly before editing and quitting. The application must remain open with the dirty buffer and an error instead of exiting or overwriting the file.
- For an external modification, edit the same file with another process before Eggplant saves. Eggplant must report the conflict without overwriting either version.
- Review `crates/tui/src/terminal.rs`: initialization cleans up partial failures, normal restoration attempts both alternate-screen exit and raw-mode disable even if the first operation fails, and the panic hook ignores cleanup errors before delegating to the original hook. A panic during cleanup must not cause a second panic.

If a development build ever panics inside a real terminal, the expected result is the same shell/terminal restoration as normal exit. Report any reproduction that leaves the terminal in raw mode or the alternate screen.

## Cleanup

```sh
rm -rf "$workdir"
```
