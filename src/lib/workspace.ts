import { bodyModes, buildRequest, isMethod, reservePairId } from "./request";
import {
  buildResolvedRequestContext,
  type AuthorizationConfig,
} from "./authorization";
import {
  requestFingerprint,
  reserveSessionId,
  type RequestSession,
} from "./session";
import { reserveGroupId, type RequestGroup } from "./groups";
import {
  defaultPreferences,
  normalizePreferences,
  type WorkspacePreferences,
} from "./preferences";

export const WORKSPACE_KEY = "blink.workspace.v1";
export const MAX_STATE_BYTES = 64 * 1024 * 1024;
/** Sanity bounds for a snapshot. The byte limit is the real cap. */
export const MAX_REQUESTS = 10_000;
export const MAX_GROUPS = 10_000;
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
type SnapshotV3 = {
  version: 3;
  activeId: number;
  tabs: StoredTab[];
  groups: RequestGroup[];
  /** Required in v3 encode; must be a flat string map. Empty object when no globals. */
  globalDefinitions: Record<string, string>;
  preferences?: WorkspacePreferences;
};
type SnapshotV4 = Omit<SnapshotV3, "version" | "activeId"> & {
  version: 4;
  /** Null when no tab is open. */
  activeId: number | null;
  /** Ids of requests open as tabs, in tab order. */
  openIds: number[];
};
type Snapshot = SnapshotV1 | SnapshotV2 | SnapshotV3 | SnapshotV4;
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

function validateAuthorizationConfig(
  value: unknown,
): value is AuthorizationConfig {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const obj = value as Record<string, unknown>;
  if (obj.type === "none") return true;
  if (obj.type === "bearer") return text(obj.token);
  if (obj.type === "basic") return text(obj.username) && text(obj.password);
  return false;
}

/** Max entries in a definitions map (global or local). */
const MAX_DEFINITIONS_ENTRIES = 500;
/** Max bytes for a definition name or value. */
const MAX_DEFINITION_KEY_LEN = 256;
const MAX_DEFINITION_VAL_LEN = 65536;

/** Valid definition name pattern: non-empty, no leading `_` for local names. */
function validateDefinitionName(
  name: string,
  allowLeadingUnderscore: boolean,
): boolean {
  if (!name || name.length > MAX_DEFINITION_KEY_LEN) return false;
  if (!allowLeadingUnderscore && name.startsWith("_")) return false;
  return true;
}

function validateDefinitions(
  value: unknown,
  opts: { allowLeadingUnderscore?: boolean } = {},
): value is Record<string, string> {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const entries = Object.entries(value as object);
  if (entries.length > MAX_DEFINITIONS_ENTRIES) return false;
  for (const [k, v] of entries) {
    if (!validateDefinitionName(k, opts.allowLeadingUnderscore ?? true))
      return false;
    if (typeof v !== "string" || v.length > MAX_DEFINITION_VAL_LEN)
      return false;
  }
  return true;
}

function validateDraft(input: unknown, version: number) {
  const draft = record(input);
  check(isMethod(draft.method));
  for (const key of ["url", "body", "token", "username", "password"])
    check(text(draft[key]));
  check(bodyModes.includes(draft.bodyMode as never));
  check(draft.variables === undefined || text(draft.variables));
  check(draft.bodyFile === undefined || text(draft.bodyFile));
  check(["none", "bearer", "basic"].includes(draft.auth as string));
  // v3: validate localAuth if present
  if (version >= 3 && draft.localAuth !== undefined) {
    check(validateAuthorizationConfig(draft.localAuth));
  }
  for (const key of ["query", "headers", "form"]) {
    if (key === "form" && draft.form === undefined) continue;
    const ids = new Set();
    for (const row of array(draft[key], 10_000)) {
      const entry = record(row);
      check(
        id(entry.id) &&
          !ids.has(entry.id) &&
          text(entry.key) &&
          text(entry.value) &&
          typeof entry.enabled === "boolean" &&
          (entry.file === undefined || typeof entry.file === "boolean"),
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
  check(
    (response.bodyId === undefined || text(response.bodyId)) &&
      (response.truncated === undefined ||
        typeof response.truncated === "boolean") &&
      (response.binary === undefined || typeof response.binary === "boolean") &&
      (response.finalUrl === undefined || text(response.finalUrl)) &&
      (response.redirectCount === undefined || numeric(response.redirectCount)),
  );
  if (response.timing !== undefined) {
    const timing = record(response.timing);
    check(numeric(timing.waitMs) && numeric(timing.downloadMs));
    for (const key of ["dnsMs", "connectMs", "tlsMs"])
      check(timing[key] === undefined || numeric(timing[key]));
  }
  for (const item of array(response.headers, 10_000)) {
    const header = record(item);
    check(text(header.key) && text(header.value));
  }
}
function validateGroups(input: unknown, version: number): RequestGroup[] {
  const groups = array(input, MAX_GROUPS).map((value) => {
    const group = record(value);
    check(
      id(group.id) &&
        text(group.name) &&
        (group.name as string).trim().length > 0 &&
        (group.name as string).length <= 80 &&
        (group.parentId === null || id(group.parentId)) &&
        typeof group.collapsed === "boolean",
    );
    // v3: validate optional localAuth and localDefinitions
    if (version >= 3) {
      if (group.localAuth !== undefined) {
        check(validateAuthorizationConfig(group.localAuth));
      }
      if (group.localDefinitions !== undefined) {
        check(
          validateDefinitions(group.localDefinitions, {
            allowLeadingUnderscore: false,
          }),
        );
      }
      if (group.defaultMethod !== undefined)
        check(isMethod(group.defaultMethod));
      if (group.defaultUrl !== undefined)
        check(
          text(group.defaultUrl) &&
            (group.defaultUrl as string).length <= 65536,
        );
    }
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
      "Workspace exceeds the 64 MiB save limit. Delete unused requests before saving.",
    );
  let raw: unknown;
  try {
    raw = JSON.parse(content);
  } catch {
    throw invalid();
  }
  const data = record(raw);
  check([1, 2, 3, 4].includes(data.version as number));
  const version = data.version as number;
  check(id(data.activeId) || (version >= 4 && data.activeId === null));
  const tabs = array(data.tabs, MAX_REQUESTS);
  check(tabs.length > 0);
  const ids = new Set();
  for (const item of tabs) {
    const tab = record(item);
    check(id(tab.id) && !ids.has(tab.id));
    ids.add(tab.id);
    validateDraft(tab.draft, version);
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
  if (version >= 4) {
    const openIds = array(data.openIds, MAX_REQUESTS);
    check(
      openIds.every((openId) => ids.has(openId)) &&
        new Set(openIds).size === openIds.length,
    );
    check(
      data.activeId === null
        ? openIds.length === 0
        : openIds.includes(data.activeId),
    );
  } else check(ids.has(data.activeId));
  if (version >= 2) {
    const groups = validateGroups(data.groups, version);
    const groupIds = new Set(groups.map((group) => group.id));
    for (const item of tabs) {
      const tab = record(item);
      check(tab.groupId === null || groupIds.has(tab.groupId as number));
    }
  }
  if (version >= 3) {
    // globalDefinitions is required in v3 and must be a flat string map
    check(
      data.globalDefinitions !== undefined &&
        validateDefinitions(data.globalDefinitions),
    );
    if (data.preferences !== undefined)
      check(normalizePreferences(data.preferences) !== null);
  }
  return raw as Snapshot;
}

/**
 * Migrate flat draft auth fields (v1/v2) to structured localAuth.
 * V1/V2 auth was always explicit (there was no inheritance), so we always
 * produce an explicit localAuth.
 */
function migrateDraftAuth(draft: Record<string, unknown>): AuthorizationConfig {
  const auth = draft.auth as string;
  if (auth === "bearer") {
    return { type: "bearer", token: (draft.token as string) ?? "" };
  }
  if (auth === "basic") {
    return {
      type: "basic",
      username: (draft.username as string) ?? "",
      password: (draft.password as string) ?? "",
    };
  }
  return { type: "none" };
}

export function encodeWorkspace(
  sessions: RequestSession[],
  activeId: number | null,
  groups: RequestGroup[] = [],
  globalDefinitions: Record<string, string> = {},
  preferences: WorkspacePreferences = defaultPreferences(),
  openIds: number[] = sessions.map((session) => session.id),
): string {
  return JSON.stringify({
    version: 4,
    activeId,
    openIds,
    groups,
    globalDefinitions,
    preferences,
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
        // The body file does not survive a restart. A truncated or binary
        // preview is dropped too: keeping every preview up to the
        // inspection limit could blow past the workspace save limit.
        response: response && {
          ...response,
          body: response.truncated || response.binary ? "" : response.body,
          bodyId: undefined,
        },
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
  const version = data.version;
  const sessions: RequestSession[] = data.tabs.map(
    ({ interrupted, groupId = null, ...tab }) => {
      const draft = { ...tab.draft } as RequestSession["draft"];
      // Migrate v1/v2 flat auth to localAuth
      if (version < 3) {
        draft.localAuth = migrateDraftAuth(
          tab.draft as unknown as Record<string, unknown>,
        );
      }
      return {
        ...tab,
        draft,
        groupId,
        busy: false,
        elapsed: 0,
        error: interrupted
          ? "Request interrupted when Blink closed. Send again to retry."
          : tab.error,
      };
    },
  );
  for (const session of sessions) {
    reserveSessionId(session.id);
    [
      ...session.draft.query,
      ...session.draft.headers,
      ...(session.draft.form ?? []),
    ].forEach((row) => reservePairId(row.id));
    if (session.response) delete session.response.bodyId;
  }
  const groups =
    version >= 2 ? (data as SnapshotV2 | SnapshotV3 | SnapshotV4).groups : [];
  for (const group of groups) reserveGroupId(group.id);
  const globalDefinitions =
    version >= 3 ? (data as SnapshotV3 | SnapshotV4).globalDefinitions : {};
  if (version < 3) {
    for (const session of sessions) {
      if (!session.response) continue;
      try {
        const context = buildResolvedRequestContext(
          session.draft,
          session.groupId,
          groups,
          globalDefinitions,
        );
        session.sentFingerprint = requestFingerprint(
          buildRequest(session.draft, context),
          context.auth.type,
        );
      } catch {
        session.sentFingerprint = "";
      }
    }
  }
  const saved =
    version >= 3 ? (data as SnapshotV3 | SnapshotV4).preferences : undefined;
  const preferences =
    saved === undefined
      ? defaultPreferences()
      : (normalizePreferences(saved) ?? defaultPreferences());
  // Before v4 every request was an open tab.
  const openIds =
    version >= 4
      ? (data as SnapshotV4).openIds
      : sessions.map((session) => session.id);
  return {
    sessions,
    activeId: data.activeId,
    openIds,
    groups,
    globalDefinitions,
    preferences,
  };
}
