import { bodyModes, methods, type BodyMode, type Method } from "./request";
import type { RequestSession } from "./session";
import type { RequestGroup } from "./groups";

export type WorkspacePreferences = {
  defaultMethod: Method;
  defaultBodyMode: BodyMode;
  pretty: boolean;
  wrap: boolean;
  confirmCloseDrafts: boolean;
};

export const defaultPreferences = (): WorkspacePreferences => ({
  defaultMethod: "GET",
  defaultBodyMode: "none",
  pretty: true,
  wrap: false,
  confirmCloseDrafts: true,
});

export function validPreferences(
  value: unknown,
): value is WorkspacePreferences {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const p = value as Record<string, unknown>;
  return (
    methods.includes(p.defaultMethod as Method) &&
    bodyModes.includes(p.defaultBodyMode as BodyMode) &&
    typeof p.pretty === "boolean" &&
    typeof p.wrap === "boolean" &&
    typeof p.confirmCloseDrafts === "boolean"
  );
}

export function resolveNewRequestDefaults(
  groups: RequestGroup[],
  groupId: number | null,
  preferences: WorkspacePreferences,
) {
  const byId = new Map(groups.map((group) => [group.id, group]));
  let group = groupId === null ? undefined : byId.get(groupId);
  const result = {
    method: undefined as Method | undefined,
    bodyMode: preferences.defaultBodyMode,
    url: undefined as string | undefined,
    pretty: preferences.pretty,
    wrap: preferences.wrap,
  };
  const seen = new Set<number>();
  while (group && !seen.has(group.id)) {
    seen.add(group.id);
    if (result.method === undefined && group.defaultMethod !== undefined)
      result.method = group.defaultMethod;
    if (result.url === undefined && group.defaultUrl !== undefined)
      result.url = group.defaultUrl;
    group = group.parentId === null ? undefined : byId.get(group.parentId);
  }
  return {
    ...result,
    method: result.method ?? preferences.defaultMethod,
    url: result.url ?? "",
  };
}

export function applyNewRequestDefaults(
  session: RequestSession,
  defaults: ReturnType<typeof resolveNewRequestDefaults>,
) {
  session.draft.method = defaults.method;
  session.draft.bodyMode = defaults.bodyMode;
  session.draft.url = defaults.url;
  session.view.pretty = defaults.pretty;
  session.view.wrap = defaults.wrap;
  return session;
}
