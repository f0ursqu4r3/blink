<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref } from "vue";
import { ArrowUpRight, Check, ChevronDown, Terminal } from "lucide-vue-next";
import { Button } from "@/components/ui/button";
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
} from "@/components/ui/context-menu";
import RequestEditor from "./RequestEditor.vue";
import ResponsePanel from "./ResponsePanel.vue";
import { methods } from "@/lib/request";
import type { RequestSession } from "@/lib/session";
import type { RequestGroup } from "@/lib/groups";
import {
  buildResolvedRequestContext,
  resolveAuthorization,
} from "@/lib/authorization";
import { useRequestRunner } from "@/composables/useRequestRunner";
import { useClipboard } from "@/composables/useClipboard";
const props = defineProps<{
  session: RequestSession;
  active: boolean;
  groups?: RequestGroup[];
  globalDefinitions?: Record<string, string>;
}>();

const resolvedCtx = computed(() =>
  buildResolvedRequestContext(
    props.session.draft,
    props.session.groupId ?? null,
    props.groups ?? [],
    props.globalDefinitions ?? {},
  ),
);

const effectiveAuth = computed(() =>
  resolveAuthorization(
    props.session.draft.localAuth,
    props.session.groupId ?? null,
    props.groups ?? [],
  ),
);

const inheritedSource = computed(() => {
  if (props.session.draft.localAuth !== undefined) return undefined;
  const groups = props.groups ?? [];
  const byId = new Map(groups.map((g) => [g.id, g]));
  let cursor: number | null = props.session.groupId ?? null;
  const seen = new Set<number>();
  while (cursor !== null) {
    if (seen.has(cursor)) break;
    seen.add(cursor);
    const g = byId.get(cursor);
    if (!g) break;
    if (g.localAuth !== undefined) {
      const authLabel =
        g.localAuth.type === "bearer"
          ? "Bearer"
          : g.localAuth.type === "basic"
            ? "Basic"
            : "None";
      return `${g.name} · ${authLabel}`;
    }
    cursor = g.parentId;
  }
  return undefined;
});

const { prepared, curl, stale, send } = useRequestRunner(
  props.session,
  resolvedCtx,
);
const { copied, copyError, copy } = useClipboard();
const showCurl = ref(false);
const urlInput = ref<HTMLInputElement>();
const workspace = ref<HTMLElement>();
const requestPanelWidth = ref(420);
const resizing = ref(false);
const prefix = `request-${props.session.id}`;
const shortcut = /Mac/i.test(navigator.platform) ? "⌘" : "Ctrl";
const minimumPanelWidth = 280;
const minimumResponseWidth = 340;
const resizeHandleWidth = 8;
const maximumPanelWidth = computed(() =>
  Math.max(
    minimumPanelWidth,
    (workspace.value?.clientWidth ?? 900) -
      minimumResponseWidth -
      resizeHandleWidth,
  ),
);
const panelWidth = computed(() =>
  Math.round(
    Math.min(
      maximumPanelWidth.value,
      Math.max(minimumPanelWidth, requestPanelWidth.value),
    ),
  ),
);
const panelStyle = computed(() => ({
  "--request-panel-width": `${panelWidth.value}px`,
}));
async function focusUrl() {
  await nextTick();
  urlInput.value?.focus();
  urlInput.value?.select();
}
function onKey(event: KeyboardEvent) {
  if (
    !props.active ||
    event.isComposing ||
    event.repeat ||
    event.defaultPrevented ||
    event.altKey
  )
    return;
  if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
    event.preventDefault();
    void send();
  }
  if (event.key.toLowerCase() === "l" && (event.metaKey || event.ctrlKey)) {
    event.preventDefault();
    void focusUrl();
  }
  if (event.key === "Escape") showCurl.value = false;
}

onMounted(() => {
  window.addEventListener("keydown", onKey);
  if (props.active) void focusUrl();
});
onUnmounted(() => {
  window.removeEventListener("keydown", onKey);
  stopResize();
});

function setPanelWidth(width: number) {
  requestPanelWidth.value = Math.min(
    maximumPanelWidth.value,
    Math.max(minimumPanelWidth, width),
  );
}
function resizePointer(event: PointerEvent) {
  setPanelWidth(
    event.clientX - (workspace.value?.getBoundingClientRect().left ?? 0),
  );
}
function stopResize() {
  resizing.value = false;
  window.removeEventListener("pointermove", resizePointer);
  window.removeEventListener("pointerup", stopResize);
}
function startResize(event: PointerEvent) {
  if (event.button !== 0) return;
  event.preventDefault();
  resizing.value = true;
  window.addEventListener("pointermove", resizePointer);
  window.addEventListener("pointerup", stopResize, { once: true });
}
function resizeWithKeyboard(event: KeyboardEvent) {
  const step = event.shiftKey ? 48 : 16;
  if (event.key === "ArrowLeft") setPanelWidth(panelWidth.value - step);
  else if (event.key === "ArrowRight") setPanelWidth(panelWidth.value + step);
  else if (event.key === "Home") setPanelWidth(minimumPanelWidth);
  else if (event.key === "End") setPanelWidth(maximumPanelWidth.value);
  else return;
  event.preventDefault();
}
</script>

<template>
  <section
    ref="workspace"
    class="request-workspace"
    data-request-pane
    :data-active="active"
    :aria-hidden="!active"
    :id="`request-pane-${session.id}`"
    role="tabpanel"
    :aria-labelledby="`request-tab-${session.id}`"
  >
    <ContextMenu>
      <ContextMenuTrigger as-child>
        <form class="request-bar" @submit.prevent="send">
          <div class="endpoint">
            <div class="method-select" :data-http-method="session.draft.method">
              <label :for="`${prefix}-method`" class="sr-only"
                >HTTP method</label
              >
              <select
                :id="`${prefix}-method`"
                data-method
                v-model="session.draft.method"
                :disabled="session.busy"
              >
                <option v-for="method in methods" :key="method">
                  {{ method }}
                </option></select
              ><ChevronDown :size="12" aria-hidden="true" />
            </div>
            <label :for="`${prefix}-url`" class="sr-only">Request URL</label>
            <input
              :id="`${prefix}-url`"
              data-request-url
              ref="urlInput"
              v-model="session.draft.url"
              :disabled="session.busy"
              :aria-describedby="
                session.draft.url && prepared.error
                  ? `${prefix}-validation`
                  : undefined
              "
              type="text"
              inputmode="url"
              spellcheck="false"
              autocomplete="off"
              placeholder="https://api.example.com/v1/resource"
            />
          </div>
          <Button
            variant="secondary"
            class="curl-button"
            :disabled="!prepared.request"
            :aria-expanded="showCurl"
            :aria-controls="`${prefix}-curl`"
            aria-label="cURL"
            title="Inspect and copy cURL"
            @click="showCurl = !showCurl"
          >
            <Terminal :size="14" aria-hidden="true" /><span>cURL</span>
          </Button>
          <Button
            type="submit"
            data-send
            class="send-button"
            :disabled="!prepared.request || session.busy"
          >
            <ArrowUpRight :size="15" aria-hidden="true" /><span>{{
              session.busy ? "Sending" : "Send"
            }}</span
            ><kbd>{{ shortcut }} ↵</kbd>
          </Button>
        </form>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem
          data-testid="ctx-send"
          :disabled="session.busy || !prepared.request"
          @select="send()"
          >Send</ContextMenuItem
        >
        <ContextMenuItem data-testid="ctx-focus-url" @select="focusUrl()"
          >Focus URL</ContextMenuItem
        >
        <ContextMenuSeparator />
        <ContextMenuItem
          data-testid="ctx-show-curl"
          @select="showCurl = !showCurl"
          >cURL</ContextMenuItem
        >
      </ContextMenuContent>
    </ContextMenu>
    <p
      v-if="session.draft.url && prepared.error"
      :id="`${prefix}-validation`"
      data-request-validation
      class="validation-message"
      role="status"
    >
      {{ prepared.error }}
    </p>
    <ContextMenu>
      <ContextMenuTrigger as-child>
        <section
          v-if="showCurl"
          :id="`${prefix}-curl`"
          data-curl-preview
          class="curl-preview"
          aria-label="cURL export"
        >
          <div>
            <span>POSIX SHELL · INCLUDES CREDENTIALS</span
            ><Button variant="ghost" @click="copy(curl)"
              ><Check v-if="copied" :size="13" aria-hidden="true" />{{
                copied ? "Copied" : "Copy cURL"
              }}</Button
            >
          </div>
          <pre tabindex="0">{{ curl }}</pre>
          <p v-if="copyError" role="alert">{{ copyError }}</p>
        </section>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem data-testid="ctx-copy-curl" @select="copy(curl)"
          >Copy cURL</ContextMenuItem
        >
        <ContextMenuSeparator />
        <ContextMenuItem data-testid="ctx-close-curl" @select="showCurl = false"
          >Close</ContextMenuItem
        >
      </ContextMenuContent>
    </ContextMenu>
    <div class="panels" :class="{ resizing }" :style="panelStyle">
      <RequestEditor
        v-model="session.draft"
        v-model:tab="session.view.requestTab"
        :busy="session.busy"
        :effective-auth="effectiveAuth"
        :inherited-source="inheritedSource"
      />
      <div
        class="panel-resize"
        data-panel-resize
        role="separator"
        aria-label="Resize panels"
        aria-orientation="vertical"
        :aria-valuemin="minimumPanelWidth"
        :aria-valuemax="maximumPanelWidth"
        :aria-valuenow="panelWidth"
        tabindex="0"
        @pointerdown="startResize"
        @keydown="resizeWithKeyboard"
      />
      <ResponsePanel
        v-model:view="session.view"
        :active="active"
        :response="session.response"
        :busy="session.busy"
        :error="session.error"
        :elapsed="session.elapsed"
        :stale="stale"
      />
    </div>
  </section>
</template>

<style scoped>
.request-workspace {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  min-width: 0;
}
.request-workspace[data-active="false"] {
  position: absolute;
  inset: 0;
  visibility: hidden;
  pointer-events: none;
}
.request-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 14px;
  border-bottom: 1px solid var(--border);
  background: var(--secondary);
}
.endpoint {
  display: flex;
  align-items: stretch;
  flex: 1;
  min-width: 0;
  height: 34px;
  border: 1px solid var(--input);
  border-radius: 2px;
  background: var(--background);
}
.endpoint:focus-within {
  border-color: var(--primary);
}
.method-select {
  position: relative;
  border-right: 1px solid var(--border);
  flex-shrink: 0;
  color: var(--primary);
}
.method-select[data-http-method="GET"] {
  color: var(--success);
}
.method-select[data-http-method="DELETE"] {
  color: var(--destructive);
}
.method-select select {
  height: 100%;
  width: 94px;
  padding: 0 26px 0 12px;
  appearance: none;
  border: 0;
  background: transparent;
  color: inherit;
  font: 600 0.6875rem var(--font-mono);
}
.method-select svg {
  position: absolute;
  top: 10px;
  right: 8px;
  pointer-events: none;
  color: var(--muted-foreground);
}
[data-request-url] {
  flex: 1;
  min-width: 0;
  background: transparent;
  border: 0;
  padding: 0 12px;
  font: 0.75rem var(--font-mono);
}
.send-button,
.curl-button {
  height: 34px;
}
.send-button {
  padding: 0 12px;
}
kbd {
  opacity: 0.65;
  font-size: 0.625rem;
  margin-left: 10px;
}
.validation-message {
  padding: 8px 14px;
  color: var(--destructive);
  border-bottom: 1px solid var(--border);
  font: 0.6875rem/1.6 var(--font-mono);
}
.curl-preview {
  max-height: 200px;
  overflow: auto;
  padding: 8px 14px 12px;
  border-bottom: 1px solid var(--border);
  background: var(--muted);
}
.curl-preview > div {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font: 0.5625rem var(--font-mono);
  letter-spacing: 0.08em;
  color: var(--primary);
}
.curl-preview pre {
  font-size: 0.6875rem;
  line-height: 1.8;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
.curl-preview p {
  color: var(--destructive);
  font-size: 0.6875rem;
}
.panels {
  display: grid;
  grid-template-columns: minmax(280px, var(--request-panel-width)) 8px minmax(
      340px,
      1fr
    );
  min-height: 0;
  flex: 1;
}
.panels > :first-child {
  border-right: 1px solid var(--border);
}
.panel-resize {
  position: relative;
  z-index: 1;
  margin: 0 -3px;
  cursor: col-resize;
  outline-offset: -2px;
}
.panel-resize::after {
  position: absolute;
  top: 0;
  bottom: 0;
  left: 3px;
  width: 1px;
  background: var(--border);
  content: "";
}
.panel-resize:hover::after,
.panel-resize:focus-visible::after,
.panels.resizing .panel-resize::after {
  width: 2px;
  background: var(--primary);
}
.panels.resizing {
  user-select: none;
}
@media (max-width: 900px) {
  .panels {
    grid-template-columns: minmax(0, 1fr);
  }
  .panels > :first-child {
    border-right: 0;
    border-bottom: 1px solid var(--border);
    min-height: 300px;
    max-height: 480px;
  }
  .panel-resize {
    display: none;
  }
  .panels > :last-child {
    min-height: 360px;
    max-height: 700px;
  }
  kbd {
    display: none;
  }
  .request-bar {
    padding: 10px;
    gap: 6px;
  }
  .curl-button {
    padding: 0 8px;
  }
  .curl-button span {
    display: none;
  }
}
@media (pointer: coarse) {
  .endpoint {
    min-height: 44px;
  }
  [data-request-url] {
    font-size: 1rem;
  }
  .method-select svg {
    top: 15px;
  }
}
</style>
