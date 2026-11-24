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

- For Helium and Chrome 121 or later, download `todoer-pro-X.Y.Z-chromium.zip` from an `extension-vX.Y.Z` release. In Helium, open `chrome://extensions`, enable **Developer mode**, and drag the ZIP from your file manager onto the page. To use **Load unpacked**, extract the ZIP and select the folder containing `manifest.json`. If you use a source checkout, select its `extension` directory. [GitHub Releases](https://github.com/mattmezza/todoer.pro/releases)
- For Firefox 140 or later, run `node scripts/build-firefox-extension.mjs` from the repository root. Open `about:debugging#/runtime/this-firefox`, select **Load Temporary Add-on**, and choose `.local/browser-extension/firefox/manifest.json`. Load it again after restarting Firefox.

To update an unpacked installation, replace the files in the same folder and click **Reload** on the extension’s card. For Firefox, rerun the build command before clicking **Reload**.

Right-click the Todoer toolbar icon for **Open in tab** and **Extension settings**. In settings, choose the toolbar popup or browser side panel. In Firefox and browsers based on Chromium 142 or later, you can also set a second click to close the side panel. Paste a default URL from your Todoer server, or leave it blank for Inbox. To open a different Todoer page from a particular website, pin its list, space, task, or subtask permalink. An exact hostname takes priority over wildcard rules. `*.cloud.microsoft` matches subdomains at any depth, `*.com` matches .com sites, and `*` matches any website. Among wildcards, the most specific rule wins.

[Get the extension source](https://github.com/mattmezza/todoer.pro/tree/main/extension).

Click **Todoer Pro** in the browser toolbar to open the web app in a dropdown anchored to the icon. Choose **Allow access** the first time you connect the extension to a Todoer server. Todoer uses your normal browser session. If you need to sign in, use the sign-in link in the dropdown, then reopen it. To use a self-hosted instance, save its HTTPS origin in the extension options. The Todoer server must support the toolbar panel.

To add a pin, copy the URL from Todoer. On the website you want to associate with it, right-click and choose **Choose Todoer link for this site**. Paste the URL and save the pin. The toolbar button opens that link from the same hostname. Use extension settings to edit or remove pins.

On a server that supports browser capture, right-click a page or selected text and choose **Capture this page in Todoer**. The app opens a draft in a browser tab with the page title, link, and any selected text. Review the draft and choose **Add task** to save it. Todoer starts with your pinned list when you can edit it and uses Inbox otherwise.

Pins stay in this browser. You can review an unfinished capture from an earlier version in extension options. To revoke an old extension credential, open Todoer’s **Connected tools**.

## Keep your work portable

Export your work or import a supported backup from the app. Self-hosted deployments also need independent database and attachment backups. [Migration and export](rebuild/migration-export.md) explains what each format preserves and how to validate a restore.
