import { describe, expect, it } from "vitest";
import { createSession, draftFingerprint } from "../session";
import { encodeWorkspace, decodeWorkspace } from "../workspace";

describe("durable workspace snapshots", () => {
  it("round-trips complete tabs, selection, response and view preferences", () => {
    const first = createSession();
    const second = createSession();
    first.draft.url = "https://example.test";
    first.draft.auth = "bearer";
    first.draft.token = "synthetic";
    first.draft.body = '{"id":9223372036854775807}';
    first.view.requestTab = "body";
    first.view.pretty = false;
    first.view.wrap = true;
    first.view.responseScroll = 250;
    first.sentFingerprint = draftFingerprint(first.draft);
    first.response = {
      status: 200,
      statusText: "OK",
      durationMs: 8,
      sizeBytes: 2,
      headers: [{ key: "X-Test", value: "yes" }],
      body: "{}",
    };
    const result = decodeWorkspace(encodeWorkspace([first, second], second.id));
    expect(result.activeId).toBe(second.id);
    expect(result.sessions[0]).toMatchObject({
      draft: first.draft,
      view: first.view,
      response: first.response,
      sentFingerprint: first.sentFingerprint,
    });
    expect(createSession().id).toBeGreaterThan(second.id);
  });
  it("restores interrupted requests as idle errors without replaying them", () => {
    const session = createSession();
    session.busy = true;
    const result = decodeWorkspace(encodeWorkspace([session], session.id));
    expect(result.sessions[0].busy).toBe(false);
    expect(result.sessions[0].error).toContain("interrupted");
  });
  it("rejects corrupt, future and malformed snapshots instead of partially resetting them", () => {
    for (const content of [
      "{",
      '{"version":99}',
      '{"version":1,"tabs":[],"activeId":1}',
    ]) {
      expect(() => decodeWorkspace(content)).toThrow();
    }
    const session = createSession();
    const data = JSON.parse(encodeWorkspace([session], session.id));
    data.tabs[0].draft.auth = "invented";
    expect(() => decodeWorkspace(JSON.stringify(data))).toThrow();
    data.tabs[0].draft.auth = "none";
    data.tabs.push(data.tabs[0]);
    expect(() => decodeWorkspace(JSON.stringify(data))).toThrow();
  });
});
