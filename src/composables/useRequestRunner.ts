import { computed, onScopeDispose } from "vue";
import { buildRequest, toCurl } from "@/lib/request";
import { draftFingerprint, type RequestSession } from "@/lib/session";
import { sendRequest } from "@/lib/transport";

export function useRequestRunner(session: RequestSession) {
  let alive = true;
  let clock: ReturnType<typeof setInterval> | undefined;
  const prepared = computed(() => {
    try {
      return { request: buildRequest(session.draft), error: "" };
    } catch (cause) {
      return {
        request: null,
        error: cause instanceof Error ? cause.message : String(cause),
      };
    }
  });
  const curl = computed(() =>
    prepared.value.request ? toCurl(prepared.value.request) : "",
  );
  const stale = computed(
    () =>
      Boolean(session.response) &&
      session.sentFingerprint !== draftFingerprint(session.draft),
  );
  async function send() {
    if (session.busy || !prepared.value.request) return;
    const request = prepared.value.request;
    session.busy = true;
    session.error = "";
    session.response = null;
    session.elapsed = 0;
    session.sentFingerprint = draftFingerprint(session.draft);
    const start = performance.now();
    clock = setInterval(() => {
      session.elapsed = performance.now() - start;
    }, 100);
    try {
      const result = await sendRequest(request);
      if (alive) session.response = result;
    } catch (cause) {
      if (alive)
        session.error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      clearInterval(clock);
      if (alive) session.busy = false;
    }
  }
  onScopeDispose(() => {
    alive = false;
    clearInterval(clock);
  });
  return { prepared, curl, stale, send };
}
