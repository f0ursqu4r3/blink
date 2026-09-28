/**
 * Tests for useRequestRunner reactive context source.
 *
 * Uses effectScope + computed to simulate the reactive setup the UI does.
 * Exercises: MaybeRefOrGetter (ref, computed, getter), stale fingerprint
 * derived from resolved RequestInput (not raw draftFingerprint), and
 * backward-compatible no-context usage.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { ref, computed, effectScope, nextTick, type EffectScope } from "vue";
import { useRequestRunner } from "@/composables/useRequestRunner";
import { createSession, requestFingerprint } from "@/lib/session";
import type { ResolvedRequestContext } from "@/lib/authorization";
import { sendRequest } from "@/lib/transport";
import { releaseResponse } from "@/lib/response-body";
import { defaultTransportOptions } from "@/lib/transport-options";

// Mock transport so send() doesn't make real network calls
vi.mock("@/lib/transport", () => ({
  sendRequest: vi.fn(() =>
    Promise.resolve({
      status: 200,
      statusText: "OK",
      durationMs: 5,
      sizeBytes: 2,
      headers: [],
      body: "{}",
    }),
  ),
  nativeTransport: false,
}));
vi.mock("@/lib/response-body", () => ({ releaseResponse: vi.fn() }));

let scope: EffectScope;
beforeEach(() => {
  scope = effectScope();
});
afterEach(() => {
  scope.stop();
  vi.restoreAllMocks();
});

function makeCtx(authType: "none" | "bearer" = "none"): ResolvedRequestContext {
  return {
    auth:
      authType === "bearer"
        ? { type: "bearer", token: "tok" }
        : { type: "none" },
    definitions: {},
    workspaceDefinitions: {},
  };
}

describe("useRequestRunner – reactive context source", () => {
  it("works without context source (backward compat)", () => {
    const session = createSession();
    session.draft.url = "https://example.test";
    let runner: ReturnType<typeof useRequestRunner>;
    scope.run(() => {
      runner = useRequestRunner(session);
    });
    expect(runner!.prepared.value.error).toBe("");
    expect(runner!.prepared.value.request).not.toBeNull();
  });

  it("accepts a plain Ref as context source and tracks it", async () => {
    const session = createSession();
    session.draft.url = "https://example.test";
    const ctxRef = ref<ResolvedRequestContext | undefined>(makeCtx("none"));
    let runner: ReturnType<typeof useRequestRunner>;
    scope.run(() => {
      runner = useRequestRunner(session, ctxRef);
    });
    expect(runner!.prepared.value.request).not.toBeNull();
    // Swap to bearer context
    ctxRef.value = makeCtx("bearer");
    await nextTick();
    // prepared should now have picked up bearer auth
    const req = runner!.prepared.value.request;
    expect(req?.headers.some((h) => h.key === "Authorization")).toBe(true);
  });

  it("accepts a ComputedRef as context source", async () => {
    const session = createSession();
    session.draft.url = "https://example.test";
    const authType = ref<"none" | "bearer">("none");
    let runner: ReturnType<typeof useRequestRunner>;
    scope.run(() => {
      const ctxComputed = computed<ResolvedRequestContext>(() =>
        makeCtx(authType.value),
      );
      runner = useRequestRunner(session, ctxComputed);
    });
    expect(
      runner!.prepared.value.request?.headers.some(
        (h) => h.key === "Authorization",
      ),
    ).toBe(false);
    authType.value = "bearer";
    await nextTick();
    expect(
      runner!.prepared.value.request?.headers.some(
        (h) => h.key === "Authorization",
      ),
    ).toBe(true);
  });

  it("accepts a getter function as context source", async () => {
    const session = createSession();
    session.draft.url = "https://example.test";
    const ctxRef = ref<ResolvedRequestContext>(makeCtx("none"));
    let runner: ReturnType<typeof useRequestRunner>;
    scope.run(() => {
      runner = useRequestRunner(session, () => ctxRef.value);
    });
    expect(runner!.curl.value).not.toContain("Authorization");
    ctxRef.value = makeCtx("bearer");
    await nextTick();
    expect(runner!.curl.value).toContain("Authorization");
  });

  it("stale uses resolved fingerprint: group auth change marks stale", async () => {
    const session = createSession();
    session.draft.url = "https://example.test";
    const ctxRef = ref<ResolvedRequestContext>(makeCtx("none"));
    let runner: ReturnType<typeof useRequestRunner>;
    scope.run(() => {
      runner = useRequestRunner(session, ctxRef);
    });

    // Simulate what send() stores
    const req = runner!.prepared.value.request!;
    session.sentFingerprint = requestFingerprint(req, "none");
    session.response = {
      status: 200,
      statusText: "OK",
      durationMs: 5,
      sizeBytes: 2,
      headers: [],
      body: "{}",
    };

    expect(runner!.stale.value).toBe(false);

    // Change group auth → effective bearer auth
    ctxRef.value = makeCtx("bearer");
    await nextTick();

    // stale should now be true because resolved request now has Authorization header
    expect(runner!.stale.value).toBe(true);
  });

  it("stale is false when response matches current resolved fingerprint", async () => {
    const session = createSession();
    session.draft.url = "https://example.test";
    const ctxRef = ref<ResolvedRequestContext>(makeCtx("none"));
    let runner: ReturnType<typeof useRequestRunner>;
    scope.run(() => {
      runner = useRequestRunner(session, ctxRef);
    });

    const req = runner!.prepared.value.request!;
    session.sentFingerprint = requestFingerprint(req, "none");
    session.response = {
      status: 200,
      statusText: "OK",
      durationMs: 5,
      sizeBytes: 2,
      headers: [],
      body: "{}",
    };

    expect(runner!.stale.value).toBe(false);
  });

  it("stale is false with no response", async () => {
    const session = createSession();
    session.draft.url = "https://example.test";
    let runner: ReturnType<typeof useRequestRunner>;
    scope.run(() => {
      runner = useRequestRunner(session, () => makeCtx("none"));
    });
    expect(runner!.stale.value).toBe(false);
  });

  it("send() stores requestFingerprint (resolved) not draftFingerprint", async () => {
    const session = createSession();
    session.draft.url = "https://example.test";
    const ctxRef = ref<ResolvedRequestContext>(makeCtx("bearer"));
    let runner: ReturnType<typeof useRequestRunner>;
    scope.run(() => {
      runner = useRequestRunner(session, ctxRef);
    });

    await runner!.send();
    // sentFingerprint must contain the Authorization header from resolved bearer
    expect(session.sentFingerprint).toContain("Authorization");
    // And must not equal just the draft fingerprint (which has no Authorization)
    const { draftFingerprint } = await import("@/lib/session");
    expect(session.sentFingerprint).not.toBe(draftFingerprint(session.draft));
  });
});

describe("sentUrl", () => {
  it("captures the resolved request URL when a send starts", async () => {
    const session = createSession();
    session.draft.url = "https://example.test/{{path}}";
    const ctxRef = ref<ResolvedRequestContext>({
      auth: { type: "none" },
      definitions: { path: "widgets" },
      workspaceDefinitions: {},
    });
    let runner: ReturnType<typeof useRequestRunner>;
    scope.run(() => {
      runner = useRequestRunner(session, ctxRef);
    });
    expect(runner!.sentUrl.value).toBe("");
    await runner!.send();
    expect(runner!.sentUrl.value).toBe(runner!.prepared.value.request!.url);
    expect(runner!.sentUrl.value).toBe("https://example.test/widgets");
  });
});

describe("transport options and body release", () => {
  it("sends with the given options and releases the previous body", async () => {
    const session = createSession();
    session.draft.url = "https://example.test/";
    const previous = {
      status: 200,
      statusText: "OK",
      durationMs: 1,
      sizeBytes: 0,
      headers: [],
      body: "",
      bodyId: "old",
    };
    session.response = previous;
    const options = { ...defaultTransportOptions(), timeoutSeconds: 7 };
    const { send } = scope.run(() =>
      useRequestRunner(session, undefined, () => options),
    )!;
    await send();
    expect(vi.mocked(releaseResponse)).toHaveBeenCalledWith(previous);
    const calls = vi.mocked(sendRequest).mock.calls;
    expect(calls[calls.length - 1][1]).toEqual(options);
  });

  it("releases a result that arrives after the scope stops", async () => {
    let resolve!: (value: unknown) => void;
    vi.mocked(sendRequest).mockImplementationOnce(
      () => new Promise((done) => (resolve = done)) as never,
    );
    const session = createSession();
    session.draft.url = "https://example.test/";
    const local = effectScope();
    const { send } = local.run(() => useRequestRunner(session))!;
    const pending = send();
    local.stop();
    const late = {
      status: 200,
      statusText: "OK",
      durationMs: 1,
      sizeBytes: 0,
      headers: [],
      body: "",
      bodyId: "late",
    };
    resolve(late);
    await pending;
    expect(vi.mocked(releaseResponse)).toHaveBeenCalledWith(late);
    expect(session.response).toBeNull();
  });
});
