import { buildResolvedRequestContext } from "./authorization";
import type { RequestGroup } from "./groups";
import { buildRequest, toCurl } from "./request";
import type { RequestSession } from "./session";

/**
 * The cURL command for a session, resolved as a send resolves it. Empty when
 * the draft does not build.
 */
export function sessionCurl(
  session: RequestSession,
  groups: RequestGroup[],
  globals: Record<string, string>,
) {
  try {
    const ctx = buildResolvedRequestContext(
      session.draft,
      session.groupId ?? null,
      groups,
      globals,
    );
    return toCurl(buildRequest(session.draft, ctx));
  } catch {
    return "";
  }
}
