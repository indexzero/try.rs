# vhs demo — render with `vhs demo.tape` (produces demo.gif).
# Drives the real binary against a scratch tries dir.
Output demo.gif

Set FontSize 18
Set Width 900
Set Height 500
Set Theme "Catppuccin Mocha"

Hide
Type "export TRY_PATH=$(mktemp -d) && mkdir -p $TRY_PATH/2026-07-08-rust-port $TRY_PATH/2026-07-01-vhs-experiments $TRY_PATH/2026-06-20-fuzzy-scoring"
Enter
Type `eval "$(./target/release/tryme init)"`
Enter
Type "clear"
Enter
Show

Type "try" Sleep 500ms Enter
Sleep 1.5s
Type "fuz"
Sleep 1.5s
Enter
Sleep 1s
Type "pwd" Sleep 500ms Enter
Sleep 2s
