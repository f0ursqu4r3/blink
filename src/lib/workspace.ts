import { methods, reservePairId } from "./request";
import { reserveSessionId, type RequestSession } from "./session";
import { reserveGroupId, type RequestGroup } from "./groups";

export const WORKSPACE_KEY = "blink.workspace.v1";
export const MAX_STATE_BYTES = 64 * 1024 * 1024;
type StoredTab = Omit<RequestSession, "busy" | "elapsed" | "groupId"> & {
  interrupted: boolean;
  groupId?: number | null;
};
type SnapshotV1 = { version: 1; activeId: number; tabs: StoredTab[] };
type SnapshotV2 = {
  version: 2;
  activeId: number;
  tabs: StoredTab[];
  groups: RequestGroup[];
};
type Snapshot = SnapshotV1 | SnapshotV2;
const invalid = () =>
  new Error(
    "Saved workspace is invalid or from an unsupported version. It has not been changed.",
  );
function record(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw invalid();
  return value as Record<string, unknown>;
}
function check(value: unknown): asserts value {
  if (!value) throw invalid();
}
const text = (value: unknown) => typeof value === "string";
const numeric = (value: unknown) =>
  typeof value === "number" && Number.isFinite(value) && value >= 0;
const id = (value: unknown) =>
  typeof value === "number" &&
  Number.isSafeInteger(value) &&
  value > 0 &&
  value < 1_000_000_000;
function array(value: unknown, max: number): unknown[] {
  check(Array.isArray(value) && value.length <= max);
  return value;
}
function validateDraft(input: unknown) {
  const draft = record(input);
  check(methods.includes(draft.method as never));
  for (const key of ["url", "body", "token", "username", "password"])
    check(text(draft[key]));
  check(["none", "json", "text"].includes(draft.bodyMode as string));
  check(["none", "bearer", "basic"].includes(draft.auth as string));
  for (const key of ["query", "headers"]) {
    const ids = new Set();
    for (const row of array(draft[key], 10_000)) {
      const entry = record(row);
      check(
        id(entry.id) &&
          !ids.has(entry.id) &&
          text(entry.key) &&
          text(entry.value) &&
          typeof entry.enabled === "boolean",
      );
      ids.add(entry.id);
    }
  }
}
function validateResponse(input: unknown) {
  if (input === null) return;
  const response = record(input);
  check(
    Number.isInteger(response.status) &&
      Number(response.status) >= 100 &&
      Number(response.status) <= 599,
  );
  check(
    text(response.statusText) &&
      text(response.body) &&
      numeric(response.durationMs) &&
      numeric(response.sizeBytes),
  );
  for (const item of array(response.headers, 10_000)) {
    const header = record(item);
    check(text(header.key) && text(header.value));
  }
}
function validateGroups(input: unknown): RequestGroup[] {
  const groups = array(input, 128).map((value) => {
    const group = record(value);
    check(
      id(group.id) &&
        text(group.name) &&
        group.name.trim().length > 0 &&
        group.name.length <= 80 &&
        (group.parentId === null || id(group.parentId)) &&
        typeof group.collapsed === "boolean",
    );
    return group as unknown as RequestGroup;
  });
  const byId = new Map(groups.map((group) => [group.id, group]));
  check(byId.size === groups.length);
  for (const group of groups) {
    const visited = new Set<number>([group.id]);
    let parentId = group.parentId;
    while (parentId !== null) {
      check(!visited.has(parentId));
      visited.add(parentId);
      const parent = byId.get(parentId);
      check(parent);
      parentId = parent.parentId;
    }
  }
  return groups;
}
function parseSnapshot(content: string): Snapshot {
  if (
    content.length > MAX_STATE_BYTES ||
    new TextEncoder().encode(content).byteLength > MAX_STATE_BYTES
  )
    throw new Error(
      "Workspace exceeds the 64 MiB save limit. Close unused tabs before saving.",
    );
  let raw: unknown;
  try {
    raw = JSON.parse(content);
  } catch {
    throw invalid();
  }
  const data = record(raw);
  check((data.version === 1 || data.version === 2) && id(data.activeId));
  const tabs = array(data.tabs, 128);
  check(tabs.length > 0);
  const ids = new Set();
  for (const item of tabs) {
    const tab = record(item);
    check(id(tab.id) && !ids.has(tab.id));
    ids.add(tab.id);
    validateDraft(tab.draft);
    validateResponse(tab.response);
    check(tab.groupId === undefined || tab.groupId === null || id(tab.groupId));
    check(
      text(tab.error) &&
        text(tab.sentFingerprint) &&
        typeof tab.interrupted === "boolean",
    );
    const view = record(tab.view);
    check(
      ["query", "headers", "body", "auth"].includes(view.requestTab as string),
    );
    check(["body", "headers"].includes(view.responseTab as string));
    check(
      typeof view.pretty === "boolean" &&
        typeof view.wrap === "boolean" &&
        numeric(view.responseScroll),
    );
  }
  check(ids.has(data.activeId));
  if (data.version === 2) {
    const groups = validateGroups(data.groups);
    const groupIds = new Set(groups.map((group) => group.id));
    for (const item of tabs) {
      const tab = record(item);
      check(tab.groupId === null || groupIds.has(tab.groupId as number));
    }
  }
  return raw as Snapshot;
}
export function encodeWorkspace(
  sessions: RequestSession[],
  activeId: number,
  groups: RequestGroup[] = [],
): string {
  return JSON.stringify({
    version: 2,
    activeId,
    groups,
    tabs: sessions.map(
      ({
        id,
        groupId,
        draft,
        response,
        error,
        sentFingerprint,
        view,
        busy,
      }) => ({
        id,
        groupId,
        draft,
        response,
        error,
        sentFingerprint,
        view,
        interrupted: busy,
      }),
    ),
  });
}
export function validateWorkspace(content: string) {
  parseSnapshot(content);
}
export function decodeWorkspace(content: string) {
  const data = parseSnapshot(content);
  const sessions: RequestSession[] = data.tabs.map(
    ({ interrupted, groupId = null, ...tab }) => ({
      ...tab,
      groupId,
      busy: false,
      elapsed: 0,
      error: interrupted
        ? "Request interrupted when Blink closed. Send again to retry."
        : tab.error,
    }),
  );
  for (const session of sessions) {
    reserveSessionId(session.id);
    [...session.draft.query, ...session.draft.headers].forEach((row) =>
      reservePairId(row.id),
    );
  }
  const groups = data.version === 2 ? data.groups : [];
  for (const group of groups) reserveGroupId(group.id);
  return { sessions, activeId: data.activeId, groups };
}
