# Lark Bitable — Team Activity table schema

The Phase 3a-3 workspace state publisher writes one row per active workspace to
a dedicated Lark Bitable table. The table schema is fixed and owned by Ansambel;
Settings → Team Activity ships a `Setup table schema` button that creates these
columns automatically.

If you'd rather provision the table manually, use the spec below.

| Column                 | Type         | Required      | Notes                                                                  |
| ---------------------- | ------------ | ------------- | ---------------------------------------------------------------------- |
| `workspace_id`         | Text         | yes (primary) | Local Ansambel workspace ID (`ws_*` ULID). Unique per machine.         |
| `repo_remote_url`      | Text         | no            | Canonical `git remote get-url origin` of the worktree.                 |
| `repo_display_name`    | Text         | no            | Human-readable repo name.                                              |
| `task_title`           | Text         | no            | Title of the task this workspace is for.                               |
| `assignee_machine`     | Text         | no            | `<user>@<hostname>` of the engineer running this workspace.            |
| `ansambel_status`      | SingleSelect | no            | Options: `idle`, `running`, `waiting`, `blocked`, `pr_ready`, `done`.  |
| `last_activity_at`     | DateTime     | no            | Epoch ms of the most recent state change.                              |
| `last_message_preview` | Text         | no            | ≤200-char redacted preview of the last assistant message.              |
| `branch_name`          | Text         | no            | Git branch the worktree is on.                                         |
| `diff_summary`         | Text         | no            | e.g. `+45 -12 across 3 files`. (Not currently emitted — see plan §3.)  |
| `pr_url`               | URL          | no            | GitHub PR URL once one is open. (Not currently emitted — see plan §3.) |
| `private`              | Checkbox     | no            | When true, sensitive columns are blanked.                              |

## Sensitivity

All text columns are passed through a credential redactor (regex set:
OpenAI-style `sk-*`, `Bearer *`, JWTs, named credentials) before they leave the
local machine. The publisher's debounce coalesces multiple events into one
upsert per 3-second window per workspace, capping outbound traffic.

## Visibility

Engineer-level filter is client-side: the Team Activity sidebar (Phase 3a-4,
follow-up) only shows rows whose `repo_remote_url` matches a repo in the
viewer's local Ansambel install. The Bitable itself contains all rows from the
team; if a viewer has direct Lark access they will see them.
