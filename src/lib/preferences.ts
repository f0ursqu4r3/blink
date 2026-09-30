import { bodyModes, isMethod, type BodyMode, type Method } from "./request";
import type { RequestSession } from "./session";
import type { RequestGroup } from "./groups";
import {
  defaultTransportOptions,
  proxyUrlError,
  transportFieldErrors,
  type TransportOptions,
} from "./transport-options";

export type WorkspacePreferences = {
  defaultMethod: Method;
  defaultBodyMode: BodyMode;
  pretty: boolean;
  wrap: boolean;
  confirmCloseDrafts: boolean;
} & TransportOptions;

export const defaultPreferences = (): WorkspacePreferences => ({
  defaultMethod: "GET",
  defaultBodyMode: "none",
  pretty: true,
  wrap: false,
  confirmCloseDrafts: true,
  ...defaultTransportOptions(),
});

export function validPreferences(
  value: unknown,
): value is WorkspacePreferences {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const p = value as Record<string, unknown>;
  return (
    isMethod(p.defaultMethod) &&
    bodyModes.includes(p.defaultBodyMode as BodyMode) &&
    typeof p.pretty === "boolean" &&
    typeof p.wrap === "boolean" &&
    typeof p.confirmCloseDrafts === "boolean" &&
    typeof p.followRedirects === "boolean" &&
    typeof p.verifyTls === "boolean" &&
    typeof p.proxyUrl === "string" &&
    proxyUrlError(p.proxyUrl) === "" &&
    Object.keys(transportFieldErrors(p as TransportOptions)).length === 0
  );
}

/**
 * Fill fields added after a workspace was saved with their defaults. Returns
 * null for a wrong type or an out-of-range value. Unknown keys are dropped.
 */
export function normalizePreferences(
  value: unknown,
): WorkspacePreferences | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  const input = value as Record<string, unknown>;

  // Check that all non-transport fields are present and valid
  if (
    !isMethod(input.defaultMethod) ||
    !bodyModes.includes(input.defaultBodyMode as BodyMode) ||
    typeof input.pretty !== "boolean" ||
    typeof input.wrap !== "boolean" ||
    typeof input.confirmCloseDrafts !== "boolean"
  )
    return null;

  // Fill in missing transport fields from defaults
  const defaults = defaultTransportOptions();
  const merged: Record<string, unknown> = {
    defaultMethod: input.defaultMethod,
    defaultBodyMode: input.defaultBodyMode,
    pretty: input.pretty,
    wrap: input.wrap,
    confirmCloseDrafts: input.confirmCloseDrafts,
  };

  // Add transport fields (use provided values, or defaults)
  for (const key of Object.keys(defaults) as (keyof typeof defaults)[])
    merged[key] = key in input ? input[key] : defaults[key];

  // Validate the complete object
  return validPreferences(merged) ? (merged as WorkspacePreferences) : null;
}

export const transportOptions = (
  preferences: WorkspacePreferences,
): TransportOptions => ({
  timeoutSeconds: preferences.timeoutSeconds,
  connectTimeoutSeconds: preferences.connectTimeoutSeconds,
  followRedirects: preferences.followRedirects,
  maxRedirects: preferences.maxRedirects,
  inspectionLimitMiB: preferences.inspectionLimitMiB,
  verifyTls: preferences.verifyTls,
  proxyUrl: preferences.proxyUrl,
});

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
