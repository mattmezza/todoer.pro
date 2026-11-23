# A little less in your head

Todoer gives your tasks a home, from a quick thought to a project with its own next steps. Start with a list and one task. Add structure when it helps.

## Make yourself at home

Open [app.todoer.pro](https://app.todoer.pro) and sign in using an option enabled by your server. Registration depends on the deployment. Self-hosting? Start with the [deployment guide](rebuild/deployment.md).

A **space** groups one area of your life, such as Personal or Work. A **list** holds a project or a collection of tasks inside that space. On your first visit, the optional welcome flow can help you choose spaces and useful lists. You can decide later, too.

## Capture the thought

Open a list and add the thing you want to remember. A task title can be a rough sentence; it does not need a plan behind it. Add a description when context will make it easier to pick up later.

[Smart capture](rebuild/smart-capture.md) explains the supported capture syntax and how to review suggestions before adding tasks.

## Find a next step

Use sections to group a list, and nested tasks to break a larger piece of work into smaller actions. Tags help you find related work across a list. Add a date when it is meaningful.

- [Organize lists, sections and tags](rebuild/references.md)
- [Understand dates and recurring tasks](rebuild/scheduling.md)
- [Search your work](rebuild/search.md)
- [Plan a weekly review](rebuild/statistics.md)

## Keep the context close

Descriptions, comments and attachments keep the details beside the task. Moving work can change who has access. Choose the destination and select **Move** to apply it immediately; the page stays in place and shows a confirmation toast.

- [Discussion and attachments](rebuild/attachments.md)
- [Sharing and moving lists](rebuild/sharing-moves.md)
- [Notifications](rebuild/notification-center.md)

## Work from your own tools

The web app works on its own. The CLI, REST API and MCP interface give compatible clients another way to work with your tasks, using explicit authorization.

[Sign in with the CLI](rebuild/cli-login.md), then explore the [CLI and agent workflow](rebuild/agent-workflow.md). Optional [built-in assistance](rebuild/assist.md) produces drafts you review before applying.

### Open Todoer from your browser toolbar

The extension source can be loaded locally in desktop Chrome and Firefox.

- In Chrome, open `chrome://extensions`, enable **Developer mode**, select **Load unpacked**, and choose the `extension` directory.
- In Firefox 140 or later, open `about:debugging#/runtime/this-firefox`, select **Load Temporary Add-on**, and choose `extension/manifest.json`. Load it again after restarting Firefox.

When the source changes, click **Reload** on the extension’s card on the same browser page and reopen the popup or Settings.

[Get the extension source](https://github.com/mattmezza/todoer.pro/tree/main/extension).

Open **Todoer Pro** from the browser toolbar and select **Settings**. Use the cloud server address or enter the HTTPS origin of your own Todoer instance. Select **Sign in with Todoer**, then approve the spaces or lists you want to share. Choose **Read and write** to add and complete tasks, or **Read** to browse. Your browser may ask to allow access to a self-hosted server.

If your server has not been updated to support extension sign-in, expand **Use a REST credential** in the extension. In Todoer’s **Connected tools**, create a **Script (REST API)** credential for the lists you want, then paste it into the extension.

Switch to the list you want, then select **Pin to** followed by the current website hostname. The extension opens that list whenever you open the popup on that hostname. Subdomains can have their own pins. On unpinned sites, the popup opens an available Inbox or the first accessible list. Manage existing pins in the extension’s settings.

Pins stay in this browser. Task changes go to your selected Todoer server. **Disconnect** removes the saved credential and revokes access issued through **Sign in with Todoer** when the server is reachable. To revoke pasted credentials or revoke access from another device, use Todoer’s **Connected tools**.

## Keep your work portable

Export your work or import a supported backup from the app. Self-hosted deployments also need independent database and attachment backups. [Migration and export](rebuild/migration-export.md) explains what each format preserves and how to validate a restore.
