# Disk-backed projects

## Agreed behavior

A project is a disk-backed root group. The workspace is the full Blink session.
The Browser can show several projects and local groups at the same time. Child
groups inherit their root's storage. Existing local workspaces remain readable.

Project folders contain readable JSON definitions: `blink.json`, nested group
folders, and one file per request. Stable project-local IDs preserve references
through renames. App-local attachments map these IDs to workspace IDs, so two
copies of a project can coexist without ID collisions.

Open Project Folder attaches a project. Save Group to Folder converts a local
root after a file/credential disclosure. Close Project removes it from the
Browser without deleting files. Project roots cannot be nested or deleted with
the ordinary group operation. Ordinary drag/drop cannot cross storage scopes;
an explicit transfer must explain dependencies before changing scopes.

## Storage and safety

Project definitions are the source of truth. Responses, history, cookies,
selected environment, tab state, captured values, and local credential values
remain in application storage. Literal authorization credentials are extracted
into local token values; the shared files contain references. Other request
content is shared deliberately and conversion discloses this.

Project requests do not inherit workspace-global tokens or response tokens.
Cross-project response-token dependencies are invalid. Cookie jars are separated
by project. Loading never sends requests or grants access to upload files.

Disk access runs outside the UI thread. Saves compare the files with the last
read version, refuse to overwrite external changes, and report conflicts.
Explicit reload discards local definition edits only after confirmation. Missing
or invalid folders remain visible with an error and are never recreated by an
autosave. Managed paths cannot escape the project through traversal or symlinks.

## Verification

Tests cover project round trips, multiple attachments with colliding IDs,
reference remapping, separation of shared and private state, legacy snapshots,
scope guards, close versus delete, external edits, missing folders, malformed
files, unsafe paths, stale writes, and Browser actions. Run workspace tests,
format checks, and Clippy before completion.
