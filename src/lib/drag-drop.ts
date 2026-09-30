import {
  canNestGroup,
  groupsAfterMove,
  sessionsAfterMove,
  type GroupedSession,
  type RequestGroup,
} from './groups'

export type DropZone = 'before' | 'into' | 'after'
export type Point = { x: number; y: number }
export type Box = { left: number; top: number; width: number; height: number }
export type DragPayload = { kind: 'requests'; ids: number[] } | { kind: 'group'; id: number }
export type TreeTarget =
  | { type: 'request'; id: number }
  | { type: 'group'; id: number }
  | { type: 'root'; position: 'start' | 'end' }
export type Tree = { sessions: GroupedSession[]; groups: RequestGroup[] }
export type TreeCommand =
  | {
      type: 'moveRequests'
      ids: number[]
      groupId: number | null
      beforeId: number | null
    }
  | {
      type: 'moveGroup'
      groupId: number
      parentId: number | null
      beforeGroupId: number | null
    }
/** `key` names the row that shows the indicator: "root", "group-<id>", or "request-<id>". */
export type TreeDrop = { key: string; zone: DropZone; command: TreeCommand }
/** `key` is "tab-<id>", or "strip" when no tabs are open. */
export type TabDrop = {
  key: string
  zone: DropZone
  ids: number[]
  beforeId: number | null
}

/** Which part of a row the pointer is over. */
export function hitZone(box: Box, point: Point, kind: 'request' | 'group' | 'tab'): DropZone {
  if (kind === 'tab') return point.x < box.left + box.width / 2 ? 'before' : 'after'
  const offset = (point.y - box.top) / box.height
  if (kind === 'request') return offset < 0.5 ? 'before' : 'after'
  return offset < 0.25 ? 'before' : offset > 0.75 ? 'after' : 'into'
}

/** First item after `index` that is not skipped and matches. */
function nextId<T extends { id: number }>(
  items: T[],
  index: number,
  skip: Set<number>,
  match: (item: T) => boolean,
) {
  for (let current = index + 1; current < items.length; current++) {
    const item = items[current]
    if (!skip.has(item.id) && match(item)) return item.id
  }
  return null
}

function sameOrder(left: { id: number }[], right: { id: number }[]) {
  return left.length === right.length && left.every((item, index) => item.id === right[index].id)
}

function groupKey(groupId: number | null) {
  return groupId === null ? 'root' : `group-${groupId}`
}

export function resolveTreeDrop(
  payload: DragPayload,
  target: TreeTarget,
  zone: DropZone,
  tree: Tree,
): TreeDrop | null {
  return payload.kind === 'requests'
    ? resolveRequestsDrop(payload.ids, target, zone, tree)
    : resolveGroupDrop(payload.id, target, zone, tree)
}

function resolveRequestsDrop(
  requestIds: number[],
  target: TreeTarget,
  zone: DropZone,
  tree: Tree,
): TreeDrop | null {
  const ids = requestIds.filter((id) => tree.sessions.some((session) => session.id === id))
  if (!ids.length) return null
  const moving = new Set(ids)
  let groupId: number | null = null
  let beforeId: number | null = null
  let key = 'root'
  let effective: DropZone = 'into'
  if (target.type === 'root') {
    if (target.position === 'start')
      beforeId = nextId(tree.sessions, -1, moving, (session) => session.groupId === null)
  } else if (target.type === 'group') {
    if (!tree.groups.some((group) => group.id === target.id)) return null
    groupId = target.id
    key = groupKey(target.id)
  } else {
    const index = tree.sessions.findIndex((session) => session.id === target.id)
    const anchor = tree.sessions[index]
    if (!anchor || zone === 'into') return null
    groupId = anchor.groupId
    key = `request-${anchor.id}`
    effective = zone
    beforeId = nextId(
      tree.sessions,
      zone === 'before' ? index - 1 : index,
      moving,
      (session) => session.groupId === groupId,
    )
  }
  const inGroup = (session: GroupedSession) => session.groupId === groupId
  const unchanged =
    tree.sessions.every((session) => !moving.has(session.id) || session.groupId === groupId) &&
    sameOrder(
      sessionsAfterMove(tree.sessions, ids, groupId, beforeId).filter(inGroup),
      tree.sessions.filter(inGroup),
    )
  if (unchanged) return null
  return {
    key,
    zone: effective,
    command: { type: 'moveRequests', ids, groupId, beforeId },
  }
}

function resolveGroupDrop(
  groupId: number,
  target: TreeTarget,
  zone: DropZone,
  tree: Tree,
): TreeDrop | null {
  const source = tree.groups.find((group) => group.id === groupId)
  if (!source) return null
  const skip = new Set([groupId])
  const childOf = (parentId: number | null) => (group: RequestGroup) => group.parentId === parentId
  let parentId: number | null = null
  let beforeGroupId: number | null = null
  let key = 'root'
  let effective: DropZone = 'into'
  if (target.type === 'root') {
    if (target.position === 'start') beforeGroupId = nextId(tree.groups, -1, skip, childOf(null))
  } else if (target.type === 'request') {
    const anchor = tree.sessions.find((session) => session.id === target.id)
    if (!anchor) return null
    parentId = anchor.groupId
    key = groupKey(parentId)
  } else {
    const index = tree.groups.findIndex((group) => group.id === target.id)
    const anchor = tree.groups[index]
    if (!anchor || anchor.id === groupId) return null
    const hasChildren =
      tree.groups.some(childOf(anchor.id)) ||
      tree.sessions.some((session) => session.groupId === anchor.id)
    effective = zone === 'after' && !anchor.collapsed && hasChildren ? 'into' : zone
    key = groupKey(anchor.id)
    if (effective === 'into') parentId = anchor.id
    else {
      parentId = anchor.parentId
      beforeGroupId = nextId(
        tree.groups,
        effective === 'before' ? index - 1 : index,
        skip,
        childOf(parentId),
      )
    }
  }
  if (!canNestGroup(tree.groups, groupId, parentId)) return null
  const next = groupsAfterMove(tree.groups, groupId, parentId, beforeGroupId)
  if (!next) return null
  if (
    source.parentId === parentId &&
    sameOrder(next.filter(childOf(parentId)), tree.groups.filter(childOf(parentId)))
  )
    return null
  return {
    key,
    zone: effective,
    command: { type: 'moveGroup', groupId, parentId, beforeGroupId },
  }
}

/** Drop requests on the tab bar. `targetId` null means no tabs are open. */
export function resolveTabDrop(
  payload: DragPayload,
  targetId: number | null,
  zone: DropZone,
  openIds: number[],
): TabDrop | null {
  if (payload.kind !== 'requests' || !payload.ids.length) return null
  const ids = [...new Set(payload.ids)]
  const moving = new Set(ids)
  const index = targetId === null ? openIds.length : openIds.indexOf(targetId)
  if (index < 0) return null
  const start = zone === 'before' ? index : index + 1
  const beforeId = openIds.slice(start).find((id) => !moving.has(id)) ?? null
  const remaining = openIds.filter((id) => !moving.has(id))
  const at = beforeId === null ? remaining.length : remaining.indexOf(beforeId)
  const next = [...remaining.slice(0, at), ...ids, ...remaining.slice(at)]
  if (next.length === openIds.length && next.every((id, position) => id === openIds[position]))
    return null
  return {
    key: targetId === null ? 'strip' : `tab-${targetId}`,
    zone,
    ids,
    beforeId,
  }
}

/** Keyboard move: new place for `ids` one row up or down inside their group. */
export function stepRequests(
  sessions: GroupedSession[],
  ids: number[],
  direction: -1 | 1,
): { groupId: number | null; beforeId: number | null } | null {
  const moving = new Set(ids)
  const first = sessions.find((session) => moving.has(session.id))
  if (
    !first ||
    sessions.some((session) => moving.has(session.id) && session.groupId !== first.groupId)
  )
    return null
  const groupId = first.groupId
  const siblings = sessions.filter((session) => session.groupId === groupId)
  const positions = siblings.flatMap((session, index) => (moving.has(session.id) ? [index] : []))
  if (direction < 0) {
    const previous = siblings[positions[0] - 1]
    return previous ? { groupId, beforeId: previous.id } : null
  }
  const last = positions[positions.length - 1]
  if (last >= siblings.length - 1) return null
  return { groupId, beforeId: siblings[last + 2]?.id ?? null }
}

/** Keyboard move: new place for tab `id` one step left or right. */
export function stepTab(
  openIds: number[],
  id: number,
  direction: -1 | 1,
): { beforeId: number | null } | null {
  const index = openIds.indexOf(id)
  const target = index + direction
  if (index < 0 || target < 0 || target >= openIds.length) return null
  return {
    beforeId: direction < 0 ? openIds[target] : (openIds[target + 1] ?? null),
  }
}
