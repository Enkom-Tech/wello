# Shared Kanban — how to use the board in this repo

There is a shared Akira Kanban board. **Read and update it through the `akira-board` command
only — never open `data/hermes/**/kanban.db` directly** (concurrent SQLite writers corrupt it).
If `akira-board` isn't on PATH, run the akira repo's `scripts/agents/install-akira-board.ps1` once.

## When you start working in this repo
The SessionStart hook auto-runs `sync` and prints your **board session id** plus any other
agents already active here. To see the cards yourself:

```powershell
akira-board sync
```

**Declare your area** so other agents can work this repo in parallel without colliding — use the
session id the hook printed. Optionally reserve the files/globs you intend to own:

```powershell
akira-board working "<focus, e.g. BLE transport refactor>" -Session <id> -Files src/ble/*,src/transport.zig
```

Multiple agents in one repo is fine **as long as you're in different areas**. Before starting,
check who else is here and what they're touching:

```powershell
akira-board who                  # other agents' declared areas + files they've edited
```

If another agent's area/files overlap yours, coordinate or pick a different area. If they don't
overlap, just proceed — parallel work in separate areas is expected.

If a card matches what you're about to do, claim it — pass your session id so it links to you
(then `sync`/`who` show others that you're on it, in the `active` column):

```powershell
akira-board start <card-id> -Session <id>
```

`akira-board sync` marks any card already being worked by a live agent in its `active` column —
**don't pick up a card that already shows an owner there** unless you're collaborating.

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

**A file changed and you didn't change it?** Don't investigate from scratch — ask who did:

```powershell
akira-board blame path/to/file      # "agent Y (focus: ...) edited this 2m ago"
```

If `blame` names another agent, that change is theirs, not stray corruption — don't try to "fix"
or revert it. You'll also get an **automatic warning** before you edit a file another live agent
is working in (injected by a PreToolUse hook) — heed it.
