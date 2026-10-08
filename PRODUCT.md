# Blink

## Register

product

## Product purpose

Blink is a fast, private desktop HTTP client for professional API work. It
helps developers build, send, inspect, and debug API requests without accounts
or cloud sync. The local Browser sidebar organizes requests in nested groups
with inherited authorization and tokens.

## Users

Developers who work with HTTP APIs every day. They know the protocol and
expect full control of it: any method, any body type, their own certificates
and proxies, and the ability to stop a request at any time. They want a
compact, inspectable workspace instead of a project-management tool.

## Design principles

- Make the request path the primary control surface.
- Send what the user wrote. Do not hide or limit protocol features, such as
  custom methods, file and multipart bodies, proxies, or TLS settings.
- Never block the user. Requests can be canceled, and drafts stay editable
  while a request runs.
- Import and export work in the formats developers already use, starting
  with cURL.
- Keep request data local. Restore the workspace without cloud sync.
- Disclose that saved drafts, credentials, and responses are not encrypted.
- Show protocol facts with clear labels and tabular alignment.
- Use a quiet desktop workspace that supports sustained technical work.

## Anti-references

- No marketing hero language inside the app.
- No oversized metric cards or decorative gradients.
- No hidden state behind menus when it can be visible in the workspace.
- No limits that exist only to keep the tool simple.

## Disk-backed projects

A project is one Browser root group backed by a disk folder. The workspace
is the full application session. Multiple projects and local groups can
coexist. Child groups inherit their root's storage location. Project files
are the source of shared definitions; application storage keeps private
session data.

Users can open a project from the Browser header, File menu, or command
center. Save Group to Folder converts a local root after a disclosure of
shared data. Close Project keeps the folder and a private local session
archive. Project roots cannot be nested or deleted through ordinary group
actions.

The manifest, `blink.json`, stores the root group and inventories nested
`group.json` files and individual request files. Stable project-local IDs
preserve identity across renames. Manual additions and moves require manifest
inventory updates and valid group membership fields. Loading a project never
sends a request or grants upload access. Project-relative upload paths require
an explicit file-picker grant.

Ordinary moves cannot cross storage locations. Move to Another Project checks
token dependencies and inherited request settings before confirmation. Cookie
jars are separate per project. Project requests cannot inherit workspace-global
tokens or depend on responses from another project.

Blink checks for external changes every three seconds and reloads them when
local definitions are unchanged. Conflicting saves stop with an error.
Explicit reload requires confirmation before discarding definition edits.
Missing folders remain visible and are not recreated by autosave.

Authorization credentials and captured response values stay in local
application storage. Responses, history, cookies, selected environments,
tab state, and closed-project archives also stay local. This storage is
plaintext. Request credentials do not use the OS Keychain; Keychain storage
remains future work. Bodies, custom headers, and ordinary token values are
shared deliberately. Use token references and review shared files; arbitrary
secret detection is not provided.

Workspace snapshots use version 5 when they contain project attachments or
closed-project archives. Versions 1 through 5 remain readable. Local-only
snapshots without these records continue to use version 4.

Project saves use staged files, synced backups, and a durable journal at
`.blink-save-*/transaction.json`. An interrupted multi-file save blocks loading
and saving until manual recovery. Preserve that journal and its backups until
recovery is complete. Automatic recovery, team synchronization, and collection
execution are outside this feature's scope.


## Professional workflows

The Browser supports ordered collection runs with CSV/JSON rows, checks, captures, failed-case retries, isolated row cookies and JSON reports. The `blink-run` binary runs disk-backed projects for CI.

Requests include an ephemeral transport inspector. Credential headers and known named secrets are redacted by default. Named secrets, OAuth PKCE tokens and origin-scoped PEM client identities use macOS Keychain. Existing literal secrets are not migrated automatically.

History supports JSON field comparisons, ignored JSON pointers and a persistent local baseline across requests and environments. Imported OpenAPI 3.x operations retain source context and provide parameter suggestions, examples, request/response checks and source reload. Unsupported schema checks remain explicit.
