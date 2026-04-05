# Notion API CLI Skill

Use `notion_api_cli` for ALL Notion operations. NEVER use browser automation for Notion.

## Authentication

Before using any command, ensure the Notion API token is available:

```bash
# Try loading from workspace .notion_env first
source .notion_env 2>/dev/null || true

# Check if token is available
if [ -z "$NOTION_API_TOKEN" ]; then
  echo "No Notion integration available. User needs to link Notion at dowhiz.com"
  exit 1
fi
```

## Available Commands

### Read Page Content
```bash
notion_api_cli read-page <page_id>
```
Returns the page content as text.

### Get Comments
```bash
notion_api_cli get-comments <page_id>
```
Returns all comments on a page. Useful for reading @mentions or discussions.

### Create Comment
```bash
notion_api_cli create-comment <page_id> "Your message here"
```
Creates a new top-level comment on a page.

### Reply to Comment
```bash
notion_api_cli reply <comment_id> "Your reply here"
```
Replies to an existing comment thread.

### Search Pages
```bash
notion_api_cli search "query string"
```
Searches for pages matching the query. Returns page IDs and titles.

### Create Page
```bash
notion_api_cli create-page --parent-id <parent_page_id> --title "Page Title"
```
Creates a new page under the specified parent page.

### Update Page
```bash
notion_api_cli update-page <page_id> --property "PropertyName=Value"
```
Updates page properties.

## Common Workflows

### Create a page and reply to user
```bash
# 1. Check for token
source .notion_env 2>/dev/null || true
if [ -z "$NOTION_API_TOKEN" ]; then
  echo "Please link your Notion workspace at dowhiz.com first"
  exit 0
fi

# 2. Search for a parent page (optional)
notion_api_cli search "Projects"

# 3. Create the page
notion_api_cli create-page --parent-id <found_page_id> --title "My New Page"

# 4. Include the link in your reply email/message
```

### Read and respond to @mention
```bash
# 1. Load context (if available)
source .notion_env

# 2. Get page_id from .notion_context.json (if this is a Notion @mention task)
PAGE_ID=$(jq -r '.page_id' .notion_context.json 2>/dev/null)

# 3. Read comments to understand the request
notion_api_cli get-comments "$PAGE_ID"

# 4. Post your reply
notion_api_cli create-comment "$PAGE_ID" "Here is my response..."

# 5. Mark as complete (if this is a Notion channel task)
touch .notion_api_replied
```

## Important Notes

1. **NEVER use browser automation** - The API is faster, more reliable, and doesn't require user login
2. **Check for token first** - If no `NOTION_API_TOKEN`, tell the user to link Notion at dowhiz.com
3. **For Notion channel tasks** - Always create `.notion_api_replied` marker after posting a reply
4. **Cross-channel** - When receiving a Notion request via email/Slack/etc., still use `notion_api_cli`

## Troubleshooting

| Issue | Solution |
|-------|----------|
| "unauthorized" error | Token may be expired. Ask user to re-link Notion at dowhiz.com |
| Page not found | User may not have shared the page with the integration |
| No NOTION_API_TOKEN | User hasn't linked Notion integration yet |
