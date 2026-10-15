# Contributing and policy changes

This is Justin's personal guidebook. Suggestions are welcome, but a contributor's preference is not a new instruction from Justin. Do not add personal facts, chat transcripts, private source links, or credentials to an issue or pull request.

1. Read `GUIDEBOOK.md` and the relevant source module. Preserve stable rule IDs; new rules get a new ID. Distinguish an explicit user instruction from an implementation choice.
2. Explain the concrete problem, proposed behavior, privacy implications, and affected rules. Keep current user instructions and inherited project licenses controlling in their proper scope.
3. Change source modules rather than generated copies. Update `policy.json`, `CHANGELOG.md`, and provenance when changing policy; build the standalone guidebook and entry points. Run the full instruction audit in AUD-001 through AUD-020, record meaningful findings, and distinguish semantic review from automated checks and live model behavior.
4. Run `python3 scripts/guidebook.py check` and `python3 -m unittest discover -s tests -v`. Test changed behavior through the real CLI. If it changes installation, exercise a disposable Git repository with existing instructions and an unrelated working directory.
5. Review the diff, staged files, and test evidence. Do not claim behavioral model compliance from static checks. Submit a reviewable change; do not bypass required checks or merge an unreviewed policy change without Justin's authorization.

Human-facing dates use MM-DD-YYYY. Software version strings and machine timestamps keep their required syntax. The project is a document repository; the small Python maintenance tool is a recorded exception to the Rust-first application default.
