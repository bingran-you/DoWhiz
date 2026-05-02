# DoWhiz Auth API Reference

Base URL: `/auth` and `/api`

All authenticated endpoints require a Supabase JWT token in the `Authorization: Bearer <token>` header.

---

## Account Management

### POST /auth/signup
Create a new account.

**Request:**
```json
{
  "auth_user_id": "uuid"
}
```

**Response 200:**
```json
{
  "account_id": "uuid",
  "auth_user_id": "uuid",
  "created": false,
  "organization_id": "uuid or null",
  "organization_name": "deeptutor or null"
}
```

---

### GET /auth/account
Get current account details and linked identifiers.

**Headers:** `Authorization: Bearer <token>`

**Response 200:**
```json
{
  "account_id": "uuid",
  "auth_user_id": "uuid",
  "identifiers": [
    {
      "identifier_type": "email",
      "identifier": "user@example.com",
      "verified": true
    }
  ],
  "tokens_to_hours": 0.5,
  "organization_id": "uuid or null",
  "organization_name": "deeptutor or null"
}
```

---

### DELETE /auth/account
Delete the current account and all associated data.

**Headers:** `Authorization: Bearer <token>`

**Response 200:**
```json
{
  "message": "Account deleted"
}
```

---

## Organization Management

### POST /auth/organization
Create a new organization.

**Headers:** `Authorization: Bearer <token>`

**Request:**
```json
{
  "name": "deeptutor"
}
```

**Response 201:**
```json
{
  "id": "uuid",
  "name": "deeptutor",
  "notion_database_id": null,
  "created_at": "2026-04-14T00:00:00Z"
}
```

**Response 409 (Conflict):**
```json
{
  "error": "Organization 'deeptutor' already exists"
}
```

---

### GET /auth/organizations
List organizations, optionally filtered by search term.

**Headers:** `Authorization: Bearer <token>`

**Query Parameters:**
- `search` (optional) - Case-insensitive partial match on organization name

**Example:** `GET /auth/organizations?search=deep`

**Response 200:**
```json
{
  "organizations": [
    {
      "id": "uuid",
      "name": "deeptutor",
      "notion_database_id": "notion-db-id",
      "created_at": "2026-04-14T00:00:00Z"
    }
  ]
}
```

---

### GET /auth/organization/:name/member-count
Get the number of members in an organization.

**Headers:** `Authorization: Bearer <token>`

**Path Parameters:**
- `name` - Organization name

**Example:** `GET /auth/organization/deeptutor/member-count`

**Response 200:**
```json
{
  "organization_name": "deeptutor",
  "member_count": 3
}
```

**Response 404:**
```json
{
  "error": "Organization 'xyz' not found"
}
```

---

### PUT /auth/organization/:name/database
Update organization's Notion database ID and optionally workspace ID.

**Headers:** `Authorization: Bearer <token>`

**Path Parameters:**
- `name` - Organization name

**Request:**
```json
{
  "database_id": "notion-database-id-or-url",
  "workspace_id": "optional-workspace-id"
}
```

If `workspace_id` is not provided, it will be auto-detected by testing the user's Notion credentials against the database.

**Response 200:**
```json
{
  "success": true,
  "organization_name": "deeptutor",
  "notion_database_id": "abc123",
  "notion_workspace_id": "xyz789"
}
```

---

### PUT /auth/organization/:name/leader
Set the organization's leader account. The leader's Notion credentials are used for all TPM operations.

**Headers:** `Authorization: Bearer <token>`

**Path Parameters:**
- `name` - Organization name

**Request:**
```json
{
  "leader_account_id": "uuid-of-leader-account"
}
```

**Response 200:**
```json
{
  "success": true,
  "organization_name": "deeptutor",
  "leader_account_id": "uuid"
}
```

**Response 403:**
```json
{
  "error": "You are not a member of this organization"
}
```

---

### PUT /auth/account/organization
Join an organization by name.

**Headers:** `Authorization: Bearer <token>`

**Request:**
```json
{
  "organization_name": "deeptutor"
}
```

**Response 200:**
```json
{
  "account_id": "uuid",
  "organization_id": "uuid",
  "organization_name": "deeptutor"
}
```

**Response 404:**
```json
{
  "error": "Organization 'xyz' not found"
}
```

---

### DELETE /auth/account/organization
Leave the current organization.

**Headers:** `Authorization: Bearer <token>`

**Response 200:**
```json
{
  "account_id": "uuid",
  "organization_id": null
}
```

---

### POST /api/tpm/setup-cron
Set up TPM cron job for an organization. Called when first member joins to enable daily TPM syncs.

**Headers:** `Authorization: Bearer <token>`

**Request:**
```json
{
  "organization_name": "deeptutor"
}
```

**Response 200:**
```json
{
  "success": true,
  "task_id": "uuid",
  "user_id": "uuid",
  "organization": "deeptutor",
  "email": "user@example.com",
  "cron": "0 0 9 * * MON-FRI",
  "workspace_dir": "/path/to/workspace"
}
```

**Response 400:**
```json
{
  "error": "You must be a member of an organization to set up TPM cron"
}
```

**Response 403:**
```json
{
  "error": "You are not a member of organization 'xyz'"
}
```

**Response 404:**
```json
{
  "error": "Organization 'xyz' not found"
}
```

---

### POST /api/tpm/trigger-sync
Trigger an immediate TPM sync for an organization. Creates a one-shot task that runs immediately.

**Headers:** `Authorization: Bearer <token>`

**Request:**
```json
{
  "organization_name": "deeptutor"
}
```

**Response 200:**
```json
{
  "success": true,
  "task_id": "uuid",
  "user_id": "uuid",
  "organization": "deeptutor",
  "email": "user@example.com",
  "workspace_dir": "/path/to/workspace"
}
```

**Response 400:**
```json
{
  "error": "You must be a member of an organization to trigger TPM sync"
}
```

**Response 403:**
```json
{
  "error": "You are not a member of organization 'xyz'"
}
```

**Response 404:**
```json
{
  "error": "Organization 'xyz' not found"
}
```

---

## Identifier Linking

### POST /auth/link
Link a new identifier (email, phone, etc.) to the account.

**Headers:** `Authorization: Bearer <token>`

**Request:**
```json
{
  "identifier_type": "email",
  "identifier": "user@example.com"
}
```

**Response 200:**
```json
{
  "id": "uuid",
  "identifier_type": "email",
  "identifier": "user@example.com",
  "verified": false
}
```

---

### POST /auth/verify
**STUB** - Marks identifier as verified without validating code.

**Headers:** `Authorization: Bearer <token>`

**Request:**
```json
{
  "identifier_type": "email",
  "identifier": "user@example.com",
  "code": "123456"
}
```

> Note: This endpoint is unfinished. It currently does not validate the code - just marks as verified.

---

### GET /auth/verify-email
Verify email via token link (from verification email).

**Query Parameters:**
- `token` - Verification token from email

**Response:** Redirects to success/failure page

---

### DELETE /auth/unlink
Remove a linked identifier from the account.

**Headers:** `Authorization: Bearer <token>`

**Request:**
```json
{
  "identifier_type": "email",
  "identifier": "user@example.com"
}
```

---

## Memo (User Preferences)

### GET /auth/memo
Get the user's memo/preferences.

**Headers:** `Authorization: Bearer <token>`

---

### POST /auth/memo
Update the user's memo/preferences.

**Headers:** `Authorization: Bearer <token>`

---

## OAuth Flows

All OAuth start endpoints require `Authorization: Bearer <token>` header.

### Discord
- `GET /auth/discord` - Start Discord OAuth flow
- `GET /auth/discord/callback` - OAuth callback
- `GET /auth/discord/bot-callback` - Bot installation callback

### Slack
- `GET /auth/slack` - Start Slack OAuth flow
- `GET /auth/slack/callback` - OAuth callback
- `GET /auth/slack/bot-callback` - Bot installation callback

### GitHub
- `GET /auth/github` - Start GitHub OAuth flow
- `GET /auth/github/callback` - OAuth callback

### Notion
- `GET /auth/notion` - Start Notion OAuth flow
- `GET /auth/notion/callback` - OAuth callback

### Lark
- `GET /auth/lark` - Start Lark OAuth flow
- `GET /auth/lark/callback` - OAuth callback

### WeCom/WeChat
- `GET /auth/wechat` - Start WeCom OAuth flow
- `GET /auth/wechat/callback` - OAuth callback

---

## Workspace & Recommendations

### GET /api/workspace/provider-state
Get the current state of workspace providers (integrations).

**Headers:** `Authorization: Bearer <token>`

---

### POST /api/workspace/recommendation
Get a recommendation for the workspace.

**Headers:** `Authorization: Bearer <token>`

---

### POST /api/workspace/recommendation-feedback
Record feedback on a recommendation.

**Headers:** `Authorization: Bearer <token>`

---

### GET /api/workspace/recommendation-preferences
Get recommendation preferences.

**Headers:** `Authorization: Bearer <token>`

### POST /api/workspace/recommendation-preferences
Update recommendation preferences.

**Headers:** `Authorization: Bearer <token>`

---

## Tasks & Routines

### GET /api/tasks
Get tasks (legacy endpoint).

---

### GET /api/account/tasks
Get all tasks for the authenticated account.

**Headers:** `Authorization: Bearer <token>`

---

### GET /api/account/routines
Get all routines for the authenticated account.

**Headers:** `Authorization: Bearer <token>`

---

### POST /api/account/routines/:task_id/pause
Pause a routine.

**Headers:** `Authorization: Bearer <token>`

---

### POST /api/account/routines/:task_id/resume
Resume a paused routine.

**Headers:** `Authorization: Bearer <token>`

---

### DELETE /api/account/routines/:task_id
Delete a routine.

**Headers:** `Authorization: Bearer <token>`

---

## Channel Install Onboarding

### POST /api/channel-install-onboarding/resend
Resend the onboarding message for a channel installation.

**Headers:** `Authorization: Bearer <token>`

---

## Startup Workspace

### POST /api/startup-workspace/intake-chat
Handle intake chat for startup workspace setup.

**Headers:** `Authorization: Bearer <token>`
