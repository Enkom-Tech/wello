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
