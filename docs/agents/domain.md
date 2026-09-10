# Domain docs

Use a single-context layout: root CONTEXT.md and docs/adr/.

Before exploring, read CONTEXT.md and ADRs relevant to the work.
If a root CONTEXT-MAP.md exists later, follow its relevant context links.

Proceed silently when these files are absent. Domain modeling creates
them as terminology and architectural decisions are resolved; setup
does not create placeholder domain documents.

Use glossary terms in code, tests, and tickets. Identify genuine gaps
for domain modeling. Surface conflicts with existing ADRs explicitly
rather than silently overriding their decisions.
