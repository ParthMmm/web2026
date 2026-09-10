# Issue tracker: GitHub

Track specs and tickets in ParthMmm/web2026 using the gh CLI.
Use --repo ParthMmm/web2026 when outside the checkout.

## Operations

- Read: gh issue view <number> --json title,body,comments,labels,state
- List: gh issue list --state open --json number,title,labels,assignees
- Publish: gh issue create --title "..." --body-file <file>
- Comment: gh issue comment <number> --body-file <file>
- Label: gh issue edit <number> --add-label "..." --remove-label "..."
- Close: gh issue close <number> --comment "..."

Read the full body and comments before implementing a ticket.

## Dependencies

Use native GitHub blocking relationships. Read them with:
gh api repos/ParthMmm/web2026/issues/<number>/dependencies/blocked_by

Add a blocker with:
gh api --method POST repos/ParthMmm/web2026/issues/<number>/dependencies/blocked_by -F issue_id=<blocker-database-id>

Fetch the database ID with:
gh api repos/ParthMmm/web2026/issues/<blocker-number> --jq .id

A ticket can start when all its blockers are closed. A ready-for-agent
label does not override an open blocker.

Reference the parent spec in each ticket. When using to-tickets,
leave the parent unchanged.

## Pull requests as a triage surface

PRs as a request surface: no.
