# Audit Evidence Operator Artifact Contract

This contract is for Phase 1 audit evidence operations. It standardizes the package that DoWhiz returns to the auditor.

## Directory contract

Write generated artifacts under:

```text
audit/output/
```

For email workflows, copy generated artifacts into:

```text
reply_email_attachments/
```

Write the auditor-facing reply body to:

```text
reply_email_draft.html
```

Do not copy original client evidence into `reply_email_attachments/` unless the auditor explicitly asks for it.

## Required artifacts

### attachment_inventory.md

Purpose: prove what DoWhiz received and inspected.

Required columns or bullet fields:

- File name
- Relative path
- Detected type
- Size if available
- Stable identifier such as hash if available
- Pages, sheet names, row counts, or paragraph count when available
- Likely audit purpose
- Read status: `Read`, `Partially read`, `Unreadable`, or `Needs auditor review`
- Notes

### pbc_tracker.xlsx

Purpose: let the audit team track PBC request status.

Required columns:

- PBC item ID
- Request description
- Client/entity
- Period
- Status: `Received`, `Missing`, `Duplicate`, `Wrong period`, `Unreadable`, `Needs auditor review`, or `Not applicable per auditor`
- Matched evidence file
- Source reference
- Exception ID
- Owner
- Next step
- Last updated

If `.xlsx` generation is blocked, create `pbc_tracker.csv` as a fallback and state the blocker in `reviewer_summary.md`.

### exception_report.md

Purpose: give the auditor reviewable exceptions without claiming audit conclusions.

Each exception must include:

- Exception ID
- Issue
- Source reference
- Why it matters
- Suggested next step
- Confidence
- Status

Allowed statuses:

- `Needs auditor review`
- `Client follow-up draft prepared`
- `Resolved by provided evidence`
- `Information only`

Forbidden conclusions:

- `audit failed`
- `fraud detected`
- `evidence sufficient`
- `ready for sign-off`
- `material misstatement confirmed`

### reviewer_summary.md

Purpose: give the senior/reviewer a compact package overview.

Include:

- Client and period if known
- Scope understood from the auditor request
- Count of files processed
- PBC status counts
- Top exceptions
- Open questions
- Validation notes
- Explicit limitations

### client_followup_draft.html

Purpose: provide a draft message the auditor may review and send to the client.

Rules:

- Address the client neutrally.
- Ask for missing or corrected materials.
- Do not accuse the client of errors, fraud, or non-compliance.
- Do not disclose internal audit risk language unless the auditor asked for it.
- Include a visible note for the auditor that the draft has not been sent.

### reply_email_draft.html

Purpose: reply to the auditor with the generated package.

Required sections:

- `Processed materials`
- `PBC status`
- `Key exceptions`
- `Attached package`
- `Auditor review required`

Keep this email short. Put detail in the attachments.

## Source reference format

Use the most precise locator available:

- Spreadsheet: `file.xlsx > Sheet1!A12:D12`
- CSV: `file.csv row 17`
- PDF: `file.pdf page 4`
- Text/Markdown: `file.md paragraph 3` or a short quoted excerpt
- Email: `incoming_email/email.html subject/date/from`

If no precise locator is possible, use `Needs auditor review` and explain why.
