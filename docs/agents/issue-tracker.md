# Issue tracker: Linear

Track specs and tickets in Linear, team `Pallas`, project `web2026`.

## Access through Executor

Agents reach Linear through the Executor MCP.

1. Find a tool with `search`, for example `linear save_issue`. Take the exact path and input type from the result, and never guess argument names.
2. Run it with `call`, or with `execute` when one result feeds the next call.
3. If Executor tools are absent from the session, say so and ask for access. Do not write local `.scratch/` issue files.

A missing standalone Linear connector does not mean Linear is unreachable. Credentials stay out of chat and repository files.

## Operations

All operations use team `Pallas` (key `PAL`) and project `web2026`. Pass `team: "Pallas"` and `project: "web2026"` when you create an issue.

- Create: `tools.linear.mutations.save_issue` with `team`, `project`, `title`, `description`, and `labels`.
- Read: `tools.linear.queries.get_issue` with the issue key (`PAL-123`). Request `includeRelations` for blockers. Comments are not in the issue; request them separately when you need the discussion.
- List: `tools.linear.queries.list_issues` with `project: "web2026"`. Filter by `state`, `label`, `assignee`, or `parentId`.
- Comment: `tools.linear.mutations.save_comment` with `issueId` and `body`.
- Label: `save_issue` with `id`, `addLabels`, and `removeLabels`.
- Close: `save_issue` with `id` and a closing `state` (`Done` or `Canceled`), then a comment with the resolution.

"Publish to the issue tracker" means create a Linear issue in this project. "Fetch the relevant ticket" means read the issue and its comments.

The project is at https://linear.app/parthm/project/web2026-43e90ffa8ace.

## Moved GitHub issues

This repo used GitHub Issues until 2026-10-22. Every open GitHub issue was copied to Linear and closed on GitHub with a link. Each moved Linear issue links its GitHub original. A `#N` in Linear issue text, ADRs, or older docs refers to a closed GitHub issue, not a Linear issue. Use the Linear issue for current scope and status.

Read the full body and comments before implementing a ticket.

## Dependencies

Use Linear `blockedBy` relations. Read them with `get_issue` and `includeRelations`. Add a blocker with `save_issue`, `id`, and `blockedBy: ["PAL-n"]`.

A ticket can start when all its blockers are closed. A ready-for-agent label does not override an open blocker.

Reference the parent spec in each ticket. When using to-tickets, leave the parent unchanged.

## Pull requests as a triage surface

PRs as a request surface: no.
