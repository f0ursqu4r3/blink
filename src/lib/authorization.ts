/**
 * Authorization inheritance and resolved request context for Blink.
 *
 * AuthorizationConfig is a discriminated union:
 *   { type: "none" }                         — explicitly no auth
 *   { type: "bearer"; token: string }        — bearer token
 *   { type: "basic"; username: string; password: string } — basic auth
 *
 * Absence of localAuth on a draft or group means "inherit from parent".
 * Explicit { type: "none" } blocks inheritance.
 *
 * Resolution order: draft.localAuth > nearest group > ancestors > none.
 */

import type { RequestGroup } from './groups'
import { groupDefinitions } from './environments'
import type { InterpolationContext } from './interpolation'

export type AuthorizationConfig =
  | { type: 'none' }
  | { type: 'bearer'; token: string }
  | { type: 'basic'; username: string; password: string }

export type ResolvedRequestContext = InterpolationContext & {
  /** Fully resolved effective authorization for this request. */
  auth: AuthorizationConfig
}

/**
 * Walk the group ancestry (nearest first) and return the first
 * `localAuth` found. Returns `{ type: "none" }` if none found.
 */
export function resolveAuthorization(
  draftLocalAuth: AuthorizationConfig | undefined,
  groupId: number | null,
  groups: RequestGroup[],
): AuthorizationConfig {
  // Draft overrides everything (including explicit none)
  if (draftLocalAuth !== undefined) return draftLocalAuth

  // Walk ancestry from direct group upward
  const byId = new Map(groups.map((g) => [g.id, g]))
  let cursor: number | null = groupId
  const seen = new Set<number>()
  while (cursor !== null) {
    if (seen.has(cursor)) break
    seen.add(cursor)
    const g = byId.get(cursor)
    if (!g) break
    if (g.localAuth !== undefined) return g.localAuth
    cursor = g.parentId
  }
  return { type: 'none' }
}

/**
 * Build the merged token definitions for interpolation.
 * Nearest group wins for duplicate keys; workspace global is always separate.
 */
export function resolveTokenDefinitions(
  groupId: number | null,
  groups: RequestGroup[],
  workspaceGlobal: Record<string, string>,
): InterpolationContext {
  const byId = new Map(groups.map((g) => [g.id, g]))

  // Collect ancestor chain (nearest → root)
  const chain: RequestGroup[] = []
  let cursor: number | null = groupId
  const seen = new Set<number>()
  while (cursor !== null) {
    if (seen.has(cursor)) break
    seen.add(cursor)
    const g = byId.get(cursor)
    if (!g) break
    chain.push(g)
    cursor = g.parentId
  }

  // Merge: nearest wins (process chain in order, skip if key already set)
  const merged: Record<string, string> = {}
  for (const g of chain) {
    const definitions = groupDefinitions(g)
    if (definitions) {
      for (const [k, v] of Object.entries(definitions)) {
        if (!Object.prototype.hasOwnProperty.call(merged, k)) {
          merged[k] = v
        }
      }
    }
  }

  return {
    definitions: merged,
    workspaceDefinitions: workspaceGlobal,
  }
}

/**
 * Build a fully resolved request context combining resolved auth and tokens.
 */
export function buildResolvedRequestContext(
  draft: { localAuth?: AuthorizationConfig },
  groupId: number | null,
  groups: RequestGroup[],
  workspaceGlobal: Record<string, string>,
): ResolvedRequestContext {
  const auth = resolveAuthorization(draft.localAuth, groupId, groups)
  const interpCtx = resolveTokenDefinitions(groupId, groups, workspaceGlobal)
  return { auth, ...interpCtx }
}
