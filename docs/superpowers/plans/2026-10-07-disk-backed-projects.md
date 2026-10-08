# Disk-backed Projects Implementation Plan

**Goal:** Support several disk-backed Browser roots beside existing local groups.

**Architecture:** A project document contains shared groups and request drafts.
Project disk storage validates and updates managed files. Workspace attachments
remap stable project IDs and retain private state. The engine coordinates disk
writes; the Store and Browser expose open, convert, reload, and close actions.

**Tech Stack:** Rust, serde JSON, tempfile, Tokio, GPUI Kit.

**Spec:** ../specs/2026-10-07-disk-backed-projects-design.md

## Global constraints

- Preserve the existing uncommitted work.
- Keep local snapshot compatibility and protect failed saves.
- Never send a request or authorize an upload during project loading.
- Never overwrite external edits silently.
- Use project for a disk-backed root and workspace for the whole session.

## Task 1: Project file storage

- [x] Define `ProjectDocument { root_id, groups, requests }` and
  `ProjectRequest { id, group_id, draft }` with serde and deterministic JSON.
- [x] Test create/open/save round trips and nested folders before implementation.
- [x] Implement `ProjectDisk::create`, `open`, `save`, and `changed`.
- [x] Test malformed definitions, missing folders, unrelated files, symlinks,
  external edit conflicts, and preservation after a rejected write.

## Task 2: Workspace attachments

- [x] Add a serializable `ProjectAttachment` and `Workspace.projects`.
- [x] Test attachment of two documents with identical IDs before implementation.
- [x] Implement attach, convert, project projection, reload, and close operations.
- [x] Remap response tokens and retain stable IDs across saves and reloads.
- [x] Keep runtime state and credential/capture values out of shared documents.
- [x] Test local snapshot round trips, context isolation, and move/delete guards.

## Task 3: Engine, Store, and Browser

- [x] Add serialized background project IO and save ordering.
- [x] Restore attached projects from disk and display unavailable/conflict states.
- [x] Add native folder dialogs, conversion disclosure, reload confirmation,
  project indicators, folder actions, and Close Project.
- [x] Use a project cookie scope for sends and cookie management.
- [x] Test restart, failed save, close without file deletion, and visible actions.

## Task 4: Verification and documentation

- [x] Review the complete change for data loss and project boundary errors.
- [x] Run `cargo test --workspace` and fix failures.
- [x] Run `cargo fmt --all -- --check`.
- [x] Run strict Clippy. The existing `graphql.rs:1748` `needless_late_init` lint blocks the unmodified command. Checks pass with only that lint excluded: `cargo clippy --workspace --all-targets -- -D warnings -A clippy::needless_late_init`.
- [x] Update README and PRODUCT with the final behavior and limitations.

## Review focus

1. A Git checkout during autosave must not be overwritten (Task 1).
2. Identical IDs from two projects must not share credentials (Tasks 2 and 3).
3. Closing or losing a project folder must not delete project files (Task 3).
4. Captures and authentication literals must not appear in shared files (Task 2).
5. Drag/drop, settings, and undo must preserve storage boundaries (Task 2).

## Verification record

- Full workspace tests: 157 app tests and 683 core tests passed; one existing core test ignored.
- Format and diff whitespace checks passed.
- Independent review found and verified fixes for duplicate-open baselines, failed-save recovery, rollback backup retention, empty project snapshots, environment capture precedence, and persistent credential privacy.
- Privacy classifications are recorded before queued snapshots are encoded. Independent re-review confirmed the queued-save regression is addressed.
- Project saves retain a durable journal after interruption. Recovery is manual.
- Close Project requires file/save errors to be resolved first, so unresolved edits cannot be discarded by detaching a project.
