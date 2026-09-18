import { describe, expect, it } from "vitest";
import {
  createSession,
  draftFingerprint,
  hasDraft,
  sessionHost,
  sessionLabel,
  sessionStatus,
} from "../session";

describe("request session metadata", () => {
  it("distinguishes endpoints without leaking URL credentials or query tokens", () => {
    const session = createSession();
    session.draft.url =
      "https://user:synthetic-secret@example.test/v1/health?token=synthetic-query#fragment";
    expect(sessionLabel(session)).toBe("/v1/health");
    expect(sessionHost(session)).toBe("example.test");
  });
  it("marks responses from an earlier draft as edited", () => {
    const session = createSession();
    expect(hasDraft(session)).toBe(false);
    session.draft.url = "https://example.test";
    expect(hasDraft(session)).toBe(true);
    session.sentFingerprint = draftFingerprint(session.draft);
    session.response = {
      status: 200,
      statusText: "OK",
      durationMs: 1,
      sizeBytes: 0,
      body: "",
      headers: [],
    };
    expect(sessionStatus(session)).toBe("200");
    session.draft.method = "POST";
    expect(sessionStatus(session)).toBe("Edited");
  });
  it("duplicates all credentials and body data without row aliasing or responses", () => {
    const first = createSession();
    first.draft.auth = "bearer";
    first.draft.token = "synthetic";
    first.draft.bodyMode = "json";
    first.draft.body = '{"id":9223372036854775807}';
    first.draft.headers[0].enabled = false;
    first.response = {
      status: 200,
      statusText: "OK",
      durationMs: 1,
      sizeBytes: 0,
      body: "",
      headers: [],
    };
    const second = createSession(first.draft);
    expect(draftFingerprint(second.draft)).toBe(draftFingerprint(first.draft));
    expect(second.draft.headers[0].id).not.toBe(first.draft.headers[0].id);
    expect(second.response).toBeNull();
    expect(second.busy).toBe(false);
    second.draft.token = "changed";
    expect(first.draft.token).toBe("synthetic");
  });
});
