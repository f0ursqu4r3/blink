import { describe, expect, it } from "vitest";
import {
  createSession,
  draftFingerprint,
  requestFingerprint,
} from "../session";
import { buildRequest } from "../request";
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
  it("round-trips nested groups and request membership", () => {
    const first = createSession();
    const second = createSession();
    const workspace = JSON.parse(encodeWorkspace([first, second], first.id));
    workspace.groups = [
      { id: 1, name: "Platform", parentId: null, collapsed: false },
      { id: 2, name: "Identity", parentId: 1, collapsed: true },
    ];
    workspace.tabs[0].groupId = 2;
    workspace.version = 2;

    const result = decodeWorkspace(JSON.stringify(workspace));

    expect(result.groups).toEqual(workspace.groups);
    expect(result.sessions[0].groupId).toBe(2);
  });
  it("migrates version 1 snapshots into the ungrouped browser section", () => {
    const session = createSession();
    const legacy = JSON.parse(encodeWorkspace([session], session.id));
    legacy.version = 1;
    delete legacy.groups;
    delete legacy.tabs[0].groupId;
    const result = decodeWorkspace(JSON.stringify(legacy));

    expect(result.groups).toEqual([]);
    expect(result.sessions[0].groupId).toBeNull();
  });
  it("migrates old response fingerprints to the resolved request format", () => {
    const session = createSession();
    session.draft.url = "https://example.test/users";
    session.response = {
      status: 200,
      statusText: "OK",
      durationMs: 8,
      sizeBytes: 2,
      headers: [],
      body: "{}",
    };
    session.sentFingerprint = draftFingerprint(session.draft);
    const legacy = JSON.parse(encodeWorkspace([session], session.id));
    legacy.version = 2;
    delete legacy.globalDefinitions;

    const result = decodeWorkspace(JSON.stringify(legacy));

    expect(result.sessions[0].sentFingerprint).toBe(
      requestFingerprint(buildRequest(result.sessions[0].draft), "none"),
    );
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
