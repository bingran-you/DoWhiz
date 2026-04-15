# send_emails_module

Postmark outbound email sender used by scheduler `SendReply` tasks.

## Features

- send HTML email from file (`html_path`)
- wrap outbound HTML in a responsive DoWhiz-branded shell that uses the shared website light-theme colors
- send attachments from flat directory (`attachments_dir`)
- supports To/Cc/Bcc + threading headers (`In-Reply-To`, `References`)

## Required Env

- `POSTMARK_SERVER_TOKEN`

Optional:
- `POSTMARK_API_BASE_URL` (test/mocking override)

## Usage

```rust
use send_emails_module::{send_email, SendEmailParams};
use std::path::PathBuf;

let params = SendEmailParams {
    subject: "Hello".to_string(),
    html_path: PathBuf::from("/path/to/reply_email_draft.html"),
    attachments_dir: PathBuf::from("/path/to/reply_email_attachments"),
    from: Some("oliver@dowhiz.com".to_string()),
    to: vec!["user@example.com".to_string()],
    cc: vec![],
    bcc: vec![],
    in_reply_to: None,
    references: None,
    reply_to: None,
};

let resp = send_email(&params)?;
println!("message id: {}", resp.message_id);
```

`send_email` expects the draft file to contain the email content itself (paragraphs, headings,
lists, tables, links). The module applies the shared DoWhiz shell at send time so replies stay
responsive and avoid overly narrow centered layouts.

## Tests

Offline tests:

```bash
cd DoWhiz_service
cargo test -p send_emails_module
```

Live Postmark tests are opt-in and require env credentials:

```bash
cd DoWhiz_service
POSTMARK_LIVE_TEST=1 cargo test -p send_emails_module -- --nocapture
```
