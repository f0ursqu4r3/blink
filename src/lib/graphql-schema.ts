import { markRaw, shallowReactive } from "vue";
import type { GraphQLSchema, IntrospectionQuery } from "graphql";
import { buildRequest, type Draft } from "./request";
import type { ResolvedRequestContext } from "./authorization";
import { sendRequest } from "./transport";
import { releaseResponse } from "./response-body";
import type { TransportOptions } from "./transport-options";

export type CachedSchema = { schema: GraphQLSchema; fetchedAt: number };

const NOT_INTROSPECTION =
  "Endpoint did not return an introspection result. Introspection may be disabled.";

/** In memory only, keyed by resolved URL. Schemas are refetched after restart. */
const cache = shallowReactive(new Map<string, CachedSchema>());

// graphql loads on first fetch to keep startup lean; the key needs only the URL.
const introspectionDraft = (draft: Draft, body = "{__typename}"): Draft => ({
  ...draft,
  method: "POST",
  bodyMode: "graphql",
  body,
  variables: "",
});

export function schemaKey(
  draft: Draft,
  ctx?: ResolvedRequestContext,
): string | null {
  try {
    return buildRequest(introspectionDraft(draft), ctx).url;
  } catch {
    return null;
  }
}

export const getCachedSchema = (key: string) => cache.get(key);
export const clearSchemaCache = () => cache.clear();

export async function fetchSchema(
  draft: Draft,
  ctx?: ResolvedRequestContext,
  options?: TransportOptions,
): Promise<GraphQLSchema> {
  const { buildClientSchema, getIntrospectionQuery } = await import("graphql");
  const request = buildRequest(
    introspectionDraft(draft, getIntrospectionQuery()),
    ctx,
  );
  const response = await sendRequest(request, options);
  try {
    if (response.status < 200 || response.status >= 300)
      throw new Error(
        `Schema request failed: ${response.status} ${response.statusText}`.trim(),
      );
    if (response.truncated)
      throw new Error("Schema response exceeds the inspection limit.");
    const schema = markRaw(
      parseIntrospection(response.body, buildClientSchema),
    );
    cache.set(request.url, { schema, fetchedAt: Date.now() });
    return schema;
  } finally {
    releaseResponse(response);
  }
}

type IntrospectionPayload = {
  data?: { __schema?: unknown } | null;
  errors?: { message?: unknown }[];
} | null;

function parseIntrospection(
  body: string,
  buildClientSchema: (data: IntrospectionQuery) => GraphQLSchema,
): GraphQLSchema {
  let payload: IntrospectionPayload;
  try {
    payload = JSON.parse(body);
  } catch {
    throw new Error(NOT_INTROSPECTION);
  }
  if (!payload?.data?.__schema) {
    const message = payload?.errors?.[0]?.message;
    throw new Error(typeof message === "string" ? message : NOT_INTROSPECTION);
  }
  try {
    return buildClientSchema(payload.data as unknown as IntrospectionQuery);
  } catch {
    throw new Error(NOT_INTROSPECTION);
  }
}

export function formatSchemaAge(fetchedAt: number, now: number): string {
  const minutes = Math.floor((now - fetchedAt) / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m ago`;
  return `${Math.floor(minutes / 60)}h ago`;
}
