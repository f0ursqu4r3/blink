import type { RequestGroup } from "./groups";

/** Theme colors an environment badge can use. */
export const environmentColors = [
  "destructive",
  "warning",
  "success",
  "info",
  "keyword",
] as const;
export type EnvironmentColor = (typeof environmentColors)[number];
export const environmentColorLabels: Record<EnvironmentColor, string> = {
  destructive: "Red",
  warning: "Amber",
  success: "Green",
  info: "Blue",
  keyword: "Purple",
};
export const isEnvironmentColor = (value: unknown): value is EnvironmentColor =>
  environmentColors.includes(value as EnvironmentColor);

/**
 * A named set of token values on a root group. A value here replaces the
 * group's base token of the same name while the environment is active.
 */
export type Environment = {
  id: number;
  name: string;
  color: EnvironmentColor;
  /** Ask before the first send after switching to it. */
  protected?: boolean;
  values: Record<string, string>;
};

export const MAX_ENVIRONMENTS = 20;
export const MAX_ENVIRONMENT_NAME = 24;

/** The root group of `groupId`, or undefined for an ungrouped request. */
export function rootGroup(groupId: number | null, groups: RequestGroup[]) {
  const byId = new Map(groups.map((group) => [group.id, group]));
  const seen = new Set<number>();
  let group = groupId === null ? undefined : byId.get(groupId);
  while (group && group.parentId !== null && !seen.has(group.id)) {
    seen.add(group.id);
    const parent = byId.get(group.parentId);
    if (!parent) break;
    group = parent;
  }
  return group;
}

/** The active environment of a root group. Nested groups have none. */
export function activeEnvironment(group: RequestGroup | undefined) {
  if (!group || group.parentId !== null) return undefined;
  return group.environments?.find(
    (environment) => environment.id === group.activeEnvironmentId,
  );
}

/** The active environment that applies to a request in `groupId`. */
export const requestEnvironment = (
  groupId: number | null,
  groups: RequestGroup[],
) => activeEnvironment(rootGroup(groupId, groups));

/** The base tokens of a group, with the active environment's values on top. */
export function groupDefinitions(group: RequestGroup) {
  const environment = activeEnvironment(group);
  return environment
    ? { ...(group.localDefinitions ?? {}), ...environment.values }
    : (group.localDefinitions ?? {});
}

let sequence = 0;
export function reserveEnvironmentId(id: number) {
  sequence = Math.max(sequence, id);
}
export function createEnvironment(
  name: string,
  color: EnvironmentColor = "info",
): Environment {
  return { id: ++sequence, name, color, values: {} };
}

/** A color the group does not use yet, in palette order. */
export function nextEnvironmentColor(environments: Environment[] = []) {
  const used = new Set(environments.map((environment) => environment.color));
  return (
    [...environmentColors].reverse().find((color) => !used.has(color)) ?? "info"
  );
}

/**
 * Where captured values go for a request: the active environment of its
 * root group, else the root group's base tokens, else the global tokens.
 */
export function applyCapture(
  groupId: number | null,
  groups: RequestGroup[],
  values: Record<string, string>,
): "environment" | "group" | "global" {
  const root = rootGroup(groupId, groups);
  const environment = activeEnvironment(root);
  if (environment) {
    environment.values = { ...environment.values, ...values };
    return "environment";
  }
  if (root) {
    root.localDefinitions = { ...(root.localDefinitions ?? {}), ...values };
    return "group";
  }
  return "global";
}
