export type RequestGroup = {
  id: number
  name: string
  parentId: number | null
  collapsed: boolean
  /** Local auth override. Undefined = inherit from parent group. */
  localAuth?: import('./authorization').AuthorizationConfig | undefined
  /** Local token definitions for interpolation. */
  localDefinitions?: Record<string, string> | undefined
  /** Defaults for new requests in this group. Undefined inherits. */
  defaultMethod?: import('./request').Method | undefined
  defaultUrl?: string | undefined
  /** Root groups only: named token sets, switched in the Browser. */
  environments?: import('./environments').Environment[]
  /** The active environment id. Undefined or null: base tokens only. */
  activeEnvironmentId?: number | null
}

export type GroupedSession = {
  id: number
  groupId: number | null
}

let sequence = 0

export function reserveGroupId(id: number) {
  sequence = Math.max(sequence, id)
}

export function createGroup(name: string, parentId: number | null = null): RequestGroup {
  return {
    id: ++sequence,
    name: name.trim(),
    parentId,
    collapsed: false,
  }
}

export function canNestGroup(groups: RequestGroup[], groupId: number, parentId: number | null) {
  if (parentId === null) return true
  if (groupId === parentId) return false

  const parentById = new Map(groups.map((group) => [group.id, group.parentId]))
  let cursor: number | null | undefined = parentId
  const seen = new Set<number>()
  while (cursor !== null && cursor !== undefined) {
    if (cursor === groupId || seen.has(cursor) || !parentById.has(cursor)) return false
    seen.add(cursor)
    cursor = parentById.get(cursor)
  }
  return true
}

/** Ids of `groupId` and all of its descendants. Empty for an unknown group. */
export function groupSubtree(groups: RequestGroup[], groupId: number) {
  const ids = new Set<number>()
  if (!groups.some((group) => group.id === groupId)) return ids
  ids.add(groupId)
  let grew = true
  while (grew) {
    grew = false
    for (const group of groups)
      if (group.parentId !== null && ids.has(group.parentId) && !ids.has(group.id)) {
        ids.add(group.id)
        grew = true
      }
  }
  return ids
}

/**
 * Order of `sessions` after moving `ids` into `groupId`, before `beforeId`
 * or after the last session of that group. Returns the same objects; the
 * caller assigns `groupId`.
 */
export function sessionsAfterMove<T extends GroupedSession>(
  sessions: T[],
  ids: number[],
  groupId: number | null,
  beforeId: number | null,
): T[] {
  const moving = new Set(ids)
  const moved = sessions.filter((session) => moving.has(session.id))
  const remaining = sessions.filter((session) => !moving.has(session.id))
  let insertAt =
    beforeId === null
      ? -1
      : remaining.findIndex((session) => session.id === beforeId && session.groupId === groupId)
  if (insertAt < 0) {
    const last = remaining.reduce(
      (index, session, current) => (session.groupId === groupId ? current : index),
      -1,
    )
    insertAt = last < 0 ? remaining.length : last + 1
  }
  remaining.splice(insertAt, 0, ...moved)
  return remaining
}

/**
 * Order of `groups` after moving `groupId` under `parentId`, before
 * `beforeGroupId` or after its last new sibling. Returns the same objects;
 * the caller assigns `parentId`. Null when the move names an unknown group
 * or would nest a group inside itself.
 */
export function groupsAfterMove(
  groups: RequestGroup[],
  groupId: number,
  parentId: number | null,
  beforeGroupId: number | null,
): RequestGroup[] | null {
  const source = groups.find((group) => group.id === groupId)
  if (!source || !canNestGroup(groups, groupId, parentId)) return null
  const remaining = groups.filter((group) => group.id !== groupId)
  let insertAt =
    beforeGroupId === null
      ? -1
      : remaining.findIndex((group) => group.id === beforeGroupId && group.parentId === parentId)
  if (insertAt < 0) {
    const last = remaining.reduce(
      (index, group, current) => (group.parentId === parentId ? current : index),
      -1,
    )
    insertAt = last < 0 ? remaining.length : last + 1
  }
  remaining.splice(insertAt, 0, source)
  return remaining
}

export function deleteGroupAndPromoteContents<T extends GroupedSession>(
  groups: RequestGroup[],
  sessions: T[],
  groupId: number,
) {
  const removed = groups.find((group) => group.id === groupId)
  if (!removed) return { groups, sessions }

  return {
    groups: groups
      .filter((group) => group.id !== groupId)
      .map((group) =>
        group.parentId === groupId ? { ...group, parentId: removed.parentId } : group,
      ),
    sessions: sessions.map((session) =>
      session.groupId === groupId ? { ...session, groupId: removed.parentId } : session,
    ),
  }
}
