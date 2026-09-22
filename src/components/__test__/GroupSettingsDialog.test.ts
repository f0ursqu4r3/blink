import { describe, it, expect } from "vitest";
import { mount } from "@vue/test-utils";
import GroupSettingsDialog from "../GroupSettingsDialog.vue";
import type { RequestGroup } from "../../lib/groups";
import type { RequestSession } from "../../lib/session";

// reka-ui Dialog uses Teleport + portals; stub them for jsdom
const mkGroup = (overrides: Partial<RequestGroup> = {}): RequestGroup => ({
  id: 1,
  name: "Root",
  parentId: null,
  collapsed: false,
  ...overrides,
});

const mkSession = (overrides: Partial<RequestSession> = {}): RequestSession =>
  ({
    id: 1,
    groupId: 1,
    draft: {
      method: "GET",
      url: "",
      headers: [],
      body: "",
      localAuth: undefined,
    },
    response: null,
    busy: false,
    error: "",
    elapsed: 0,
    sentFingerprint: "",
    view: {
      requestTab: "query",
      responseTab: "body",
      pretty: true,
      wrap: false,
      responseScroll: 0,
    },
    ...overrides,
  }) as RequestSession;

const defaultProps = {
  group: mkGroup(),
  groups: [mkGroup()],
  sessions: [mkSession()],
  globalDefinitions: {},
  open: true,
};

function mountDialog(props = {}) {
  return mount(GroupSettingsDialog, {
    props: { ...defaultProps, ...props },
    global: {
      stubs: {
        Teleport: true,
      },
    },
    attachTo: document.body,
  });
}

describe("GroupSettingsDialog", () => {
  describe("visibility", () => {
    it("renders dialog content when open=true", () => {
      const w = mountDialog({ open: true });
      expect(w.find('[data-testid="group-settings-dialog"]').exists()).toBe(
        true,
      );
    });

    it("does not render dialog content when open=false", () => {
      const w = mountDialog({ open: false });
      expect(w.find('[data-testid="group-settings-dialog"]').exists()).toBe(
        false,
      );
    });
  });

  describe("General section", () => {
    it("shows editable group name input", () => {
      const w = mountDialog();
      const input = w.find('[data-testid="group-name-input"]');
      expect(input.exists()).toBe(true);
      expect((input.element as HTMLInputElement).value).toBe("Root");
    });

    it("shows parent breadcrumb for root group (no parent)", () => {
      const w = mountDialog();
      const breadcrumb = w.find('[data-testid="parent-breadcrumb"]');
      expect(breadcrumb.exists()).toBe(true);
      expect(breadcrumb.text()).toContain("(root)");
    });

    it("shows parent breadcrumb chain for nested group", () => {
      const parent = mkGroup({ id: 2, name: "Browser", parentId: null });
      const child = mkGroup({ id: 3, name: "Platform", parentId: 2 });
      const group = mkGroup({ id: 4, name: "Chrome", parentId: 3 });
      const w = mountDialog({
        group,
        groups: [parent, child, group],
      });
      const breadcrumb = w.find('[data-testid="parent-breadcrumb"]');
      expect(breadcrumb.text()).toContain("Browser");
      expect(breadcrumb.text()).toContain("Platform");
    });

    it("shows effective auth source label without credentials", () => {
      const group = mkGroup({
        id: 1,
        localAuth: { type: "bearer", token: "secret-token" },
      });
      const w = mountDialog({ group, groups: [group] });
      const label = w.find('[data-testid="effective-auth-label"]');
      expect(label.exists()).toBe(true);
      expect(label.text()).toContain("Bearer");
      expect(label.text()).not.toContain("secret-token");
    });

    it("shows inherited auth source label", () => {
      const parent = mkGroup({
        id: 2,
        name: "Parent",
        parentId: null,
        localAuth: { type: "bearer", token: "tok" },
      });
      const child = mkGroup({ id: 3, name: "Child", parentId: 2 });
      const w = mountDialog({ group: child, groups: [parent, child] });
      const label = w.find('[data-testid="effective-auth-label"]');
      expect(label.text()).toContain("Inherited");
      expect(label.text()).toContain("Bearer");
      expect(label.text()).not.toContain("tok");
    });

    it("shows no-auth label when no auth configured anywhere", () => {
      const w = mountDialog();
      const label = w.find('[data-testid="effective-auth-label"]');
      expect(label.text()).toContain("No auth");
    });

    it("shows descendant request count", () => {
      const groups = [mkGroup({ id: 1 }), mkGroup({ id: 2, parentId: 1 })];
      const sessions = [
        mkSession({ id: 1, groupId: 1 }),
        mkSession({ id: 2, groupId: 2 }),
        mkSession({ id: 3, groupId: 2 }),
      ];
      const w = mountDialog({ groups, sessions });
      const counts = w.find('[data-testid="descendant-counts"]');
      // group 1: direct session + 2 in child = 3 total requests, 1 child group
      expect(counts.text()).toMatch(/3/);
    });

    it("shows descendant group count", () => {
      const groups = [
        mkGroup({ id: 1 }),
        mkGroup({ id: 2, parentId: 1 }),
        mkGroup({ id: 3, parentId: 1 }),
      ];
      const sessions: RequestSession[] = [];
      const w = mountDialog({ groups, sessions });
      const counts = w.find('[data-testid="descendant-counts"]');
      expect(counts.text()).toMatch(/2/);
    });
  });

  describe("Authorization section", () => {
    it("shows inherit option selected by default when localAuth is undefined", () => {
      const group = mkGroup({ localAuth: undefined });
      const w = mountDialog({ group, groups: [group] });
      const inheritOpt = w.find('[data-testid="auth-option-inherit"]');
      expect((inheritOpt.element as HTMLInputElement).checked).toBe(true);
    });

    it('shows no-auth option selected when localAuth is {type:"none"}', () => {
      const group = mkGroup({ localAuth: { type: "none" } });
      const w = mountDialog({ group, groups: [group] });
      const noneOpt = w.find('[data-testid="auth-option-none"]');
      expect((noneOpt.element as HTMLInputElement).checked).toBe(true);
    });

    it("shows bearer option and token input when localAuth is bearer", () => {
      const group = mkGroup({
        localAuth: { type: "bearer", token: "mytoken" },
      });
      const w = mountDialog({ group, groups: [group] });
      const bearerOpt = w.find('[data-testid="auth-option-bearer"]');
      expect((bearerOpt.element as HTMLInputElement).checked).toBe(true);
      const tokenInput = w.find('[data-testid="bearer-token-input"]');
      expect(tokenInput.exists()).toBe(true);
      expect(tokenInput.attributes("type")).toBe("password");
    });

    it("shows basic option and username/password inputs when localAuth is basic", () => {
      const group = mkGroup({
        localAuth: { type: "basic", username: "user", password: "pass" },
      });
      const w = mountDialog({ group, groups: [group] });
      const basicOpt = w.find('[data-testid="auth-option-basic"]');
      expect((basicOpt.element as HTMLInputElement).checked).toBe(true);
      expect(w.find('[data-testid="basic-username-input"]').exists()).toBe(
        true,
      );
      expect(w.find('[data-testid="basic-password-input"]').exists()).toBe(
        true,
      );
      expect(
        w.find('[data-testid="basic-password-input"]').attributes("type"),
      ).toBe("password");
    });

    it("selecting bearer shows token input", async () => {
      const group = mkGroup({ localAuth: undefined });
      const w = mountDialog({ group, groups: [group] });
      await w.find('[data-testid="auth-option-bearer"]').setValue(true);
      expect(w.find('[data-testid="bearer-token-input"]').exists()).toBe(true);
    });

    it("selecting basic shows username and password inputs", async () => {
      const group = mkGroup({ localAuth: undefined });
      const w = mountDialog({ group, groups: [group] });
      await w.find('[data-testid="auth-option-basic"]').setValue(true);
      expect(w.find('[data-testid="basic-username-input"]').exists()).toBe(
        true,
      );
      expect(w.find('[data-testid="basic-password-input"]').exists()).toBe(
        true,
      );
    });

    it("selecting inherit hides credential inputs", async () => {
      const group = mkGroup({ localAuth: { type: "bearer", token: "tok" } });
      const w = mountDialog({ group, groups: [group] });
      await w.find('[data-testid="auth-option-inherit"]').setValue(true);
      expect(w.find('[data-testid="bearer-token-input"]').exists()).toBe(false);
      expect(w.find('[data-testid="basic-username-input"]').exists()).toBe(
        false,
      );
    });

    it("does not expose inherited credentials in text nodes", () => {
      const parent = mkGroup({
        id: 2,
        name: "Parent",
        parentId: null,
        localAuth: { type: "bearer", token: "supersecret" },
      });
      const child = mkGroup({
        id: 3,
        name: "Child",
        parentId: 2,
        localAuth: undefined,
      });
      const w = mountDialog({ group: child, groups: [parent, child] });
      expect(w.html()).not.toContain("supersecret");
    });
  });

  describe("Save / Cancel", () => {
    it("emits save with groupId and changed name", async () => {
      const group = mkGroup({ id: 42, name: "OldName" });
      const w = mountDialog({ group, groups: [group] });
      const nameInput = w.find('[data-testid="group-name-input"]');
      await nameInput.setValue("NewName");
      await w.find('[data-testid="save-button"]').trigger("click");
      const emitted = w.emitted("save");
      expect(emitted).toBeTruthy();
      expect(emitted![0][0]).toBe(42);
      expect((emitted![0][1] as any).name).toBe("NewName");
    });

    it("emits save with localAuth when auth changed to bearer", async () => {
      const group = mkGroup({ id: 5, localAuth: undefined });
      const w = mountDialog({ group, groups: [group] });
      await w.find('[data-testid="auth-option-bearer"]').setValue(true);
      const tokenInput = w.find('[data-testid="bearer-token-input"]');
      await tokenInput.setValue("mytoken");
      await w.find('[data-testid="save-button"]').trigger("click");
      const emitted = w.emitted("save");
      expect(emitted).toBeTruthy();
      expect((emitted![0][1] as any).localAuth).toEqual({
        type: "bearer",
        token: "mytoken",
      });
    });

    it("emits save with localAuth undefined when inherit selected", async () => {
      const group = mkGroup({
        id: 5,
        localAuth: { type: "bearer", token: "tok" },
      });
      const w = mountDialog({ group, groups: [group] });
      await w.find('[data-testid="auth-option-inherit"]').setValue(true);
      await w.find('[data-testid="save-button"]').trigger("click");
      const emitted = w.emitted("save");
      const changes = emitted![0][1] as any;
      // localAuth should be explicitly undefined (inherit)
      expect(Object.prototype.hasOwnProperty.call(changes, "localAuth")).toBe(
        true,
      );
      expect(changes.localAuth).toBeUndefined();
    });

    it("cancel emits update:open false without saving", async () => {
      const group = mkGroup({ id: 1, name: "Original" });
      const w = mountDialog({ group, groups: [group] });
      await w.find('[data-testid="group-name-input"]').setValue("Changed");
      await w.find('[data-testid="cancel-button"]').trigger("click");
      expect(w.emitted("save")).toBeFalsy();
      const openEmit = w.emitted("update:open");
      expect(openEmit).toBeTruthy();
      expect(openEmit![0][0]).toBe(false);
    });
  });

  describe("Tokens section", () => {
    it("shows local token definitions as JSON", () => {
      const group = mkGroup({
        localDefinitions: { mytoken: "abc", other: "xyz" },
      });
      const w = mountDialog({ group, groups: [group] });
      expect(
        (w.get("[data-local-token-json]").element as HTMLTextAreaElement).value,
      ).toContain('"mytoken"');
    });

    it("emits parsed local definitions from the JSON editor", async () => {
      const group = mkGroup({ id: 7, localDefinitions: {} });
      const w = mountDialog({ group, groups: [group] });
      await w
        .get("[data-local-token-json]")
        .setValue('{"mykey":"{{_.apiKey}}"}');
      await w.find('[data-testid="save-button"]').trigger("click");
      expect((w.emitted("save")![0][1] as any).localDefinitions).toEqual({
        mykey: "{{_.apiKey}}",
      });
    });

    it("rejects invalid local token JSON", async () => {
      const group = mkGroup({ id: 7, localDefinitions: {} });
      const w = mountDialog({ group, groups: [group] });
      await w.get("[data-local-token-json]").setValue('{"port":443}');
      await w.find('[data-testid="save-button"]').trigger("click");
      expect(w.get("[data-token-json-error]").text()).toContain(
        "string values",
      );
      expect(w.emitted("save")).toBeFalsy();
    });
  });

  describe("Integration", () => {
    it("bearer token resolves to effective auth label without exposing token value in any text node", () => {
      const group = mkGroup({
        id: 1,
        localAuth: { type: "bearer", token: "HIDDEN_TOKEN_VALUE" },
      });
      const w = mountDialog({ group, groups: [group] });
      // The label should say Bearer but not reveal the token
      const label = w.find('[data-testid="effective-auth-label"]');
      expect(label.text()).toContain("Bearer");
      // Check entire HTML - token must not appear in any text (could be in input value since it's local auth)
      // But it should not be in visible text labels
      const allText = w
        .findAll("*")
        .map((el) => Array.from(el.element.childNodes))
        .flat()
        .filter((n: ChildNode) => n.nodeType === Node.TEXT_NODE)
        .map((n: ChildNode) => n.textContent || "")
        .join("");
      expect(allText).not.toContain("HIDDEN_TOKEN_VALUE");
    });
  });
});
