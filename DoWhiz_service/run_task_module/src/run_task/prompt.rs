use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::errors::RunTaskError;
use super::types::UserIdentities;
use super::workspace::resolve_rel_dir;

const GITHUB_NOTIFICATIONS_ADDRESS: &str = "notifications@github.com";

/// Check if we've already prompted user to register in this thread.
/// Returns true if a marker file exists, indicating we've already prompted.
fn has_prompted_registration(workspace_dir: &Path) -> bool {
    workspace_dir.join(".registration_prompted").exists()
}

/// Mark that we've prompted the user to register in this thread.
fn mark_registration_prompted(workspace_dir: &Path) {
    let _ = fs::write(workspace_dir.join(".registration_prompted"), "1");
}

pub(super) fn build_prompt(
    input_email_dir: &Path,
    input_attachments_dir: &Path,
    memory_dir: &Path,
    reference_dir: &Path,
    workspace_dir: &Path,
    runner: &str,
    memory_context: &str,
    reply_required: bool,
    channel: &str,
    has_unified_account: bool,
    user_identities: &UserIdentities,
) -> String {
    build_prompt_internal(
        input_email_dir,
        input_attachments_dir,
        memory_dir,
        reference_dir,
        workspace_dir,
        runner,
        memory_context,
        reply_required,
        channel,
        has_unified_account,
        user_identities,
        false,
    )
}

pub(super) fn build_prompt_with_fast_completion(
    input_email_dir: &Path,
    input_attachments_dir: &Path,
    memory_dir: &Path,
    reference_dir: &Path,
    workspace_dir: &Path,
    runner: &str,
    memory_context: &str,
    reply_required: bool,
    channel: &str,
    has_unified_account: bool,
    user_identities: &UserIdentities,
    prefer_fast_completion: bool,
) -> String {
    build_prompt_internal(
        input_email_dir,
        input_attachments_dir,
        memory_dir,
        reference_dir,
        workspace_dir,
        runner,
        memory_context,
        reply_required,
        channel,
        has_unified_account,
        user_identities,
        prefer_fast_completion,
    )
}

fn build_prompt_internal(
    input_email_dir: &Path,
    input_attachments_dir: &Path,
    memory_dir: &Path,
    reference_dir: &Path,
    workspace_dir: &Path,
    runner: &str,
    memory_context: &str,
    reply_required: bool,
    channel: &str,
    has_unified_account: bool,
    user_identities: &UserIdentities,
    prefer_fast_completion: bool,
) -> String {
    let memory_section = if memory_context.trim().is_empty() {
        "Memory context (from memory/*.md):\n- (no memory files found)\n\n".to_string()
    } else {
        format!(
            "Memory context (from memory/*.md):\n{memory_context}\n\n",
            memory_context = memory_context.trim_end()
        )
    };

    // Channel-specific reply instructions
    let reply_instruction = if !reply_required {
        "2. After finishing the task (step one), do not write any reply. This inbound message is from a non-replyable address, so skip creating any reply files."
    } else {
        match channel.to_lowercase().as_str() {
            "slack" => {
                "2. After finishing the task (step one), write a plain text reply in reply_message.txt in the workspace root. Use Slack mrkdwn formatting: *bold*, _italic_, `code`, ```code blocks```. Keep the reply concise and conversational. Do not use HTML. If there are files to attach, put them in reply_attachments/ and mention them in the reply. Do not pretend the job has been done without actually doing it."
            }
            "discord" => {
                "2. After finishing the task (step one), write a plain text reply in reply_message.txt in the workspace root. Use Discord markdown formatting: **bold**, *italic*, `code`, ```code blocks```. Keep the reply concise and conversational. Do not use HTML. If there are files to attach, put them in reply_attachments/ and mention them in the reply. Do not pretend the job has been done without actually doing it."
            }
            "telegram" => {
                "2. After finishing the task (step one), write a plain text reply in reply_message.txt in the workspace root. Use Telegram MarkdownV2 formatting. Keep the reply concise. Do not use HTML. If there are files to attach, put them in reply_attachments/. Do not pretend the job has been done without actually doing it."
            }
            "sms" => {
                "2. After finishing the task (step one), write a plain text reply in reply_message.txt in the workspace root. Keep the reply concise and conversational. Do not use HTML. If there are files to attach, put them in reply_attachments/ and mention them in the reply. Do not pretend the job has been done without actually doing it."
            }
            "bluebubbles" => {
                "2. After finishing the task (step one), write a plain text reply in reply_message.txt in the workspace root. Keep the reply concise and conversational. Do not use HTML or markdown. If there are files to attach, put them in reply_attachments/ and mention them in the reply. Do not pretend the job has been done without actually doing it."
            }
            "whatsapp" => {
                "2. After finishing the task (step one), write a plain text reply in reply_message.txt in the workspace root. Keep the reply concise and conversational. Do not use HTML. If there are files to attach, put them in reply_attachments/ and mention them in the reply. Do not pretend the job has been done without actually doing it."
            }
            "wechat" | "wechat_mp" => {
                "2. After finishing the task (step one), write a plain text reply in reply_message.txt in the workspace root. Keep the reply concise and conversational. Do not use HTML or markdown. If there are files to attach, put them in reply_attachments/ and mention them in the reply. Do not pretend the job has been done without actually doing it."
            }
            "lark" | "feishu" => {
                r#"2. After finishing the task (step one), write a plain text reply in reply_message.txt in the workspace root. Keep the reply concise and conversational. Lark supports basic markdown: **bold**, *italic*, ~~strikethrough~~, `code`. If there are files to attach, put them in reply_attachments/ and mention them in the reply. Do not pretend the job has been done without actually doing it.

LARK TOOLS (use lark_cli for Lark operations):
- Do NOT use browser automation or OAuth flows for Lark operations.
- Use the `lark_cli` command-line tool for Lark Docs, Sheets, Bitable, and Drive.
- The CLI authenticates automatically using environment variables (LARK_APP_ID, LARK_APP_SECRET).

Available commands:
- Docs: `lark_cli get-doc`, `lark_cli read-doc`, `lark_cli create-doc`
- Sheets: `lark_cli get-sheet`, `lark_cli read-range`, `lark_cli write-range`, `lark_cli append-rows`
- Bitable (database): `lark_cli list-tables`, `lark_cli get-table`, `lark_cli query-records`, `lark_cli create-record`, `lark_cli update-record`, `lark_cli delete-record`
- Drive: `lark_cli list-files`, `lark_cli get-file`, `lark_cli create-folder`
- Sharing: `lark_cli share-file` (share docs/sheets/bitable with users)

Example usage:
- List files: `lark_cli list-files`
- Create doc: `lark_cli create-doc --title "My Document"`
- Read sheet range: `lark_cli read-range --spreadsheet-id "shtXXX" --sheet-id "Sheet1" --range "A1:C10"`
- Query bitable records: `lark_cli query-records --app-token "appXXX" --table-id "tblYYY"`
- Share a file: `lark_cli share-file --token "docXXX" --file-type docx --member-id "ou_xxx"`

SHARING FILES: When you create a doc/sheet/bitable, share it with the user so they can access it.
- Use the user's Lark open_id from "Lark Open IDs" in the User Context section above
- If "Lark Open IDs" is not listed, tell the user they need to link their Lark account at dowhiz.com first
- Command: `lark_cli share-file --token <file_token> --file-type <type> --member-id <open_id>`

See `.agents/skills/lark/SKILL.md` for complete command reference."#
            }
            "notion" => {
                r#"2. After finishing the task (step one), you MUST reply directly to the Notion comment using the Notion API CLI.

CRITICAL RESTRICTIONS:
- Do NOT use browser automation or browser-use for Notion. The API is faster and more reliable.
- Do NOT try to log into Notion via Google or any other OAuth flow.
- Do NOT create reply_email_draft.html - this is a Notion @mention, not email.
- ONLY use the notion_api_cli command-line tool.

STEP 0 - GET THE TASK CONTENT (IMPORTANT):
The incoming_email may have empty/minimal content due to API timing. You MUST fetch the actual task:
1. Read .notion_context.json to get `page_id` and `comment_id`
2. Source the OAuth token: `source .notion_env`
3. Fetch ALL comments: `notion_api_cli get-comments <page_id>`
4. Find YOUR task by matching `comment_id` from .notion_context.json
5. The text of that comment is YOUR TASK - execute it

If there are multiple comments, each ACI handles ONE specific comment_id. Only execute the task from YOUR comment_id.

DUPLICATE REPLY CHECK (recommended but optional):
Before posting a reply, review the existing comments from get-comments output. If you already replied to this task (your previous message is visible), you may skip posting again. However, if you have genuinely new information to share or the situation warrants additional communication, feel free to post.

To reply after completing the task:
1. Post your reply: `notion_api_cli create-comment <page_id> "Your message"`
2. Create the marker: `touch .notion_api_replied`

The .notion_env file contains NOTION_API_TOKEN. The .notion_context.json has page_id and comment_id.

The marker file `.notion_api_replied` is REQUIRED. Without it, the task retries and creates duplicate replies.

Keep your reply concise. Use the API only - no browser automation."#
            }
            "zoom" => {
                r#"2. After finishing the task (step one), you MUST route your reply to a linked channel since Zoom has no API for in-meeting replies.

REQUIRED STEPS:
1. Check the "User Context" section below for the user's linked channels
2. Route to the FIRST available channel in this precedence order:
   - Email (preferred): {"channel": "email", "identifier": "<email>"}
   - Lark: {"channel": "lark", "identifier": "<open_id>"}
   - Slack: {"channel": "slack", "identifier": "<user_id>"}
   - WeChat: {"channel": "wechat", "identifier": "<user_id>"}
   - WeChat MP: {"channel": "wechat_mp", "identifier": "<open_id>"}
   - Discord: {"channel": "discord", "identifier": "<user_id>"}
3. Write reply_routing.json with the chosen channel
4. Write your reply in the TARGET channel's format:
   - email: reply_email_draft.html (HTML), attachments in reply_email_attachments/
   - all others: reply_message.txt (plain text or channel-appropriate markdown)

If no channels are linked, complete the task but note in your logs that the reply cannot be delivered.

Do not pretend the job has been done without actually doing it."#
            }
            _ => {
                // Default to email (HTML)
                if prefer_fast_completion {
                    "2. Recovery-mode override for email replies: write a useful HTML email draft in reply_email_draft.html as soon as you have enough information to help the user. In this recovery run, a concise but honest email reply is preferable to timing out while trying to rebuild the entire original project. Keep the HTML content-focused: use semantic blocks like paragraphs, lists, tables, headings, and links, and avoid hard-coding narrow outer containers, oversized side margins, or overflow-prone layouts because DoWhiz applies a shared responsive email shell at send time. Do NOT start new PDFs, slide decks, LaTeX reports, or other large attachments unless the user explicitly required that format and it is already nearly complete. If the original task is blocked or cannot be fully completed within this run, explain what you were able to verify, what remains uncertain, and what next step or source would be needed."
                } else {
                    "2. After finishing the task (step one), make sure you write a proper HTML email draft in reply_email_draft.html in the workspace root. Keep the HTML content-focused: use semantic blocks like paragraphs, lists, tables, headings, and links, and avoid hard-coding narrow outer containers, oversized side margins, or overflow-prone layouts because DoWhiz applies a shared responsive email shell at send time. If there are files to attach, put them in reply_email_attachments/ and reference them in the email draft. Do not pretend the job has been done without actually doing it, and do not write the email draft until the task is done. If you are not sure about the task, send another email to ask for clarification (and if any, attach information about why did you fail to get the task done, what is the exact error you encountered)."
                }
            }
        }
    };
    let guidance_section = build_guidance_section(workspace_dir, runner);
    let discord_context_section = if channel.eq_ignore_ascii_case("discord") {
        build_discord_context_section(workspace_dir)
    } else {
        String::new()
    };
    let github_coauthor_section = build_github_coauthor_section(workspace_dir, input_email_dir);
    let user_identities_section = build_user_identities_section(user_identities);
    let tpm_capabilities_section = build_tpm_capabilities_section(user_identities);
    let filesystem_security_section =
        build_allowed_paths_section(&user_identities.allowed_user_ids);
    let web_auth_capabilities_section = build_web_auth_capabilities_section();
    let human_approval_gate_section = build_human_approval_gate_section();
    let chat_history_capabilities_section =
        build_chat_history_capabilities_section(workspace_dir, channel);
    let fast_completion_section = if prefer_fast_completion {
        r#"Claude fallback execution guidance:
- This run is a recovery path after the primary runner failed. These recovery instructions take precedence over conflicting planning or artifact-building advice elsewhere in this prompt.
- Prioritize delivering a useful reply within the recovery budget over rebuilding the entire original project from scratch.
- Before starting new research, inspect any existing artifacts from the primary runner, especially `.codex_remote_output.log` and `.run_task_trace_codex_primary/`, and reuse any facts, sources, filenames, or failure context already gathered there.
- For long research or writing tasks, begin updating the final reply artifact early and keep it current as sections become ready.
- If you are still gathering evidence, clearly mark the draft as a working draft near the top, and remove or replace that note before you finish if the reply becomes complete.
- Prefer a concise in-email deliverable over new PDFs, slide decks, LaTeX reports, or other multi-file attachments unless the user explicitly required that format and it is already almost complete.
- Keep any new web research focused. Reuse evidence already gathered, avoid repeating similar searches, and stop searching once you have enough support to answer the user's questions coherently.
- If authoritative evidence is missing or the research path is blocked, send an honest limitation / next-steps reply instead of timing out with no reply.
- If you can finish only part of the task, state what is completed, what remains uncertain, and what sources or follow-up would be needed to finish the rest.

"#
    } else {
        ""
    };

    // Build registration prompt section if user doesn't have a unified account
    // and we haven't prompted them yet in this thread.
    // Skip for Notion channel since Notion users can't easily link accounts from there,
    // and the webhook sender identity doesn't map to DoWhiz accounts.
    let is_notion_channel = channel.eq_ignore_ascii_case("notion");
    let registration_section = if !has_unified_account
        && !has_prompted_registration(workspace_dir)
        && !is_notion_channel
    {
        // Mark that we've prompted so we don't repeat
        mark_registration_prompted(workspace_dir);
        r#"
Account Registration Notice:
- This user does not have a DoWhiz unified account linked.
- At the END of your reply (after completing the task), add a brief note like:
  "💡 Tip: Link your DoWhiz account to sync your preferences and project info across all channels (email, Google Docs, Slack, etc.). Visit https://www.dowhiz.com/auth/index.html to get started."
- Only mention this once - do not repeat in subsequent messages.
"#
    } else {
        ""
    };

    format!(
        r#"You are a DoWhiz digital employee. Follow the employee guidance provided below. Your task is to read incoming emails, understand the user's intent, finish the task, and draft appropriate email replies. You can also use memory and reference materials for context (already saved under current workspace). Always be cute, patient, friendly and helpful in your replies.

Employee guidance (from workspace files):
{guidance_section}

You main goal is
1. Most importantly, understand the task described in the incoming email and get the task done.
{reply_instruction}

Inputs (relative to workspace root):
- Incoming email dir: {input_email} (latest raw payload plus `thread_request.md`, `thread_history.md`, and `entries/`)
- `incoming_email/thread_request.md` is the canonical merged request for reruns after follow-up messages. Latest follow-up wins if it conflicts with older instructions.
- `incoming_email/thread_history.md` maps the full thread history and raw source files.
- For incoming email, all previous emails in current thread: /incoming_email/entries/
- Incoming attachments dir: {input_attachments}
- `incoming_attachments/` is the merged attachment view across the whole active thread. Historical per-message copies remain under `incoming_attachments/entries/`.
- Memory dir (memory about the current user): {memory}
- Reference dir (contain all past emails with the current user): {reference}

{discord_context_section}
{github_coauthor_section}

Memory about the current user:
```{memory_section}```

Memory management and maintain policy:
- Read all Markdown files under memory/ before starting; they are long-term, per-user memory.
- Persist durable facts only (identity, preferences, recurring tasks, projects, contacts,
  decisions, and working processes). Do not store transient email-specific details.
- Default file is memory/memo.md (Markdown).
- If memo.md exceeds 500 lines, split by info type into multiple files (for example:
  memo_profile.md, memo_preferences.md, memo_projects.md, memo_contacts.md,
  memo_decisions.md, memo_processes.md). Keep every file <= 500 lines.
- When split, replace memo.md with a short index or highlights so it stays <= 500 lines.
- Update memory files at the end if new durable info is learned; otherwise leave unchanged.

Scheduling:
- For any scheduling (email or task), you MUST use the skill "scheduler_maintain".

{cross_channel_capabilities}
{chat_history_capabilities_section}
{web_auth_capabilities_section}
{human_approval_gate_section}
{user_identities_section}
{tpm_capabilities_section}
{fast_completion_section}
Rules:
- Each workspace includes a `.env` file at the workspace root. You may edit it to manage per-user secrets; updates are synced back after the task completes.
- Do not modify input directories. Any file editing requests should be done on the copied version of attachments and save into reply_email_attachments/ to be sent back to the user. Mark version updates as "_v2", "_v3", etc. in the filename.
- You may create or modify other files and folders in the workspace as needed to complete the task.
  Prefer creating a work/ directory for clones, patches, and build artifacts.
- If attachments include version suffixes like _v1, _v2, the highest version should be the latest version.
- Avoid interactive commands; use non-interactive flags for git/gh (for example, `gh pr create --title ... --body ...`).
{filesystem_security_section}{registration_section}"#,
        input_email = input_email_dir.display(),
        input_attachments = input_attachments_dir.display(),
        memory = memory_dir.display(),
        reference = reference_dir.display(),
        memory_section = memory_section,
        guidance_section = guidance_section,
        reply_instruction = reply_instruction,
        discord_context_section = discord_context_section,
        github_coauthor_section = github_coauthor_section,
        cross_channel_capabilities = build_cross_channel_capabilities_section(),
        chat_history_capabilities_section = chat_history_capabilities_section,
        web_auth_capabilities_section = web_auth_capabilities_section,
        human_approval_gate_section = human_approval_gate_section,
        user_identities_section = user_identities_section,
        tpm_capabilities_section = tpm_capabilities_section,
        fast_completion_section = fast_completion_section,
        filesystem_security_section = filesystem_security_section,
        registration_section = registration_section,
    )
}

/// Build the cross-channel capabilities section that informs the agent
/// about available tools for operating across different channels.
fn build_cross_channel_capabilities_section() -> &'static str {
    r#"Cross-channel Tools (if user's Google account is linked):
- `gws` - Google Workspace CLI for Gmail/Drive/Docs operations
- `google-docs` - Create/read/edit documents, share files, manage folders
- `google-slides` - Create/read/edit presentations, share files, manage folders
- `google-sheets` - Create/read/edit spreadsheets, share files, manage folders

Google Docs Content Formatting:
When writing content to Google Docs, use simple HTML format (NOT Markdown):
- Headings: `<h1>Title</h1>`, `<h2>Section</h2>`
- Paragraphs: `<p>Text here</p>`
- Bold: `<b>bold text</b>`
- Lists: `<ul><li>item 1</li><li>item 2</li></ul>`
- Links: `<a href="url">link text</a>`
Example: `google-docs append <doc_id> "<h1>Project Plan</h1><p>Overview of the project...</p>"`

Discord Bot Tools (for Discord messages):
- `discord_cli send-message <channel_id> <message>` - Send message to a channel
- `discord_cli send-reply <channel_id> <message_id> <message>` - Reply to a specific message
- `discord_cli send-dm <user_id> <message>` - Send DM to a user
- `discord_cli list-guild-members <guild_id>` - List all members in a Discord server
- `discord_cli dm-all-guild <guild_id> <message>` - DM all members in a Discord server

IMPORTANT: For Discord operations (DMs, channel messages), ALWAYS use `discord_cli`.
Do NOT use browser automation for Discord - the bot token is already configured.

GitHub CLI (`gh`) - for GitHub repository operations:
- `gh repo create <name> --public/--private` - Create new repository
- `gh repo create <org>/<name> --public` - Create repo in an organization
- `gh issue create --title "..." --body "..."` - Create issue
- `gh issue list` - List issues
- `gh pr create --title "..." --body "..."` - Create pull request
- `gh release create <tag> --notes "..."` - Create release

IMPORTANT: For GitHub operations, ALWAYS use `gh` CLI (already authenticated).
Do NOT use browser automation for GitHub - the CLI is faster and more reliable.

IMPORTANT: When sharing source code with the user:
- If the user has a linked GitHub account (check User Context section below), create a GitHub repo and add them as a collaborator instead of creating tar.gz archives.
- Use `gh repo create <name> --private` to create the repo, then `gh api repos/OWNER/REPO/collaborators/GITHUB_USERNAME -X PUT` to add the user as a collaborator.
- This provides a better experience: version control, easy cloning, and future updates.
- If the user explicitly requests ownership transfer, use `gh api repos/OWNER/REPO/transfer -X POST -f new_owner=GITHUB_USERNAME` to transfer the repo to them. Note: they must accept the transfer, and you will lose access until they re-add you as collaborator.
- If the user does NOT have a linked GitHub account, tar.gz archives are acceptable.

Identity Lookup - for inviting Discord guild members to shared resources:
- When you need to share Google Docs/GitHub repos with Discord guild members, read `skills/identity-lookup/SKILL.md` for the `identity_lookup_cli` commands.

Group Project Coordination:
- When coordinating team workspaces or shared resources for multiple people, read `skills/group-project-coordination/SKILL.md` for the workflow.

Grocery Price Comparison (IMPORTANT - for shopping/price queries):

**BEFORE responding to ANY grocery question, you MUST:**
1. `cat skills/grocery-comparison/SKILL.md` - Read the FULL skill file (1000+ lines of guidance)
2. Check user's memory for "Grocery Preferences" section
3. Use browser-use to verify real-time prices (see below)

**This skill helps compare prices across:**
- Online: Weee, Yami, Kroger (API)
- Physical: H Mart, 168 Asian Mart, Costco, Sam's Club

**MANDATORY behaviors - your response will be LOW QUALITY without these:**

1. **Culinary knowledge** (核心差异化):
   - Ask/infer what DISH user wants to make before recommending ingredients
   - 糖醋小排 → small spare ribs (肋排); 排骨汤 → soup bones (筒骨)
   - See SKILL.md "菜肴 → 食材要求映射" table for full mappings

2. **Real-time price verification** (REQUIRED for online stores):
   ```bash
   IN_DOCKER=true browser-use open "https://www.sayweee.com/en/search?keyword=..."
   browser-use state  # Get current price
   browser-use click <product_index>  # Click into product page
   browser-use state  # Verify exact price
   ```
   - Do NOT rely on web search for prices (cached/outdated)
   - Include timestamp: `(查询时间: 2026-04-11 10:30 EST)`

3. **Include purchase links** (每个推荐都要有):
   - Weee/Yami: Product page URL
   - Physical stores: Google Maps link
   - Example: `[168 Asian Mart](https://maps.google.com/?q=168+Asian+Mart+Madison+Heights+MI)`

4. **Add disclaimer** (回复结尾):
   ```
   ⚠️ 价格说明: 以上价格为查询时的实时价格，可能随时变动。最终价格以实际页面为准。
   ```

5. **Suggest questionnaire** (结尾):
   ```
   💡 完成偏好问卷可获得更精准推荐: https://dowhiz.com/grocery/onboarding
   ```

**Response quality checklist** (比 GPT 更好的标准):
- ✅ 理解用户要做什么菜，推荐对的食材部位
- ✅ 实时查询了在线商家价格
- ✅ 考虑了用户位置和交通方式
- ✅ 每个推荐都有可点击链接
- ✅ 给出了具体建议，不是泛泛而谈
- ✅ 包含价格比较表格
- ✅ 说明了 trade-off（便宜但要开车 vs 贵但送货上门）

**BAD response example** (避免):
```
你可以去Weee或168买，价格大概3-5美元。
```

**GOOD response example**:
```
## 好丽友派比价 (查询时间: 2026-04-11 10:30 EST)

| 渠道 | 产品 | 价格 | 单价 | 链接 |
|------|------|------|------|------|
| [Weee](https://sayweee.com/...) | 12枚原味装 | $4.49 | $0.37/个 | [购买](url) |
| [Yami](https://yamibuy.com/...) | 12枚装 | $5.29 | $0.44/个 | [购买](url) |
| [168](https://maps.google.com/...) | 12枚装 | ~$3.99 | $0.33/个 | 实体店 |

**推荐**: 如果你住安娜堡，168最便宜但要开车25分钟。Weee贵$0.50但免费送货，适合懒得出门时。

---
⚠️ 价格说明: 以上价格为查询时的实时价格，可能随时变动。

💡 完成偏好问卷可获得更精准推荐: https://dowhiz.com/grocery/onboarding
```

Optional information channels:
- **Web search**: For reviews, user experiences, or when direct scraping fails
- **Kroger API**: `grocery_cli kroger search "product" --location 48109`

Security: Only access files the CURRENT USER has shared. Never access other users' files.
See `.agents/skills/google-*/SKILL.md` for detailed command references.

Build Tools & Compilers (available in this environment):
- `python3` - Python 3 interpreter (pip packages available)
- `node` / `npm` / `pnpm` - Node.js runtime and package managers
- `cargo` / `rustc` / `rustfmt` / `clippy` - Rust toolchain (at /usr/local/cargo/bin)
- `gcc` / `g++` / `make` - C/C++ compiler and build tools
- `git` - Version control
- `pandoc` - Document conversion
- `tesseract` - OCR
- Standard Unix tools: `curl`, `jq`, `tar`, `gzip`, etc.

When building or compiling code, use the appropriate tool directly. All are available in PATH.

Notion Tools (channel-agnostic - use these for ANY Notion operation regardless of inbound channel):
- ALWAYS use `notion_api_cli` for Notion operations. Do NOT use browser automation for Notion.
- Do NOT try to log into Notion via Google, Okta, or any other OAuth flow in the browser.

Available commands:
- `notion_api_cli read-page <page_id>` - Read page content
- `notion_api_cli get-comments <page_id>` - Get all comments on a page
- `notion_api_cli create-comment <page_id> "message"` - Create a new comment
- `notion_api_cli reply <comment_id> "message"` - Reply to an existing comment
- `notion_api_cli search "query"` - Search for pages
- `notion_api_cli create-page --parent-id <page_id> --title "Title"` - Create a new page
- `notion_api_cli update-page <page_id> --property "Key=Value"` - Update page properties

Authentication check (IMPORTANT):
1. First, check if `.notion_env` exists in the workspace - if so, `source .notion_env` to load the token
2. If `.notion_env` does NOT exist, check if `NOTION_API_TOKEN` is set in the environment
3. If neither is available, the user has NOT linked their Notion integration - politely tell them:
   "To use Notion features, please link your Notion workspace at dowhiz.com first."
   Do NOT attempt browser login as a fallback.

Example workflow for "create a Notion page about X":
1. Check for Notion token: `source .notion_env 2>/dev/null || true`
2. Verify token exists: `[ -n "$NOTION_API_TOKEN" ] || echo "No Notion integration"`
3. If token exists: `notion_api_cli create-page --parent-id <workspace_root_or_page> --title "X"`
4. If no token: Reply to user asking them to link Notion at dowhiz.com

See `.agents/skills/notion/SKILL.md` for detailed command reference.

"#
}

fn build_chat_history_capabilities_section(workspace_dir: &Path, channel: &str) -> String {
    if !workspace_dir.join(".chat_history_scope.json").exists() {
        return String::new();
    }
    match channel.to_ascii_lowercase().as_str() {
        "slack" => {
            r#"Scoped chat history search:
- When the user asks about earlier Slack discussion that is not already in the prompt or workspace files, use `.agents/skills/slack-history-search/SKILL.md`.
- The helper is backend-enforced and can search readable Slack conversations inside the current Slack workspace/team, including the current conversation.
- It cannot cross into other Slack workspaces, even if asked.

"#
            .to_string()
        }
        "discord" => {
            r#"Scoped chat history search:
- When the user asks about earlier Discord discussion that is not already in the prompt or workspace files, use `.agents/skills/discord-history-search/SKILL.md`.
- The helper is backend-enforced and can search only the current Discord server (or the current DM if this is not a guild message).
- It cannot cross into other Discord servers, even if asked.

"#
            .to_string()
        }
        _ => String::new(),
    }
}

fn build_web_auth_capabilities_section() -> &'static str {
    r#"Web Workspace Auth (Google web pages):
- ALWAYS prefer CLI tools when available:
  - For Google Docs/Sheets/Slides operations (create, edit, share, read), use `google-docs`, `google-sheets`, `google-slides` CLI tools.
  - For Notion operations, ALWAYS use `notion_api_cli` (see Notion Tools section above). NEVER use browser automation for Notion.
- Only use browser automation (`playwright-cli`) as a FALLBACK when:
  - No CLI tool exists for the service, OR
  - You need to scrape/read a private page that has no API access, OR
  - The CLI tool explicitly fails and browser is the only option.
- For plain HTTP fetches of public content, prefer `curl` or similar over browser automation.
- Complete sign-in only through the active browser session when needed.
- If Browserbase-backed browser sessions are configured, `playwright-cli` may reconnect to a persistent remote browser context from `.secrets/browserbase`. Still verify the actual page before assuming you are already signed in.
- Once a browser session is already open, prefer `playwright-cli goto <url>` for same-tab navigation. Avoid calling `playwright-cli open <url>` again inside the same login flow, especially after a human handoff, because reopening may replace the current browser session instead of continuing the live tab.
- During login, MFA, CAPTCHA, or other approval blockers, keep the flow in a single browser tab whenever possible so a live browser handoff can reopen the same stuck page. Avoid opening extra tabs until authentication is complete.
- If browser launch fails before sign-in:
  - If error says Chrome is missing, retry with:
    - `export PLAYWRIGHT_MCP_EXECUTABLE_PATH=/opt/google/chrome/chrome`
    - If that file does not exist, set it to the first hit under `/app/.cache/ms-playwright/*/chrome-linux*/chrome`.
  - Avoid `npx playwright install ...` on mounted workspaces (can fail with symlink errors).
    Prefer `python3 -m playwright install chromium` or `playwright install chromium`.
    If npm must be used, set `NPM_CONFIG_CACHE=/tmp/.npm` first.
- Never include raw credentials in any user-facing reply, logs, or generated files.
- Do not conclude "cannot access due to sign-in" until browser-based sign-in has been attempted.

"#
}

fn build_human_approval_gate_section() -> &'static str {
    r#"Human Approval Gate (2FA / verification challenges):
- If login/auth flow asks for OTP/passcode/device approval/number tap, or you are blocked on CAPTCHA/password after the required local checks below, use the `human-approval-gate` skill with an honest challenge type and browser screenshot.
- If the page shows CAPTCHA/image puzzle/text recognition challenge, do NOT attempt to solve it yourself. Immediately take the current browser screenshot(s) and use the MCP tool `dowhiz_human_approval_gate_request_and_wait` with `challenge_type="captcha"`.
- If login is waiting for a password, first check the workspace `.env` and the current environment for the relevant secret (for Google login, check `GOOGLE_PASSWORD` first; if it is not in `.env`, run `printenv GOOGLE_PASSWORD`). Only if the needed password is still missing should you use `dowhiz_human_approval_gate_request_and_wait` with `challenge_type="password"` and the current browser screenshot path(s).
- Only use `human_approval_gate` for steps that genuinely require human access outside the browser session, such as SMS codes, email codes sent to someone else, approval taps on another device, or information only the human can retrieve.
- If multiple verification methods are available on the same challenge page, prefer SMS verification first by default. If SMS is unavailable or fails, fall back to another method and keep using `dowhiz_human_approval_gate_request_and_wait` for human input.
- Before calling `dowhiz_human_approval_gate_request_and_wait` for 2FA, first use the website itself to initiate the challenge: click the button that sends the code / starts the approval / selects the method, and wait until the page is explicitly waiting for the human response.
- For `challenge_type="two_factor"`, do not send the email unless you can truthfully identify the current page state as either `waiting_for_code_input` or `waiting_for_device_approval`.
- For `challenge_type="two_factor"`, always describe the exact method in use: SMS, email, authenticator app, or device tap / number match, plus the masked destination if visible.
- If login identifier (email/username) is missing for owner/admin account login, try known admin identifiers first (`dowhiz@deep-tutor.com` on staging, `oliver@dowhiz.com` on production) before requesting help through `human_approval_gate`.
- For owner/admin account login, do NOT trigger `human_approval_gate` only to ask for account email/username when a known identifier is already available.
- If the requested login still cannot proceed because required credential/challenge input is missing after trying known safe identifiers, request it through `human_approval_gate` instead of guessing.
- Inside run_task/Codex environments, do NOT use the shell CLI `human_approval_gate` or split request/wait steps yourself. Use only the MCP tool `dowhiz_human_approval_gate_request_and_wait`, which blocks this Codex turn until reply or timeout.
- Every `dowhiz_human_approval_gate_request_and_wait` call must attach the current browser screenshot path(s) and must describe the current state honestly. Never claim a code was sent unless the page actually shows that it was sent and is waiting.
- If the HAG email includes a live browser handoff link, the human may complete the blocker directly inside that live browser. While waiting, do not keep clicking around or open new tabs in the blocked session.
- HAG sends still write `.human_approval_gate/events.jsonl` and emit a `HAG_EVENT ...` stderr line that includes challenge type plus attachment filenames and sizes. Use those records for debugging instead of guessing.
- Primary flow:
  1) Take the current browser screenshot(s)
  2) Call `dowhiz_human_approval_gate_request_and_wait`
  3) Continue only if the returned status is `replied`
  4) Inspect the returned `reply` payload yourself and decide what to type/click next
  5) If status is `timeout`, stop login attempts and report clearly
- Scope and recipient rules:
  - Agent logging into owner/admin account (for example Oliver's own Google/Notion/X, `dowhiz@deep-tutor.com`, `oliver@dowhiz.com`): use `scope="admin"` (sends to `admin@dowhiz.com`)
  - Agent logging into user's account: use `scope="user"` plus that user's recipient email
- Never keep retrying password/sign-in while waiting for verification.

"#
}

fn build_user_identities_section(identities: &UserIdentities) -> String {
    let has_any = identities.account_id.is_some()
        || !identities.emails.is_empty()
        || !identities.slack_user_ids.is_empty()
        || !identities.discord_user_ids.is_empty()
        || !identities.phone_numbers.is_empty()
        || !identities.telegram_user_ids.is_empty()
        || !identities.lark_user_ids.is_empty()
        || !identities.wechat_user_ids.is_empty()
        || !identities.wechat_mp_open_ids.is_empty()
        || !identities.wechat_mp_account_ids.is_empty()
        || !identities.github_usernames.is_empty();

    if !has_any {
        return "User Context: Not available (user has no linked DoWhiz account). \
If the user requests features requiring their linked accounts (sharing files, cross-channel routing), \
politely explain they need to link their accounts at dowhiz.com first.\n"
            .to_string();
    }

    let mut channels = Vec::new();
    if let Some(id) = &identities.account_id {
        channels.push(format!("- DoWhiz Account ID: {}", id));
    }
    if !identities.emails.is_empty() {
        channels.push(format!("- Email: {}", identities.emails.join(", ")));
    }
    if !identities.slack_user_ids.is_empty() {
        channels.push(format!(
            "- Slack User IDs: {}",
            identities.slack_user_ids.join(", ")
        ));
    }
    if !identities.discord_user_ids.is_empty() {
        channels.push(format!(
            "- Discord User IDs: {}",
            identities.discord_user_ids.join(", ")
        ));
    }
    if !identities.phone_numbers.is_empty() {
        channels.push(format!(
            "- Phone Numbers: {}",
            identities.phone_numbers.join(", ")
        ));
    }
    if !identities.telegram_user_ids.is_empty() {
        channels.push(format!(
            "- Telegram User IDs: {}",
            identities.telegram_user_ids.join(", ")
        ));
    }
    if !identities.lark_user_ids.is_empty() {
        channels.push(format!(
            "- Lark Open IDs: {}",
            identities.lark_user_ids.join(", ")
        ));
    }
    if !identities.wechat_user_ids.is_empty() {
        channels.push(format!(
            "- WeChat User IDs: {}",
            identities.wechat_user_ids.join(", ")
        ));
    }
    if !identities.wechat_mp_open_ids.is_empty() {
        channels.push(format!(
            "- WeChat MP Open IDs: {}",
            identities.wechat_mp_open_ids.join(", ")
        ));
    }
    if !identities.wechat_mp_account_ids.is_empty() {
        channels.push(format!(
            "- WeChat MP Account IDs: {}",
            identities.wechat_mp_account_ids.join(", ")
        ));
    }
    if !identities.github_usernames.is_empty() {
        channels.push(format!(
            "- GitHub: {}",
            identities.github_usernames.join(", ")
        ));
    }

    format!(
        r#"User Context (linked accounts & identifiers):
{channels}

IMPORTANT - Use these identifiers to:
- Share files with the user via platform CLIs (Lark, WeChat, etc. - NOT Google Workspace or Notion which use OAuth)
- Route replies to different channels (see Cross-channel Reply Routing below)
- Check what integrations the user has available before suggesting platform-specific features

If a required identifier is missing, tell the user to link their account at dowhiz.com.

Cross-channel Reply Routing:
If the user requests a reply on a different channel than the inbound channel,
write a `reply_routing.json` file in the workspace root to route the reply.
IMPORTANT: Use the identifiers from the User Context section above for the target channel.
If no routing file is written, the reply goes to the original inbound channel.

reply_routing.json schema:
```json
{{
  "channel": "email" | "slack" | "discord" | "telegram" | "sms" | "whatsapp" | "bluebubbles" | "wechat" | "wechat_mp" | "lark",
  "identifier": "<target identifier for the channel>"
}}
```

Identifier format per channel:
- email: email address (e.g., "user@example.com")
- slack: Slack user ID (e.g., "U1234567890")
- discord: Discord user ID (e.g., "123456789012345678")
- telegram: Telegram user ID (e.g., "123456789")
- sms/whatsapp/bluebubbles: phone number (e.g., "+15551234567")
- wechat: WeChat Work UserID (e.g., "zhangsan")
- wechat_mp: WeChat Official Account open_id (e.g., "oAbCdEfGh123456789")
- lark: Lark open_id (e.g., "ou_xxxxxxxxxxxxxxxxx")

IMPORTANT: When using cross-channel routing, write the reply in the TARGET channel's format:
- email target: reply_email_draft.html (HTML content only; DoWhiz adds the responsive shell at send time), attachments in reply_email_attachments/
- slack target: reply_message.txt (Slack mrkdwn: *bold*, _italic_, `code`)
- discord target: reply_message.txt (Discord markdown: **bold**, *italic*, `code`)
- telegram target: reply_message.txt (MarkdownV2)
- lark target: reply_message.txt (Lark markdown: **bold**, *italic*, ~~strikethrough~~, `code`)
- sms/whatsapp/bluebubbles/wechat/wechat_mp target: reply_message.txt (plain text)
- Attachments for non-email channels go in reply_attachments/

Example: Inbound is email, user says "reply to my Discord instead"
1. Write reply_routing.json: {{"channel": "discord", "identifier": "123456789012345678"}}
2. Write reply_message.txt (NOT reply_email_draft.html) with Discord markdown

SECURITY: You may ONLY route replies to the identifiers listed above under "user's linked channels".
Do NOT route replies to any other email addresses, user IDs, or phone numbers not listed.
If the user requests routing to an unlisted identifier, politely decline and explain they need to link that channel first.
"#,
        channels = channels.join("\n")
    )
}

fn build_tpm_capabilities_section(identities: &UserIdentities) -> String {
    let Some(org_name) = &identities.organization_name else {
        return String::new();
    };
    let account_id = identities
        .account_id
        .as_deref()
        .unwrap_or("<UNKNOWN_ACCOUNT_ID>");

    let db_flag = identities
        .notion_database_id
        .as_ref()
        .map(|id| format!(" --database-id {}", id))
        .unwrap_or_default();

    format!(
        r#"
=== TPM MODE ACTIVE for {org_name} ===

You are operating as a Technical Program Manager (TPM) for the {org_name} organization.

**IMPORTANT: When TPM mode is active, you MUST:**
1. Use tpm_cli for ANY request involving bugs, features, tasks, tickets, or development work
2. Track all actionable items in the task board - do not just respond without creating/updating tasks
3. For scheduled TPM syncs (from cron): ALWAYS run the full sync workflow below

**Task Classification:**
- Bugs, features, tasks, tickets, dev work → MUST use tpm_cli to create/update tasks
- Scheduled TPM sync (subject contains "TPM Sync") → MUST run full sync workflow
- General questions about task status → use tpm_cli list-tasks
- Non-dev requests (meetings, research, etc.) → handle normally, but consider if it should become a task

**TPM CLI Commands (tpm_cli):**
- `tpm_cli setup-board --organization {org_name} --parent-page-id <PAGE_ID> --workspace-id <WS_ID>` - Create a new task database in Notion
- `tpm_cli list-tasks --organization {org_name}{db_flag}` - List all tasks from Notion
- `tpm_cli list-tasks --organization {org_name}{db_flag} --status backlog` - Filter by status (backlog, in_progress, review, done, blocked)
- `tpm_cli list-tasks --organization {org_name}{db_flag} --assignee dev@example.com` - Filter by assignee
- `tpm_cli create-task --organization {org_name}{db_flag} --title "..." --description "..." --priority p1 --source user_feedback` - Create new task in Notion

**After creating a new task board (setup-board):**
The database is created in the USER's Notion workspace (they own it). The database_id is automatically saved to Supabase.
Remind the user to share the database:
- Share with team members (Can Edit) so they can update tasks
- Share with oliver@dowhiz.com (Can Edit) so I can run scheduled syncs
Include the database URL in your reply and these sharing instructions.

**Notion CLI Commands (notion_api_cli):**
- `notion_api_cli query-database --database-id <DB_ID>` - Query tasks from Notion board
- `notion_api_cli update-page --page-id <TASK_ID> --properties '{{...}}'` - Update task status/priority
- `notion_api_cli create-comment --page-id <TASK_ID> --content "..."` - Add comment to task

**Daily TPM Check-in Workflow:**
1. First, check if a task board exists: `tpm_cli list-tasks --organization {org_name}{db_flag}`
   - If you get "No notion_database_id configured" error, you MUST create the board first:
     a. Find a suitable parent page in Notion: `notion_api_cli search "workspace"` or use the workspace root
     b. Create the board: `tpm_cli setup-board --organization {org_name} --parent-page-id <PAGE_ID> --workspace-id <WS_ID>`
     c. Note: The workspace-id is in .notion_context.json or from the Notion OAuth connection
   - If board exists, proceed to step 2
2. Run `tpm_cli list-tasks --organization {org_name}{db_flag} --status blocked` to find blocked tasks
3. Identify stale tasks (no updates in 3+ days) by reviewing the full task list
4. Post summary to team channel (Discord/Slack)

**Before Creating New Tasks:**
ALWAYS run `tpm_cli list-tasks --organization {org_name}{db_flag}` first to:
1. Understand what the org is currently working on
2. Check for existing tasks that might be duplicates or related
3. See current priorities and workload distribution
4. Identify patterns in how tasks are structured

When creating a task, reference related existing tasks if applicable. Do not create duplicates.

**Task Sources:**
- user_feedback: From user reports, Discord, support emails
- notetaker: Extracted from meeting transcripts
- market_research: From competitive analysis
- manual: Manually created

**Priority Levels:** P0 (critical), P1 (high), P2 (medium), P3 (low)

**Account ID for trigger-sync:** {account_id}
- `tpm_cli trigger-sync --user-id {account_id} --organization {org_name}` - Queue immediate TPM sync task
"#,
        org_name = org_name,
        account_id = account_id,
        db_flag = db_flag
    )
}

fn build_allowed_paths_section(allowed_user_ids: &[String]) -> String {
    if allowed_user_ids.is_empty() {
        return r#"
Filesystem Security:
- You may ONLY access files within your current workspace directory.
- Do NOT traverse to parent directories or access paths outside this workspace.
"#
        .to_string();
    }

    let allowed_paths: Vec<String> = allowed_user_ids
        .iter()
        .map(|id| format!("  - /users/{}/", id))
        .collect();

    format!(
        r#"
Filesystem Security:
- You may ONLY access files within the following user directories:
{allowed_paths}
- Do NOT access any paths outside these directories.
- Do NOT attempt to access other users' data.
"#,
        allowed_paths = allowed_paths.join("\n")
    )
}

fn build_github_coauthor_section(workspace_dir: &Path, input_email_dir: &Path) -> String {
    let Some(login) = load_github_requester_login(workspace_dir, input_email_dir) else {
        return String::new();
    };
    let coauthor_email = format!("{login}@users.noreply.github.com");
    let trailer = format!("Co-authored-by: {login} <{coauthor_email}>");
    format!(
        r#"GitHub Attribution Requirement:
- This request came from GitHub user @{login}.
- If you create or amend any git commit for this task, append this trailer exactly once in each relevant commit message: `{trailer}`.
- If you open or update a PR, include `Requested-by: @{login}` in the PR body.
- Do not add co-author/requested-by lines when no commit or PR is created.

"#
    )
}

fn load_github_requester_login(workspace_dir: &Path, input_email_dir: &Path) -> Option<String> {
    let payload_path = workspace_dir
        .join(input_email_dir)
        .join("postmark_payload.json");
    let payload_raw = fs::read(payload_path).ok()?;
    let payload: Value = serde_json::from_slice(&payload_raw).ok()?;
    let from = payload
        .get("From")
        .or_else(|| payload.get("from"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !looks_like_github_notifications_sender(from) {
        return None;
    }
    extract_github_sender_from_headers(&payload)
        .or_else(|| extract_github_sender_from_bodies(&payload))
}

fn looks_like_github_notifications_sender(from: &str) -> bool {
    from.to_ascii_lowercase()
        .contains(&GITHUB_NOTIFICATIONS_ADDRESS.to_ascii_lowercase())
}

fn extract_github_sender_from_headers(payload: &Value) -> Option<String> {
    let headers = payload
        .get("Headers")
        .or_else(|| payload.get("headers"))
        .and_then(Value::as_array)?;
    for header in headers {
        let name = header
            .get("Name")
            .or_else(|| header.get("name"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !name.eq_ignore_ascii_case("X-GitHub-Sender") {
            continue;
        }
        let value = header
            .get("Value")
            .or_else(|| header.get("value"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        if let Some(login) = normalize_github_login(value) {
            return Some(login);
        }
    }
    None
}

fn extract_github_sender_from_bodies(payload: &Value) -> Option<String> {
    for field in ["StrippedTextReply", "TextBody", "HtmlBody"] {
        if let Some(body) = payload.get(field).and_then(Value::as_str) {
            if let Some(login) = extract_github_sender_from_text(body) {
                return Some(login);
            }
        }
    }
    None
}

fn extract_github_sender_from_text(text: &str) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(login) = extract_login_from_activity_line(trimmed) {
            return Some(login);
        }

        if let Some(login) = extract_login_from_html_activity_line(trimmed) {
            return Some(login);
        }
    }

    None
}

fn extract_login_from_activity_line(line: &str) -> Option<String> {
    let (candidate, rest) = line.split_once(char::is_whitespace)?;
    let rest = rest.trim_start().to_ascii_lowercase();
    let activity_prefixes = [
        "left a comment",
        "created an issue",
        "opened a pull request",
        "opened an issue",
        "closed an issue",
        "reopened an issue",
        "reviewed",
        "requested a review",
    ];
    if activity_prefixes
        .iter()
        .any(|prefix| rest.starts_with(prefix))
    {
        return normalize_github_login(candidate);
    }
    None
}

fn extract_login_from_html_activity_line(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let start_idx = lower.find("<strong>")?;
    let after_start = &line[start_idx + "<strong>".len()..];
    let after_start_lower = after_start.to_ascii_lowercase();
    let end_idx = after_start_lower.find("</strong>")?;
    let candidate = &after_start[..end_idx];
    let rest = after_start[end_idx + "</strong>".len()..]
        .trim_start()
        .to_ascii_lowercase();
    let activity_prefixes = [
        "left a comment",
        "created an issue",
        "opened a pull request",
        "opened an issue",
    ];
    if activity_prefixes
        .iter()
        .any(|prefix| rest.starts_with(prefix))
    {
        return normalize_github_login(candidate);
    }
    None
}

fn normalize_github_login(raw: &str) -> Option<String> {
    let trimmed = raw
        .trim()
        .trim_start_matches('@')
        .trim_matches(|ch: char| matches!(ch, '"' | '\'' | '<' | '>' | '`'));
    if trimmed.is_empty() {
        return None;
    }

    let lower = trimmed.to_ascii_lowercase();
    let (base, bot_suffix) = if lower.ends_with("[bot]") {
        (&lower[..lower.len() - "[bot]".len()], true)
    } else {
        (lower.as_str(), false)
    };
    if base.is_empty() || base.len() > 39 {
        return None;
    }
    let mut chars = base.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphanumeric() {
        return None;
    }
    if chars.any(|ch| !(ch.is_ascii_alphanumeric() || ch == '-')) {
        return None;
    }
    if base.ends_with('-') {
        return None;
    }
    if bot_suffix {
        Some(format!("{base}[bot]"))
    } else {
        Some(base.to_string())
    }
}

fn build_guidance_section(workspace_dir: &Path, runner: &str) -> String {
    let mut blocks = Vec::new();

    if let Some(content) = load_optional_text(&workspace_dir.join("SOUL.md")) {
        blocks.push(format_guidance_block("SOUL.md", &content));
    }
    if let Some(content) = load_optional_text(&workspace_dir.join("AGENTS.md")) {
        blocks.push(format_guidance_block("AGENTS.md", &content));
    }
    if runner.eq_ignore_ascii_case("claude") {
        if let Some(content) = load_optional_text(&workspace_dir.join("CLAUDE.md")) {
            blocks.push(format_guidance_block("CLAUDE.md", &content));
        }
    }

    if blocks.is_empty() {
        "- (no employee guidance files found)\n".to_string()
    } else {
        blocks.join("\n")
    }
}

fn load_optional_text(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn format_guidance_block(label: &str, content: &str) -> String {
    format!("{label}:\n```\n{content}\n```\n")
}

fn build_discord_context_section(workspace_dir: &Path) -> String {
    let path = workspace_dir
        .join("discord_context")
        .join("context_for_agent.md");
    let fallback_path = workspace_dir
        .join("incoming_email")
        .join("discord_context_for_agent.md");
    let content = fs::read_to_string(&path)
        .or_else(|_| fs::read_to_string(&fallback_path))
        .ok();
    let Some(content) = content else {
        return String::new();
    };
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let max_chars = 12_000usize;
    let mut clipped: String = trimmed.chars().take(max_chars).collect();
    if trimmed.chars().count() > max_chars {
        clipped.push_str("\n\n(Truncated. Use discord_context/thread_full.json, discord_context/thread_full.txt, discord_context/channel_history_full.json, and discord_context/channel_history_full.txt for complete context.)");
    }
    format!(
        "Discord context snapshot (auto-generated; full history is stored in local files):\n```markdown\n{}\n```\n",
        clipped
    )
}

pub(super) fn load_memory_context(
    workspace_dir: &Path,
    memory_dir: &Path,
) -> Result<String, RunTaskError> {
    let resolved = resolve_rel_dir(workspace_dir, memory_dir, "memory_dir")?;
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(&resolved)? {
        let entry = entry?;
        if entry.file_type()?.is_file() && is_markdown_file(&entry.path()) {
            files.push(entry.path());
        }
    }
    files.sort_by(|left, right| left.file_name().cmp(&right.file_name()));

    let mut sections = Vec::new();
    for path in files {
        let content = fs::read_to_string(&path)?;
        let rel_path = path.strip_prefix(workspace_dir).unwrap_or(&path);
        sections.push(format!(
            "--- {path} ---\n{content}",
            path = rel_path.display(),
            content = content.trim_end()
        ));
    }
    Ok(sections.join("\n\n"))
}

fn is_markdown_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "md" | "markdown"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_memory_context_sorts_and_includes_markdown() {
        let temp = TempDir::new().expect("tempdir");
        let workspace = temp.path().join("workspace");
        let memory_dir = workspace.join("memory");
        fs::create_dir_all(&memory_dir).expect("memory dir");
        fs::write(memory_dir.join("b.md"), "second").expect("b.md");
        fs::write(memory_dir.join("a.md"), "first").expect("a.md");
        fs::write(memory_dir.join("note.txt"), "ignore").expect("note.txt");

        let context = load_memory_context(&workspace, Path::new("memory")).expect("context");

        let first_idx = context.find("--- memory/a.md ---").expect("a.md marker");
        let second_idx = context.find("--- memory/b.md ---").expect("b.md marker");
        assert!(first_idx < second_idx, "expected a.md before b.md");
        assert!(context.contains("first"));
        assert!(context.contains("second"));
        assert!(!context.contains("note.txt"));
    }

    #[test]
    fn build_prompt_includes_memory_policy_and_section() {
        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            Path::new("."),
            "codex",
            "--- memory/memo.md ---\nHello",
            true,
            "email",
            true, // has_unified_account
            &UserIdentities::default(),
        );

        assert!(prompt.contains("Memory context"));
        assert!(prompt.contains("memory/memo.md"));
        assert!(prompt.contains("Memory management"));
        assert!(prompt.contains("memo.md"));
        assert!(prompt.contains("500 lines"));
    }

    #[test]
    fn build_prompt_skips_reply_instruction_for_non_replyable() {
        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            Path::new("."),
            "codex",
            "",
            false,
            "email",
            true, // has_unified_account
            &UserIdentities::default(),
        );

        assert!(prompt.contains("non-replyable"));
        assert!(!prompt.contains("write a proper HTML email draft"));
    }

    #[test]
    fn build_prompt_includes_registration_notice_for_unregistered_user() {
        let temp = TempDir::new().expect("tempdir");
        let workspace = temp.path();

        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            workspace,
            "codex",
            "",
            true,
            "email",
            false, // has_unified_account = false
            &UserIdentities::default(),
        );

        assert!(prompt.contains("Account Registration Notice"));
        assert!(prompt.contains("www.dowhiz.com/auth/index.html"));

        // Second call should NOT include the notice (already prompted)
        let prompt2 = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            workspace,
            "codex",
            "",
            true,
            "email",
            false,
            &UserIdentities::default(),
        );

        assert!(!prompt2.contains("Account Registration Notice"));
    }

    #[test]
    fn build_prompt_includes_discord_context_snapshot_when_available() {
        let temp = TempDir::new().expect("tempdir");
        let workspace = temp.path();
        let context_dir = workspace.join("discord_context");
        fs::create_dir_all(&context_dir).expect("discord_context");
        fs::write(
            context_dir.join("context_for_agent.md"),
            "# Discord Context Snapshot\nQuoted + thread context",
        )
        .expect("context file");

        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            workspace,
            "codex",
            "",
            true,
            "discord",
            true,
            &UserIdentities::default(),
        );

        assert!(prompt.contains("Discord context snapshot (auto-generated"));
        assert!(prompt.contains("Quoted + thread context"));
    }

    #[test]
    fn build_prompt_includes_scoped_discord_history_skill_when_scope_file_exists() {
        let temp = TempDir::new().expect("tempdir");
        let workspace = temp.path();
        fs::write(workspace.join(".chat_history_scope.json"), "{}\n").expect("scope file");

        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            workspace,
            "codex",
            "",
            true,
            "discord",
            true,
            &UserIdentities::default(),
        );

        assert!(prompt.contains("Scoped chat history search"));
        assert!(prompt.contains(".agents/skills/discord-history-search/SKILL.md"));
        assert!(prompt.contains("current Discord server"));
        assert!(prompt.contains("cannot cross into other Discord servers"));
    }

    #[test]
    fn build_prompt_includes_scoped_slack_history_skill_when_scope_file_exists() {
        let temp = TempDir::new().expect("tempdir");
        let workspace = temp.path();
        fs::write(workspace.join(".chat_history_scope.json"), "{}\n").expect("scope file");

        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            workspace,
            "codex",
            "",
            true,
            "slack",
            true,
            &UserIdentities::default(),
        );

        assert!(prompt.contains("Scoped chat history search"));
        assert!(prompt.contains(".agents/skills/slack-history-search/SKILL.md"));
        assert!(prompt.contains("current Slack workspace/team"));
        assert!(prompt.contains("cannot cross into other Slack workspaces"));
    }

    #[test]
    fn build_prompt_includes_github_coauthor_guidance_when_sender_detected() {
        let temp = TempDir::new().expect("tempdir");
        let workspace = temp.path();
        let incoming_dir = workspace.join("incoming_email");
        fs::create_dir_all(&incoming_dir).expect("incoming_email");
        fs::write(
            incoming_dir.join("postmark_payload.json"),
            r#"{
  "From": "Bingran You <notifications@github.com>",
  "Headers": [{"Name":"X-GitHub-Sender","Value":"bingran-you"}]
}"#,
        )
        .expect("postmark payload");

        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            workspace,
            "codex",
            "",
            true,
            "email",
            true,
            &UserIdentities::default(),
        );

        assert!(prompt.contains("GitHub Attribution Requirement"));
        assert!(
            prompt.contains("Co-authored-by: bingran-you <bingran-you@users.noreply.github.com>")
        );
        assert!(prompt.contains("Requested-by: @bingran-you"));
    }

    #[test]
    fn build_prompt_omits_github_coauthor_guidance_for_non_github_email() {
        let temp = TempDir::new().expect("tempdir");
        let workspace = temp.path();
        let incoming_dir = workspace.join("incoming_email");
        fs::create_dir_all(&incoming_dir).expect("incoming_email");
        fs::write(
            incoming_dir.join("postmark_payload.json"),
            r#"{
  "From": "Alice <alice@example.com>",
  "TextBody": "hello"
}"#,
        )
        .expect("postmark payload");

        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            workspace,
            "codex",
            "",
            true,
            "email",
            true,
            &UserIdentities::default(),
        );

        assert!(!prompt.contains("GitHub Attribution Requirement"));
        assert!(!prompt.contains("Co-authored-by:"));
    }

    #[test]
    fn build_user_identities_section_shows_not_available_for_default() {
        let identities = UserIdentities::default();
        let section = build_user_identities_section(&identities);
        assert!(section.contains("Not available"));
        assert!(section.contains("no linked DoWhiz account"));
    }

    #[test]
    fn build_user_identities_section_includes_account_id() {
        let identities = UserIdentities {
            account_id: Some("test-account-123".to_string()),
            ..Default::default()
        };
        let section = build_user_identities_section(&identities);
        assert!(section.contains("DoWhiz Account ID: test-account-123"));
        assert!(section.contains("User Context"));
    }

    #[test]
    fn build_user_identities_section_includes_wechat_mp_account_ids() {
        let identities = UserIdentities {
            wechat_mp_account_ids: vec!["gh_mp_account_123".to_string()],
            ..Default::default()
        };
        let section = build_user_identities_section(&identities);
        assert!(section.contains("WeChat MP Account IDs: gh_mp_account_123"));
    }

    #[test]
    fn build_user_identities_section_includes_all_channels() {
        let identities = UserIdentities {
            account_id: Some("acct-123".to_string()),
            emails: vec!["user@example.com".to_string()],
            slack_user_ids: vec!["U123456".to_string()],
            discord_user_ids: vec!["987654321".to_string()],
            phone_numbers: vec!["+15551234567".to_string()],
            telegram_user_ids: vec!["12345678".to_string()],
            lark_user_ids: vec![],
            wechat_user_ids: vec![],
            wechat_mp_open_ids: vec!["oMpOpenId123".to_string()],
            wechat_mp_account_ids: vec!["gh_mp_account_789".to_string()],
            zoom_user_ids: vec![],
            github_usernames: vec![],
            allowed_user_ids: vec![],
            organization_id: None,
            organization_name: None,
            notion_database_id: None,
        };
        let section = build_user_identities_section(&identities);

        assert!(section.contains("Email: user@example.com"));
        assert!(section.contains("Slack User IDs: U123456"));
        assert!(section.contains("Discord User IDs: 987654321"));
        assert!(section.contains("Phone Numbers: +15551234567"));
        assert!(section.contains("Telegram User IDs: 12345678"));
        assert!(section.contains("WeChat MP Open IDs: oMpOpenId123"));
        assert!(section.contains("WeChat MP Account IDs: gh_mp_account_789"));
    }

    #[test]
    fn build_user_identities_section_includes_routing_instructions() {
        let identities = UserIdentities {
            account_id: Some("acct-123".to_string()),
            ..Default::default()
        };
        let section = build_user_identities_section(&identities);

        assert!(section.contains("reply_routing.json"));
        assert!(section.contains("IMPORTANT: When using cross-channel routing"));
        assert!(section.contains("email target: reply_email_draft.html"));
        assert!(section.contains("discord target: reply_message.txt"));
        assert!(section.contains("\"wechat_mp\""));
    }

    #[test]
    fn build_prompt_includes_user_identities_when_present() {
        let temp = TempDir::new().expect("tempdir");
        let identities = UserIdentities {
            account_id: Some("test-acct".to_string()),
            emails: vec!["test@example.com".to_string()],
            discord_user_ids: vec!["123456789".to_string()],
            ..Default::default()
        };

        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            temp.path(),
            "codex",
            "",
            true,
            "email",
            true,
            &identities,
        );

        assert!(prompt.contains("User Context"));
        assert!(prompt.contains("test@example.com"));
        assert!(prompt.contains("123456789"));
        assert!(prompt.contains("reply_routing.json"));
    }

    #[test]
    fn build_prompt_includes_cross_channel_capabilities() {
        let temp = TempDir::new().expect("tempdir");

        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            temp.path(),
            "codex",
            "",
            true,
            "email",
            true,
            &UserIdentities::default(),
        );

        // Verify cross-channel tools section is included
        assert!(prompt.contains("Cross-channel Tools"));
        assert!(prompt.contains("`gws`"));
        assert!(prompt.contains("google-docs"));
        assert!(prompt.contains("google-slides"));
        assert!(prompt.contains("google-sheets"));
        assert!(prompt.contains("Discord Bot Tools"));
        assert!(prompt.contains("discord_cli"));

        // Verify security note and SKILL.md reference
        assert!(prompt.contains("CURRENT USER"));
        assert!(prompt.contains("SKILL.md"));
        assert!(prompt.contains("/app/.cache/ms-playwright/*/chrome-linux*/chrome"));
        assert!(prompt.contains("Never include raw credentials"));
        assert!(prompt.contains("persistent remote browser context"));
        assert!(prompt.contains("single browser tab"));
    }

    #[test]
    fn build_prompt_with_fast_completion_prioritizes_recovery_reply() {
        let temp = TempDir::new().expect("tempdir");

        let prompt = build_prompt_with_fast_completion(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            temp.path(),
            "claude",
            "",
            true,
            "email",
            true,
            &UserIdentities::default(),
            true,
        );

        assert!(prompt.contains("Recovery-mode override for email replies"));
        assert!(prompt.contains(".codex_remote_output.log"));
        assert!(prompt.contains(".run_task_trace_codex_primary/"));
        assert!(prompt.contains("Do NOT start new PDFs"));
        assert!(prompt.contains("send an honest limitation / next-steps reply"));
    }

    #[test]
    fn build_prompt_includes_human_approval_gate_instructions() {
        let temp = TempDir::new().expect("tempdir");

        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            temp.path(),
            "codex",
            "",
            true,
            "email",
            true,
            &UserIdentities::default(),
        );

        assert!(prompt.contains("Human Approval Gate"));
        assert!(prompt.contains("dowhiz_human_approval_gate_request_and_wait"));
        assert!(prompt.contains("scope=\"admin\""));
        assert!(prompt.contains("scope=\"user\""));
        assert!(prompt.contains("challenge_type=\"captcha\""));
        assert!(prompt.contains("challenge_type=\"password\""));
        assert!(prompt.contains("challenge_type=\"two_factor\""));
        assert!(prompt.contains("screenshot path(s)"));
        assert!(prompt.contains("GOOGLE_PASSWORD"));
        assert!(prompt.contains("printenv GOOGLE_PASSWORD"));
        assert!(prompt.contains("timeout"));
        assert!(prompt.contains("do NOT attempt to solve it yourself"));
        assert!(prompt.contains("required credential"));
        assert!(prompt.contains("prefer SMS verification first by default"));
        assert!(prompt.contains("click the button that sends the code"));
        assert!(prompt.contains("waiting_for_code_input"));
        assert!(prompt.contains("waiting_for_device_approval"));
        assert!(prompt.contains("Never claim a code was sent"));
        assert!(prompt.contains("live browser handoff link"));
        assert!(prompt.contains(".human_approval_gate/events.jsonl"));
        assert!(prompt.contains("HAG_EVENT"));
        assert!(prompt.contains("status is `replied`"));
        assert!(prompt.contains("returned `reply` payload"));
        assert!(prompt.contains("dowhiz@deep-tutor.com"));
        assert!(prompt.contains("oliver@dowhiz.com"));
        assert!(prompt.contains("do NOT use the shell CLI `human_approval_gate`"));
    }

    #[test]
    fn build_allowed_paths_section_empty_returns_workspace_only() {
        let section = build_allowed_paths_section(&[]);
        assert!(section.contains("Filesystem Security"));
        assert!(section.contains("ONLY access files within your current workspace directory"));
        assert!(section.contains("Do NOT traverse to parent directories"));
    }

    #[test]
    fn build_allowed_paths_section_with_user_ids_lists_paths() {
        let user_ids = vec![
            "550e8400-e29b-41d4-a716-446655440000".to_string(),
            "660f9500-f39c-52e5-b827-557766551111".to_string(),
        ];
        let section = build_allowed_paths_section(&user_ids);

        assert!(section.contains("Filesystem Security"));
        assert!(section.contains("/users/550e8400-e29b-41d4-a716-446655440000/"));
        assert!(section.contains("/users/660f9500-f39c-52e5-b827-557766551111/"));
        assert!(section.contains("Do NOT access any paths outside these directories"));
        assert!(section.contains("Do NOT attempt to access other users' data"));
    }

    #[test]
    fn build_prompt_includes_filesystem_security_section() {
        let temp = TempDir::new().expect("tempdir");
        let identities = UserIdentities {
            allowed_user_ids: vec!["test-user-uuid".to_string()],
            ..Default::default()
        };

        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            temp.path(),
            "codex",
            "",
            true,
            "email",
            false,
            &identities,
        );

        assert!(prompt.contains("Filesystem Security"));
        assert!(prompt.contains("/users/test-user-uuid/"));
    }

    #[test]
    fn build_prompt_includes_workspace_only_security_when_no_user_ids() {
        let temp = TempDir::new().expect("tempdir");
        let identities = UserIdentities::default();

        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            temp.path(),
            "codex",
            "",
            true,
            "email",
            false,
            &identities,
        );

        assert!(prompt.contains("Filesystem Security"));
        assert!(prompt.contains("ONLY access files within your current workspace directory"));
    }

    // ==================== Linked vs Unlinked Account Tests ====================

    #[test]
    fn unlinked_account_gets_workspace_only_restriction() {
        // User without a unified account: no account_id, no allowed_user_ids
        let temp = TempDir::new().expect("tempdir");
        let identities = UserIdentities {
            account_id: None,
            emails: vec![],
            slack_user_ids: vec![],
            discord_user_ids: vec![],
            phone_numbers: vec![],
            telegram_user_ids: vec![],
            lark_user_ids: vec![],
            wechat_user_ids: vec![],
            wechat_mp_open_ids: vec![],
            wechat_mp_account_ids: vec![],
            zoom_user_ids: vec![],
            github_usernames: vec![],
            allowed_user_ids: vec![],
            organization_id: None,
            organization_name: None,
            notion_database_id: None,
        };

        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            temp.path(),
            "codex",
            "",
            true,
            "email",
            false, // has_unified_account = false
            &identities,
        );

        // Should have workspace-only restriction
        assert!(prompt.contains("ONLY access files within your current workspace directory"));
        assert!(prompt.contains("Do NOT traverse to parent directories"));
        // Should NOT mention /users/ paths
        assert!(!prompt.contains("/users/"));
    }

    #[test]
    fn linked_account_single_channel_gets_one_user_path() {
        // User with unified account, single channel (e.g., just email)
        let temp = TempDir::new().expect("tempdir");
        let user_uuid = "abc12345-def6-7890-abcd-ef1234567890";
        let identities = UserIdentities {
            account_id: Some("unified-account-123".to_string()),
            emails: vec!["user@example.com".to_string()],
            slack_user_ids: vec![],
            discord_user_ids: vec![],
            phone_numbers: vec![],
            telegram_user_ids: vec![],
            lark_user_ids: vec![],
            wechat_user_ids: vec![],
            wechat_mp_open_ids: vec![],
            wechat_mp_account_ids: vec![],
            zoom_user_ids: vec![],
            github_usernames: vec![],
            allowed_user_ids: vec![user_uuid.to_string()],
            organization_id: None,
            organization_name: None,
            notion_database_id: None,
        };

        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            temp.path(),
            "codex",
            "",
            true,
            "email",
            true, // has_unified_account = true
            &identities,
        );

        // Should have the specific user path
        assert!(prompt.contains(&format!("/users/{}/", user_uuid)));
        // Should have the multi-directory restriction message
        assert!(prompt.contains("ONLY access files within the following user directories"));
        assert!(prompt.contains("Do NOT access any paths outside these directories"));
        // Should NOT have workspace-only message
        assert!(!prompt.contains("ONLY access files within your current workspace directory"));
    }

    #[test]
    fn linked_account_multiple_channels_gets_all_user_paths() {
        // User with unified account, multiple channels (email + slack + discord)
        let temp = TempDir::new().expect("tempdir");
        let email_uuid = "email-uuid-1111-2222-333344445555";
        let slack_uuid = "slack-uuid-6666-7777-888899990000";
        let discord_uuid = "discord-uuid-aaaa-bbbb-ccccddddeeee";

        let identities = UserIdentities {
            account_id: Some("unified-account-456".to_string()),
            emails: vec!["user@example.com".to_string()],
            slack_user_ids: vec!["U12345678".to_string()],
            discord_user_ids: vec!["987654321012345678".to_string()],
            phone_numbers: vec![],
            telegram_user_ids: vec![],
            lark_user_ids: vec![],
            wechat_user_ids: vec![],
            wechat_mp_open_ids: vec![],
            wechat_mp_account_ids: vec![],
            zoom_user_ids: vec![],
            github_usernames: vec![],
            allowed_user_ids: vec![
                email_uuid.to_string(),
                slack_uuid.to_string(),
                discord_uuid.to_string(),
            ],
            organization_id: None,
            organization_name: None,
            notion_database_id: None,
        };

        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            temp.path(),
            "codex",
            "",
            true,
            "email",
            true,
            &identities,
        );

        // Should have all user paths
        assert!(prompt.contains(&format!("/users/{}/", email_uuid)));
        assert!(prompt.contains(&format!("/users/{}/", slack_uuid)));
        assert!(prompt.contains(&format!("/users/{}/", discord_uuid)));
        // Should have the restriction message
        assert!(prompt.contains("Do NOT attempt to access other users' data"));
    }

    #[test]
    fn linked_account_with_account_id_but_no_user_ids_falls_back_to_workspace() {
        // Edge case: has account_id but allowed_user_ids is empty
        // (could happen if UserStore lookup fails)
        let temp = TempDir::new().expect("tempdir");
        let identities = UserIdentities {
            account_id: Some("unified-account-789".to_string()),
            emails: vec!["user@example.com".to_string()],
            slack_user_ids: vec![],
            discord_user_ids: vec![],
            phone_numbers: vec![],
            telegram_user_ids: vec![],
            lark_user_ids: vec![],
            wechat_user_ids: vec![],
            wechat_mp_open_ids: vec![],
            wechat_mp_account_ids: vec![],
            zoom_user_ids: vec![],
            github_usernames: vec![],
            allowed_user_ids: vec![], // Empty even though account exists
            organization_id: None,
            organization_name: None,
            notion_database_id: None,
        };

        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            temp.path(),
            "codex",
            "",
            true,
            "email",
            true, // has_unified_account = true
            &identities,
        );

        // Should fall back to workspace-only restriction
        assert!(prompt.contains("ONLY access files within your current workspace directory"));
        // Should NOT mention /users/ paths
        assert!(!prompt.contains("/users/"));
    }

    #[test]
    fn security_section_appears_after_rules_section() {
        let temp = TempDir::new().expect("tempdir");
        let identities = UserIdentities::default();

        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            temp.path(),
            "codex",
            "",
            true,
            "email",
            false,
            &identities,
        );

        // Security section should appear after the rules
        let rules_pos = prompt
            .find("Avoid interactive commands")
            .expect("rules section");
        let security_pos = prompt
            .find("Filesystem Security")
            .expect("security section");
        assert!(
            security_pos > rules_pos,
            "Security section should appear after rules"
        );
    }

    #[test]
    fn unlinked_account_does_not_get_cross_channel_routing() {
        let temp = TempDir::new().expect("tempdir");
        let identities = UserIdentities::default();

        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            temp.path(),
            "codex",
            "",
            true,
            "email",
            false,
            &identities,
        );

        // Should mention that cross-channel is not available
        assert!(prompt.contains("no linked DoWhiz account"));
        // But should still have filesystem security
        assert!(prompt.contains("Filesystem Security"));
    }

    #[test]
    fn linked_account_gets_both_cross_channel_and_security() {
        let temp = TempDir::new().expect("tempdir");
        let identities = UserIdentities {
            account_id: Some("test-account".to_string()),
            emails: vec!["user@example.com".to_string()],
            allowed_user_ids: vec!["user-uuid-1234".to_string()],
            ..Default::default()
        };

        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            temp.path(),
            "codex",
            "",
            true,
            "email",
            true,
            &identities,
        );

        // Should have user context and cross-channel routing info
        assert!(prompt.contains("User Context"));
        assert!(prompt.contains("reply_routing.json"));
        // And filesystem security with user path
        assert!(prompt.contains("Filesystem Security"));
        assert!(prompt.contains("/users/user-uuid-1234/"));
    }

    // ==================== Production Flow Simulation Tests ====================
    // These tests simulate what happens in production when:
    // 1. AccountStore returns identifiers for a unified account
    // 2. UserStore maps each identifier to a filesystem user_id
    // 3. The prompt is generated with all allowed paths

    #[test]
    fn production_flow_email_only_account() {
        // Simulates: User signed up via email only, no other channels linked
        // AccountStore returns: [email: "alice@example.com"]
        // UserStore returns: email -> "uuid-email-alice"
        let temp = TempDir::new().expect("tempdir");

        let identities = UserIdentities {
            account_id: Some("acct-alice-123".to_string()),
            emails: vec!["alice@example.com".to_string()],
            slack_user_ids: vec![],
            discord_user_ids: vec![],
            phone_numbers: vec![],
            telegram_user_ids: vec![],
            lark_user_ids: vec![],
            wechat_user_ids: vec![],
            wechat_mp_open_ids: vec![],
            wechat_mp_account_ids: vec![],
            zoom_user_ids: vec![],
            github_usernames: vec![],
            allowed_user_ids: vec!["uuid-email-alice".to_string()],
            organization_id: None,
            organization_name: None,
            notion_database_id: None,
        };

        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            temp.path(),
            "codex",
            "",
            true,
            "email",
            true,
            &identities,
        );

        // Codex should see exactly one allowed path
        assert!(prompt.contains("/users/uuid-email-alice/"));
        // And the security instruction
        assert!(prompt.contains("ONLY access files within the following user directories"));
        // Cross-channel should show email is available
        assert!(prompt.contains("Email: alice@example.com"));
    }

    #[test]
    fn production_flow_multi_channel_account() {
        // Simulates: User linked email, Slack, Discord, and phone
        // AccountStore returns all 4 identifiers
        // UserStore returns a unique user_id for each
        let temp = TempDir::new().expect("tempdir");

        let identities = UserIdentities {
            account_id: Some("acct-bob-456".to_string()),
            emails: vec!["bob@company.com".to_string()],
            slack_user_ids: vec!["U0BOB12345".to_string()],
            discord_user_ids: vec!["123456789012345678".to_string()],
            phone_numbers: vec!["+15551234567".to_string()],
            telegram_user_ids: vec![],
            lark_user_ids: vec![],
            wechat_user_ids: vec![],
            wechat_mp_open_ids: vec![],
            wechat_mp_account_ids: vec![],
            zoom_user_ids: vec![],
            github_usernames: vec![],
            // Each channel has its own filesystem user directory
            allowed_user_ids: vec![
                "uuid-email-bob".to_string(),
                "uuid-slack-bob".to_string(),
                "uuid-discord-bob".to_string(),
                "uuid-phone-bob".to_string(),
            ],
            organization_id: None,
            organization_name: None,
            notion_database_id: None,
        };

        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            temp.path(),
            "codex",
            "",
            true,
            "email", // inbound channel is email
            true,
            &identities,
        );

        // Codex should see ALL four allowed paths
        assert!(prompt.contains("/users/uuid-email-bob/"));
        assert!(prompt.contains("/users/uuid-slack-bob/"));
        assert!(prompt.contains("/users/uuid-discord-bob/"));
        assert!(prompt.contains("/users/uuid-phone-bob/"));

        // Count occurrences - should be exactly 4 user directories listed
        let user_path_count = prompt.matches("/users/").count();
        assert_eq!(user_path_count, 4, "Should have exactly 4 /users/ paths");

        // Cross-channel routing should list all channels
        assert!(prompt.contains("Email: bob@company.com"));
        assert!(prompt.contains("Slack User IDs: U0BOB12345"));
        assert!(prompt.contains("Discord User IDs: 123456789012345678"));
        assert!(prompt.contains("Phone Numbers: +15551234567"));
    }

    #[test]
    fn production_flow_same_user_id_for_multiple_channels() {
        // Edge case: Two identifiers map to the same user_id
        // (Could happen if user used same email for Slack integration)
        let temp = TempDir::new().expect("tempdir");

        let identities = UserIdentities {
            account_id: Some("acct-charlie-789".to_string()),
            emails: vec!["charlie@example.com".to_string()],
            slack_user_ids: vec!["UCHARLIE123".to_string()],
            discord_user_ids: vec![],
            phone_numbers: vec![],
            telegram_user_ids: vec![],
            lark_user_ids: vec![],
            wechat_user_ids: vec![],
            wechat_mp_open_ids: vec![],
            wechat_mp_account_ids: vec![],
            zoom_user_ids: vec![],
            github_usernames: vec![],
            // In production, identifiers_to_user_identities deduplicates
            // So if email and slack both map to same user_id, only one entry
            allowed_user_ids: vec!["uuid-charlie-shared".to_string()],
            organization_id: None,
            organization_name: None,
            notion_database_id: None,
        };

        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            temp.path(),
            "codex",
            "",
            true,
            "email",
            true,
            &identities,
        );

        // Should only list the path once (deduplicated)
        let user_path_count = prompt.matches("/users/uuid-charlie-shared/").count();
        assert_eq!(
            user_path_count, 1,
            "Deduplicated user_id should appear once"
        );
    }

    #[test]
    fn production_flow_inbound_via_slack() {
        // Simulates: Message came in via Slack, not email
        let temp = TempDir::new().expect("tempdir");

        let identities = UserIdentities {
            account_id: Some("acct-dave-000".to_string()),
            emails: vec!["dave@example.com".to_string()],
            slack_user_ids: vec!["UDAVE99999".to_string()],
            discord_user_ids: vec![],
            phone_numbers: vec![],
            telegram_user_ids: vec![],
            lark_user_ids: vec![],
            wechat_user_ids: vec![],
            wechat_mp_open_ids: vec![],
            wechat_mp_account_ids: vec![],
            zoom_user_ids: vec![],
            github_usernames: vec![],
            allowed_user_ids: vec!["uuid-email-dave".to_string(), "uuid-slack-dave".to_string()],
            organization_id: None,
            organization_name: None,
            notion_database_id: None,
        };

        let prompt = build_prompt(
            Path::new("incoming_email"), // Still "incoming_email" dir name
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            temp.path(),
            "codex",
            "",
            true,
            "slack", // <-- Inbound channel is Slack
            true,
            &identities,
        );

        // Should still have both paths - channel doesn't affect security
        assert!(prompt.contains("/users/uuid-email-dave/"));
        assert!(prompt.contains("/users/uuid-slack-dave/"));

        // Reply instruction should be Slack-specific
        assert!(prompt.contains("reply_message.txt"));
        assert!(prompt.contains("Slack mrkdwn"));
    }

    #[test]
    fn codex_sees_exact_security_message_for_linked_account() {
        // Verify the EXACT wording Codex sees
        let temp = TempDir::new().expect("tempdir");

        let identities = UserIdentities {
            account_id: Some("test-acct".to_string()),
            allowed_user_ids: vec!["first-uuid".to_string(), "second-uuid".to_string()],
            ..Default::default()
        };

        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            temp.path(),
            "codex",
            "",
            true,
            "email",
            true,
            &identities,
        );

        // Check exact phrases Codex will see
        assert!(prompt.contains("Filesystem Security:"));
        assert!(prompt.contains("You may ONLY access files within the following user directories:"));
        assert!(prompt.contains("  - /users/first-uuid/"));
        assert!(prompt.contains("  - /users/second-uuid/"));
        assert!(prompt.contains("Do NOT access any paths outside these directories."));
        assert!(prompt.contains("Do NOT attempt to access other users' data."));
    }

    #[test]
    fn codex_sees_exact_security_message_for_unlinked_account() {
        // Verify the EXACT wording Codex sees for unlinked accounts
        let temp = TempDir::new().expect("tempdir");
        let identities = UserIdentities::default();

        let prompt = build_prompt(
            Path::new("incoming_email"),
            Path::new("incoming_attachments"),
            Path::new("memory"),
            Path::new("references"),
            temp.path(),
            "codex",
            "",
            true,
            "email",
            false,
            &identities,
        );

        // Check exact phrases
        assert!(prompt.contains("Filesystem Security:"));
        assert!(
            prompt.contains("You may ONLY access files within your current workspace directory.")
        );
        assert!(prompt.contains(
            "Do NOT traverse to parent directories or access paths outside this workspace."
        ));
    }
}
