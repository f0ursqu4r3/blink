import { afterEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { buildSchema, introspectionFromSchema } from "graphql";
import { clearSchemaCache } from "@/lib/graphql-schema";
import RequestEditor from "../RequestEditor.vue";
import { createDraft } from "@/lib/request";

vi.mock("../CodeEditor.vue", () => import("./code-editor-stub"));
vi.mock("@/lib/transport", () => ({
  sendRequest: vi.fn(),
  nativeTransport: false,
}));
import { sendRequest } from "@/lib/transport";
const send = vi.mocked(sendRequest);
const introspection = JSON.stringify({
  data: introspectionFromSchema(buildSchema("type Query { viewer: ID }")),
});
function respond(body: string, status = 200, statusText = "OK") {
  send.mockResolvedValueOnce({
    status,
    statusText,
    durationMs: 1,
    sizeBytes: body.length,
    headers: [],
    body,
  });
}

const wrappers: ReturnType<typeof mount>[] = [];
afterEach(() => {
  wrappers.splice(0).forEach((w) => w.unmount());
  send.mockReset();
  clearSchemaCache();
});

const formatButton = (w: ReturnType<typeof mount>) =>
  w.findAll("[data-body-actions] button").find((b) => b.text() === "Format")!;

function mountEditor(bodyMode: "json" | "graphql") {
  const draft = {
    ...createDraft(),
    method: "POST" as const,
    bodyMode,
    url: "https://api.test/graphql",
  };
  const wrapper = mount(RequestEditor, {
    attachTo: document.body,
    props: { modelValue: draft, tab: "body", busy: false },
  });
  wrappers.push(wrapper);
  return { wrapper, draft };
}

describe("RequestEditor – GraphQL body", () => {
  it("offers GraphQL in the body mode select", () => {
    const { wrapper } = mountEditor("json");
    const options = wrapper
      .findAll("select[id$='-body-mode'] option")
      .map((o) => o.attributes("value"));
    expect(options).toContain("graphql");
  });
  it("shows a variables editor only in GraphQL mode", async () => {
    const { wrapper, draft } = mountEditor("json");
    expect(wrapper.find("textarea[id$='-variables']").exists()).toBe(false);
    await wrapper.setProps({ modelValue: { ...draft, bodyMode: "graphql" } });
    const variables = wrapper.find("textarea[id$='-variables']");
    expect(variables.exists()).toBe(true);
    expect(wrapper.find("label[for$='-body']").text()).toBe("GraphQL query");
    await variables.setValue('{"id":"1"}');
    expect(wrapper.props("modelValue").variables).toBe('{"id":"1"}');
  });
  it("formats the GraphQL query and variables", async () => {
    const { wrapper } = mountEditor("graphql");
    await wrapper.setProps({
      modelValue: {
        ...wrapper.props("modelValue"),
        body: "{viewer{id}}",
        variables: '{"a":1}',
      },
    });
    await formatButton(wrapper).trigger("click");
    await vi.waitFor(() =>
      expect(wrapper.props("modelValue").body).toBe(
        "{\n  viewer {\n    id\n  }\n}",
      ),
    );
    expect(wrapper.props("modelValue").variables).toBe('{\n  "a": 1\n}');
  });
  it("keeps the body unchanged when GraphQL is invalid", async () => {
    const { wrapper } = mountEditor("graphql");
    await wrapper.setProps({
      modelValue: { ...wrapper.props("modelValue"), body: "query {" },
    });
    await formatButton(wrapper).trigger("click");
    await vi.waitFor(() =>
      expect(wrapper.find("[role='alert']").text()).toContain(
        "Invalid GraphQL",
      ),
    );
    expect(wrapper.props("modelValue").body).toBe("query {");
  });
  it("uses a JSON code editor for JSON bodies", () => {
    const { wrapper } = mountEditor("json");
    const body = wrapper.find("[data-testid='body-editor']");
    expect(body.attributes("data-language")).toBe("json");
    expect(body.attributes("aria-labelledby")).toMatch(/-body-label$/);
  });
  it("uses GraphQL and JSON editors in GraphQL mode", () => {
    const { wrapper } = mountEditor("graphql");
    expect(
      wrapper.find("[data-testid='body-editor']").attributes("data-language"),
    ).toBe("graphql");
    expect(
      wrapper
        .find("[data-testid='variables-editor']")
        .attributes("data-language"),
    ).toBe("json");
  });
  it("keeps a plain textarea for text bodies", async () => {
    const { wrapper, draft } = mountEditor("json");
    await wrapper.setProps({ modelValue: { ...draft, bodyMode: "text" } });
    expect(wrapper.find("[data-testid='body-editor']").exists()).toBe(false);
    expect(wrapper.find("textarea[id$='-body']").exists()).toBe(true);
  });
  it("disables the code editors while busy", async () => {
    const { wrapper } = mountEditor("graphql");
    await wrapper.setProps({ busy: true });
    expect(
      wrapper.find("[data-testid='body-editor']").attributes("disabled"),
    ).toBeDefined();
    expect(
      wrapper.find("[data-testid='variables-editor']").attributes("disabled"),
    ).toBeDefined();
  });
  it("clears the format error when the body is edited", async () => {
    const { wrapper } = mountEditor("json");
    await wrapper.setProps({
      modelValue: { ...wrapper.props("modelValue"), body: "{" },
    });
    await formatButton(wrapper).trigger("click");
    await vi.waitFor(() =>
      expect(wrapper.find("[role='alert']").exists()).toBe(true),
    );
    await wrapper.find("[data-testid='body-editor']").setValue("{}");
    expect(wrapper.find("[role='alert']").exists()).toBe(false);
  });
  const fetchButton = (w: ReturnType<typeof mount>) =>
    w.findAll("button").find((b) => b.text().includes("Fetch schema"));

  it("shows Fetch schema only in GraphQL mode", async () => {
    const { wrapper, draft } = mountEditor("json");
    expect(fetchButton(wrapper)).toBeUndefined();
    await wrapper.setProps({ modelValue: { ...draft, bodyMode: "graphql" } });
    expect(fetchButton(wrapper)).toBeDefined();
  });
  it("disables Fetch schema while busy or without a URL", async () => {
    const { wrapper, draft } = mountEditor("graphql");
    await wrapper.setProps({ busy: true });
    expect(fetchButton(wrapper)!.attributes("disabled")).toBeDefined();
    await wrapper.setProps({ busy: false, modelValue: { ...draft, url: " " } });
    expect(fetchButton(wrapper)!.attributes("disabled")).toBeDefined();
  });
  it("loads a schema and passes it to the query editor", async () => {
    const { wrapper } = mountEditor("graphql");
    const before = JSON.stringify(wrapper.props("modelValue"));
    respond(introspection);
    await fetchButton(wrapper)!.trigger("click");
    await flushPromises();
    expect(wrapper.find("[data-testid='schema-status']").text()).toBe(
      "Schema loaded · just now",
    );
    expect(
      wrapper.find("[data-testid='body-editor']").attributes("data-schema"),
    ).toBe("loaded");
    expect(
      wrapper
        .find("[data-testid='variables-editor']")
        .attributes("data-schema"),
    ).toBe("none");
    expect(JSON.stringify(wrapper.props("modelValue"))).toBe(before);
  });
  it("ignores clicks while a fetch is running", async () => {
    const { wrapper } = mountEditor("graphql");
    let finish!: () => void;
    send.mockReturnValueOnce(
      new Promise((resolve) => {
        finish = () =>
          resolve({
            status: 200,
            statusText: "OK",
            durationMs: 1,
            sizeBytes: introspection.length,
            headers: [],
            body: introspection,
          });
      }),
    );
    await fetchButton(wrapper)!.trigger("click");
    expect(fetchButton(wrapper)!.attributes("disabled")).toBeDefined();
    await fetchButton(wrapper)!.trigger("click");
    finish();
    await flushPromises();
    expect(send).toHaveBeenCalledTimes(1);
  });
  it("shows fetch errors and keeps the editor usable", async () => {
    const { wrapper } = mountEditor("graphql");
    respond("nope", 401, "Unauthorized");
    await fetchButton(wrapper)!.trigger("click");
    await flushPromises();
    const error = wrapper.find("[data-testid='schema-error']");
    expect(error.attributes("role")).toBe("alert");
    expect(error.text()).toBe("Schema request failed: 401 Unauthorized");
    expect(
      wrapper.find("[data-testid='body-editor']").attributes("disabled"),
    ).toBeUndefined();
  });
  it("shows buildRequest errors from Fetch schema", async () => {
    const { wrapper, draft } = mountEditor("graphql");
    await wrapper.setProps({ modelValue: { ...draft, url: "not a url" } });
    await fetchButton(wrapper)!.trigger("click");
    await flushPromises();
    expect(wrapper.find("[data-testid='schema-error']").text()).toContain(
      "Enter an absolute",
    );
    expect(send).not.toHaveBeenCalled();
  });
  it("drops the schema and error when the URL changes", async () => {
    const { wrapper } = mountEditor("graphql");
    respond(introspection);
    await fetchButton(wrapper)!.trigger("click");
    await flushPromises();
    await wrapper.setProps({
      modelValue: {
        ...wrapper.props("modelValue"),
        url: "https://other.test/graphql",
      },
    });
    expect(wrapper.find("[data-testid='schema-status']").exists()).toBe(false);
    expect(
      wrapper.find("[data-testid='body-editor']").attributes("data-schema"),
    ).toBe("none");
  });
  it("renders with an unresolved URL token", () => {
    const draft = {
      ...createDraft(),
      method: "POST" as const,
      bodyMode: "graphql" as const,
      url: "https://{{missing}}/graphql",
    };
    const wrapper = mount(RequestEditor, {
      attachTo: document.body,
      props: { modelValue: draft, tab: "body", busy: false },
    });
    wrappers.push(wrapper);
    expect(wrapper.find("[data-testid='body-editor']").exists()).toBe(true);
  });
  it("updates the schema age as time passes", async () => {
    vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
    try {
      const { wrapper } = mountEditor("graphql");
      respond(introspection);
      await fetchButton(wrapper)!.trigger("click");
      await flushPromises();
      expect(wrapper.find("[data-testid='schema-status']").text()).toBe(
        "Schema loaded · just now",
      );
      vi.advanceTimersByTime(3 * 60_000);
      await flushPromises();
      expect(wrapper.find("[data-testid='schema-status']").text()).toBe(
        "Schema loaded · 3m ago",
      );
    } finally {
      vi.useRealTimers();
    }
  });
});
