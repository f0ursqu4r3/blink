import {
  computed,
  ref,
  toValue,
  onScopeDispose,
  type MaybeRefOrGetter,
} from "vue";
import { buildRequest, toCurl } from "@/lib/request";
import type { ResolvedRequestContext } from "@/lib/authorization";
import { requestFingerprint, type RequestSession } from "@/lib/session";
import { CANCELED, sendRequest } from "@/lib/transport";
import { addHistory, historyEntry, nextHistoryId } from "@/lib/history";
import { runAssertions, runCaptures } from "@/lib/checks";
import { releaseResponse } from "@/lib/response-body";
import {
  defaultTransportOptions,
  type TransportOptions,
} from "@/lib/transport-options";

export type { MaybeRefOrGetter };

/**
 * useRequestRunner — manages the send/cURL/stale lifecycle for a single session.
 *
 * @param session  The reactive session object whose draft is the source of truth.
 * @param contextSource  Optional reactive source (Ref, ComputedRef, or getter)
 *   for a ResolvedRequestContext (auth + token definitions) built by the caller
 *   from group ancestry and workspace globals.
 *
 *   Because `toValue()` is called inside computed getters, any Ref or computed
 *   that wraps the context is automatically tracked: changes to group auth,
 *   global definitions, or the request ancestry will re-evaluate `prepared`,
 *   `curl`, and `stale` without the caller doing anything special.
 *
 *   When omitted, buildRequest falls back to the draft's flat auth fields
 *   (backward-compatible with callers that pass no context).
 * @param optionsSource  Optional reactive source for the transport settings.
 *   Defaults apply when omitted.
 */
export function useRequestRunner(
  session: RequestSession,
  contextSource?: MaybeRefOrGetter<ResolvedRequestContext | undefined>,
  optionsSource?: MaybeRefOrGetter<TransportOptions | undefined>,
  hooks: {
    /** Receives token values from the request captures after a send. */
    onCapture?: (values: Record<string, string>) => void;
  } = {},
) {
  let alive = true;
  let clock: ReturnType<typeof setInterval> | undefined;
  let controller: AbortController | undefined;
  // The URL actually sent, captured at send time so a later edit to the
  // draft (or an unresolved token placeholder) does not change the name
  // suggested for a saved response body.
  const sentUrl = ref("");

  const prepared = computed(() => {
    // toValue(undefined) → undefined; toValue(ref(ctx)) → ctx; toValue(() => ctx) → ctx
    const ctx =
      contextSource !== undefined ? toValue(contextSource) : undefined;
    try {
      return { request: buildRequest(session.draft, ctx), error: "", ctx };
    } catch (cause) {
      return {
        request: null,
        error: cause instanceof Error ? cause.message : String(cause),
        ctx,
      };
    }
  });

  const curl = computed(() =>
    prepared.value.request
      ? toCurl(prepared.value.request, toValue(optionsSource))
      : "",
  );

  /**
   * Stale: true when a response exists but the current fully-resolved
   * RequestInput (including effective auth) differs from what was sent.
   * Uses requestFingerprint (not draftFingerprint) so group-auth / token
   * definition changes are included.
   */
  const stale = computed(() => {
    if (!session.response) return false;
    const req = prepared.value.request;
    const ctx = prepared.value.ctx;
    const authType = ctx?.auth?.type;
    return session.sentFingerprint !== requestFingerprint(req, authType);
  });

  async function send() {
    if (session.busy || !prepared.value.request) return;
    const request = prepared.value.request;
    const ctx = prepared.value.ctx;
    const authType = ctx?.auth?.type;
    session.busy = true;
    session.error = "";
    releaseResponse(session.response);
    session.response = null;
    session.testResults = undefined;
    session.captureErrors = undefined;
    session.elapsed = 0;
    sentUrl.value = request.url;
    // Persist the resolved-request fingerprint so stale can compare accurately.
    session.sentFingerprint = requestFingerprint(request, authType);
    const start = performance.now();
    const sentAt = Date.now();
    clock = setInterval(() => {
      session.elapsed = performance.now() - start;
    }, 100);
    controller = new AbortController();
    try {
      const options = toValue(optionsSource) ?? defaultTransportOptions();
      const result = await sendRequest(request, options, {
        signal: controller.signal,
      });
      // A result for an unmounted view has no owner, so free its body.
      if (alive) session.response = result;
      else releaseResponse(result);
      session.history = addHistory(
        session.history,
        historyEntry(nextHistoryId(session.history), sentAt, request, {
          response: result,
        }),
      );
      if (alive) await check(result);
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      if (alive) session.error = message;
      if (message !== CANCELED)
        session.history = addHistory(
          session.history,
          historyEntry(nextHistoryId(session.history), sentAt, request, {
            error: message,
            durationMs: Math.round(performance.now() - start),
          }),
        );
    } finally {
      controller = undefined;
      clearInterval(clock);
      if (alive) session.busy = false;
    }
  }

  /** Run the assertions and captures of the draft on `result`. */
  async function check(result: import("@/lib/request").ApiResponse) {
    const { assertions, captures } = session.draft;
    if (assertions?.some((row) => row.enabled))
      session.testResults = await runAssertions(assertions, result);
    if (captures?.some((row) => row.enabled)) {
      const { values, errors } = await runCaptures(captures, result);
      session.captureErrors = errors.length ? errors : undefined;
      if (Object.keys(values).length) hooks.onCapture?.(values);
    }
  }

  /** Stop the running send. Its error reads "Request canceled.". */
  function cancel() {
    controller?.abort();
  }

  onScopeDispose(() => {
    alive = false;
    clearInterval(clock);
  });

  /** Run the checks again on the shown response. */
  async function recheck() {
    if (session.response && !session.busy) await check(session.response);
  }

  return { prepared, curl, stale, send, cancel, sentUrl, recheck };
}
