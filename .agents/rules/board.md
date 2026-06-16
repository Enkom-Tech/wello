# Shared Kanban — how to use the board in this repo

There is a shared Akira Kanban board. **Read and update it through the `akira-board` command
only — never open `data/hermes/**/kanban.db` directly** (concurrent SQLite writers corrupt it).
If `akira-board` isn't on PATH, run the akira repo's `scripts/agents/install-akira-board.ps1` once.

## When you start working in this repo
Run this first to see the cards that concern this repo:

```powershell
akira-board sync
```

If a card matches what you're about to do, announce you're picking it up:

```powershell
akira-board start <card-id> -Agent "<tool>:host"
```

## While working
Post progress so other agents (and the dashboard) see it:

```powershell
akira-board note <card-id> "what changed"
```

## When done
```powershell
akira-board done <card-id> -Summary "outcome; tests status"
```

If you're blocked: `akira-board block <card-id> -Reason "..."`. New work with no card: `akira-board create "<title>" -Body '<route block>'`.

## If something breaks unexpectedly — check for another agent FIRST
Multiple agents (host Claude/Cursor sessions AND Hermes workers) may be working this repo at the
same time. Before assuming an unexpected failure is your own bug — a file that changed under you,
a git conflict, a vanished/locked file, tests that suddenly fail — run:

```powershell
akira-board who                      # who else is active in this repo right now
akira-board who path/to/file1 ...    # also flag if they've touched the same files
```

It lists other live host agents (with the files they've recently edited) and live Hermes workers
(with their cards). If one overlaps your work, **coordinate before fighting it**: comment on its
card (`akira-board note <id> "..."`), pause, or pick a different task. Don't both edit the same
files blind. Your own presence (and the files you edit) is posted automatically via session hooks.
