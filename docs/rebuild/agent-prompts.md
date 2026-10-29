# Give your agent a useful starting point

Todoer works with your agent through remote MCP or the `todoer` CLI. Use these prompts with an agent that can read documentation and connect to your instance. Replace the server and list names with yours. Your agent's own AI service may charge for its work; using the task API does not invoke Todoer's built-in AI assistance.

## Connect to cloud or self-hosted Todoer

> Connect to my Todoer at https://app.todoer.pro. Read https://docs.todoer.pro/llms.txt and the Todoer skill at https://github.com/mattmezza/todoer.pro/blob/main/skills/todoer.pro/SKILL.md. Prefer my existing MCP connection. Otherwise help me add its /mcp endpoint using the client's supported remote HTTP OAuth setup, or use the todoer CLI and browser login. Start with read access to the lists I choose. Verify the connection by listing those lists, without changing tasks. Keep credentials in the client credential store.

For self-hosting, replace `https://app.todoer.pro` with your server's HTTPS origin. MCP and REST credentials have different audiences; a CLI login does not sign a remote MCP client in. See [CLI sign-in](cli-login.md), [identity and permissions](authentication.md), and [the agent workflow](agent-workflow.md).

## Get a daily rundown

> Use my Todoer connection to give me today's rundown. Confirm my timezone if you do not know it. Show overdue tasks, tasks planned for today, upcoming deadlines, and work completed today in my selected lists. Use bounded structured queries and the review/planning capabilities actually exposed by the server. Include task links and retain coverage notes. Suggest three next actions based on the task data; do not change dates, priorities or completion state.

This is a read-only agent summary. It does not require a built-in daily-report feature. Review data can have incomplete older history; the agent should preserve those caveats rather than inventing activity. See [dates](scheduling.md), [search](search.md), and [weekly review](statistics.md).

## Capture a thought without losing context

> Add these tasks to my Groceries list: milk, bread, and coffee. Resolve the exact accessible list first; ask if it is ambiguous. Capture them in one batch with a retained idempotency key. Return the task links. If the request times out, retry the identical payload with that same key.

## Break a project into reviewed tasks

> Help me break “prepare the launch announcement” into a useful checklist. Read only the Todoer project context I authorize. Draft the checklist here first, keeping my original meaning and leaving dates unset unless I supply them. After I approve it, resolve the target list and add the reviewed tasks using the supported capture API. Do not move or share existing work.

Your connected agent can draft using its own AI service. Todoer's [built-in assistance](assist.md) has a separate configuration and review contract; discover its current availability before requesting it.

## Deploy Todoer with an agent

> I want to deploy Todoer version v1.0.1 on my own server. Read https://docs.todoer.pro/rebuild/deployment.md and the repository deployment runbook. Inspect my environment, then prepare the version-pinned image, HTTPS proxy, authentication configuration, and independent database and attachment backups. Keep credentials in local ignored files. Show the exact configuration and rollout steps before touching production. Do not enable development authentication. If importing existing data, use an independent backup, show a dry-run, explicitly map accounts, and verify counts and attachments before cutover.

`v1.0.1` is an example: choose an actually published release. [Deployment](deployment.md), [migration](migration-export.md), and [backup and restore](local-recovery.md) explain the operational requirements.

## Find documentation without scraping the UI

Agents can start at [llms.txt](https://docs.todoer.pro/llms.txt), load [all published guides](https://docs.todoer.pro/llms-full.txt), or request a page's `.md` URL instead of `.html`. Each guide includes a **Read as Markdown** link. The Markdown and combined text are generated from the same published guides as the website; source-only planning documents are excluded.
