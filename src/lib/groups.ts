export type RequestGroup = {
  id: number;
  name: string;
  parentId: number | null;
  collapsed: boolean;
  /** Local auth override. Undefined = inherit from parent group. */
  localAuth?: import("./authorization").AuthorizationConfig | undefined;
  /** Local token definitions for interpolation. */
  localDefinitions?: Record<string, string> | undefined;
};

export type GroupedSession = {
  id: number;
  groupId: number | null;
};

let sequence = 0;

export function reserveGroupId(id: number) {
  sequence = Math.max(sequence, id);
}

export function createGroup(
  name: string,
  parentId: number | null = null,
): RequestGroup {
  return {
    id: ++sequence,
    name: name.trim(),
    parentId,
    collapsed: false,
  };
}

export function canNestGroup(
  groups: RequestGroup[],
  groupId: number,
  parentId: number | null,
) {
  if (parentId === null) return true;
  if (groupId === parentId) return false;

  const parentById = new Map(groups.map((group) => [group.id, group.parentId]));
  let cursor: number | null | undefined = parentId;
  const seen = new Set<number>();
  while (cursor !== null && cursor !== undefined) {
    if (cursor === groupId || seen.has(cursor)) return false;
    seen.add(cursor);
    cursor = parentById.get(cursor);
  }
  return true;
}

export function deleteGroupAndPromoteContents<T extends GroupedSession>(
  groups: RequestGroup[],
  sessions: T[],
  groupId: number,
) {
  const removed = groups.find((group) => group.id === groupId);
  if (!removed) return { groups, sessions };

  return {
    groups: groups
      .filter((group) => group.id !== groupId)
      .map((group) =>
        group.parentId === groupId
          ? { ...group, parentId: removed.parentId }
          : group,
      ),
    sessions: sessions.map((session) =>
      session.groupId === groupId
        ? { ...session, groupId: removed.parentId }
        : session,
    ),
  };
}
