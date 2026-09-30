<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import {
  Columns2,
  HardDrive,
  PanelLeft,
  Rows2,
  Settings,
} from "lucide-vue-next";
import { Button } from "@/components/ui/button";
import RequestTabs from "@/components/RequestTabs.vue";
import RequestBrowser from "@/components/RequestBrowser.vue";
import DragPreview from "@/components/DragPreview.vue";
import RequestWorkspace from "@/components/RequestWorkspace.vue";
import GroupSettingsDialog from "@/components/GroupSettingsDialog.vue";
import ApplicationSettingsDialog from "@/components/ApplicationSettingsDialog.vue";
import WorkspaceStorageNotice from "@/components/WorkspaceStorageNotice.vue";
import HelpTooltip from "@/components/HelpTooltip.vue";
import CommandCenter from "@/components/CommandCenter.vue";
import CookiesDialog from "@/components/CookiesDialog.vue";
import { clearCookies } from "@/lib/cookies";
import { COMMAND_PREFIX, groupPath, type Command } from "@/lib/command-center";
import { formatBytes } from "@/lib/request";
import { codeTargets } from "@/lib/codegen";
import { applyZoom } from "@/lib/zoom";
import { shortcutLabel } from "@/lib/shortcut";
import { useTheme } from "@/composables/useTheme";
import { nativeTransport } from "@/lib/transport";
import { useWorkspaceState } from "@/composables/useWorkspaceState";
import { createSession } from "@/lib/session";
import { sessionCurl } from "@/lib/session-curl";
import { useClipboard } from "@/composables/useClipboard";
import type { AuthorizationConfig } from "@/lib/authorization";
import {
  applyNewRequestDefaults,
  resolveNewRequestDefaults,
  stepZoom,
  transportOptions,
  type WorkspacePreferences,
} from "@/lib/preferences";

const {
  sessions,
  groups,
  openIds,
  activeId,
  ready,
  closing,
  error: storageError,
  status: storageStatus,
  exitBlocked,
  globalDefinitions,
  preferences,
  restore,
  flush,
  reset,
  quitWithoutSaving,
  openRequest,
  reopenClosedTab,
  lastDeletion,
  undoDelete,
  discardDeletion,
  closeTab,
  closeTabs,
  openRequests,
  deleteRequest,
  addGroup,
  renameGroup,
  toggleGroup,
  moveRequest,
  moveRequests,
  moveGroup,
  deleteGroup,
  setRequestLocalAuth,
  setGroupName,
  setGroupLocalAuth,
  setGroupLocalDefinitions,
  setGlobalDefinitions,
  setPreferences,
  setGroupParent,
  setGroupNewRequestDefaults,
} = useWorkspaceState();

const active = computed(() =>
  sessions.value.find((session) => session.id === activeId.value),
);
const openSessions = computed(() =>
  openIds.value.flatMap((id) => {
    const session = sessions.value.find((candidate) => candidate.id === id);
    return session ? [session] : [];
  }),
);
const activeGroupPath = computed(() =>
  active.value ? groupPath(groups.value, active.value.groupId) : "",
);
const sidebarCollapsed = ref(false);
const commandCenter = ref<InstanceType<typeof CommandCenter>>();
const { copied, copyError, copy: copyText } = useClipboard();
/** cURL text for a request; empty when its draft does not build. */
function curlFor(id: number) {
  const session = sessions.value.find((candidate) => candidate.id === id);
  return session
    ? sessionCurl(
        session,
        groups.value,
        globalDefinitions.value,
        transport.value,
      )
    : "";
}
const { name: themeName } = useTheme();
// Tauri draws the macOS traffic lights over the title bar.
const macOverlay = nativeTransport && /Mac/.test(navigator.userAgent);
const mobileBrowserOpen = ref(false);
const narrow = ref(false);
const browserVisible = computed(() =>
  narrow.value ? mobileBrowserOpen.value : !sidebarCollapsed.value,
);
const browserToggleLabel = computed(() =>
  browserVisible.value ? "Hide request browser" : "Show request browser",
);
const selectedRequestIds = ref<number[]>([]);
const selectionAnchorId = ref<number | null>(null);
const sending = computed(
  () => sessions.value.filter((session) => session.busy).length,
);
const transport = computed(() => transportOptions(preferences.value));

// Group settings dialog state
const groupSettingsOpen = ref(false);
const groupSettingsId = ref<number | null>(null);
const applicationSettingsOpen = ref(false);
const cookiesOpen = ref(false);
function openCookies() {
  // From settings, focus later returns to what opened settings.
  if (!applicationSettingsOpen.value)
    dialogOpener.value = document.activeElement as HTMLElement | null;
  applicationSettingsOpen.value = false;
  cookiesOpen.value = true;
}
const dialogOpener = ref<HTMLElement | null>(null);
const groupSettingsGroup = computed(
  () => groups.value.find((g) => g.id === groupSettingsId.value) ?? null,
);

function openGroupSettings(groupId: number) {
  dialogOpener.value = document.activeElement as HTMLElement | null;
  groupSettingsId.value = groupId;
  groupSettingsOpen.value = true;
}
function openApplicationSettings(event?: Event) {
  dialogOpener.value =
    (event?.currentTarget as HTMLElement | null) ??
    (document.activeElement as HTMLElement | null);
  applicationSettingsOpen.value = true;
}
watch(
  [groupSettingsOpen, applicationSettingsOpen, cookiesOpen],
  ([groupOpen, appOpen, cookies], [wasGroupOpen, wasAppOpen, wasCookies]) => {
    if (
      (wasGroupOpen || wasAppOpen || wasCookies) &&
      !groupOpen &&
      !appOpen &&
      !cookies
    )
      void nextTick(() => dialogOpener.value?.focus());
  },
);

function handleSaveGroupSettings(
  groupId: number,
  changes: {
    name?: string;
    localAuth?: AuthorizationConfig | undefined;
    localDefinitions?: Record<string, string>;
    parentId?: number | null;
    defaultMethod?: import("@/lib/request").Method | undefined;
    defaultUrl?: string | undefined;
  },
) {
  if (changes.name !== undefined) setGroupName(groupId, changes.name);
  if (Object.prototype.hasOwnProperty.call(changes, "localAuth"))
    setGroupLocalAuth(groupId, changes.localAuth);
  if (changes.localDefinitions !== undefined)
    setGroupLocalDefinitions(groupId, changes.localDefinitions);
  if (changes.parentId !== undefined) setGroupParent(groupId, changes.parentId);
  if (Object.prototype.hasOwnProperty.call(changes, "defaultMethod"))
    setGroupNewRequestDefaults(
      groupId,
      changes.defaultMethod,
      changes.defaultUrl,
    );
  groupSettingsOpen.value = false;
}

const stacked = computed(() => preferences.value.paneLayout === "vertical");
const layoutToggleLabel = computed(() =>
  stacked.value
    ? "Place request and response side by side"
    : "Stack request above response",
);
function toggleLayout() {
  setPreferences({
    ...preferences.value,
    paneLayout: stacked.value ? "horizontal" : "vertical",
  });
}
type WorkspaceHandle = InstanceType<typeof RequestWorkspace>;
const workspaces = new Map<number, WorkspaceHandle>();
function setWorkspace(id: number, handle: unknown) {
  if (handle) workspaces.set(id, handle as WorkspaceHandle);
  else workspaces.delete(id);
}
const activeWorkspace = () =>
  activeId.value === null ? undefined : workspaces.get(activeId.value);
function setZoom(zoom: number) {
  setPreferences({ ...preferences.value, zoom });
}
watch(
  () => preferences.value.zoom,
  (zoom) => void applyZoom(zoom).catch(() => {}),
  { immediate: true },
);
const zoomPercent = computed(() => Math.round(preferences.value.zoom * 100));
function cycleTab(step: 1 | -1) {
  if (!openIds.value.length) return;
  const index = openIds.value.indexOf(activeId.value ?? -1);
  select(
    openIds.value[(index + step + openIds.value.length) % openIds.value.length],
  );
  void nextTick(() =>
    document.getElementById(`request-tab-${activeId.value}`)?.focus(),
  );
}
async function reopenTab() {
  const id = reopenClosedTab();
  if (id === null) return;
  updateSelection([id], id);
  await nextTick();
  document.getElementById(`request-tab-${id}`)?.focus();
}
function undoDeletion() {
  if (undoDelete() && activeId.value !== null)
    updateSelection([activeId.value], activeId.value);
}
const deletionLabel = computed(() => {
  const deletion = lastDeletion.value;
  if (!deletion) return "";
  if (deletion.kind === "group") return `DELETED GROUP ${deletion.group.name}`;
  const count = deletion.items.length;
  return `DELETED ${count} ${count === 1 ? "REQUEST" : "REQUESTS"}`;
});
function newGroup() {
  const names = new Set(groups.value.map((group) => group.name));
  let name = "New group";
  for (let n = 2; names.has(name); n++) name = `New group ${n}`;
  const group = addGroup(name, null);
  if (narrow.value) mobileBrowserOpen.value = true;
  else sidebarCollapsed.value = false;
  openGroupSettings(group.id);
}
const commands = computed<Command[]>(() => {
  const none = activeId.value === null;
  const current = active.value;
  const response = current?.response;
  return [
    {
      id: "toggle-layout",
      label: `View: ${layoutToggleLabel.value}`,
      shortcut: ["mod", "\\"],
    },
    { id: "toggle-browser", label: `View: ${browserToggleLabel.value}` },
    { id: "zoom-in", label: "View: Zoom in", shortcut: ["mod", "="] },
    { id: "zoom-out", label: "View: Zoom out", shortcut: ["mod", "-"] },
    {
      id: "zoom-reset",
      label: `View: Reset zoom (${zoomPercent.value}%)`,
      shortcut: ["mod", "0"],
      disabled: preferences.value.zoom === 1,
    },
    {
      id: "next-tab",
      label: "View: Next tab",
      shortcut: ["ctrl", "tab"],
      disabled: openIds.value.length < 2,
    },
    {
      id: "previous-tab",
      label: "View: Previous tab",
      shortcut: ["ctrl", "shift", "tab"],
      disabled: openIds.value.length < 2,
    },
    {
      id: "new-request",
      label: "Request: New request",
      shortcut: ["mod", "t"],
    },
    {
      id: "send",
      label: current?.busy ? "Request: Cancel request" : "Request: Send",
      shortcut: current?.busy ? ["mod", "."] : ["mod", "enter"],
      disabled: none,
    },
    {
      id: "focus-url",
      label: "Request: Focus URL",
      shortcut: ["mod", "l"],
      disabled: none,
    },
    { id: "show-code", label: "Request: Show code", disabled: none },
    ...codeTargets.map((target) => ({
      id: `copy-as-${target.id}`,
      label: `Request: Copy as ${target.label}`,
      disabled: none,
    })),
    {
      id: "duplicate-request",
      label: "Request: Duplicate request",
      shortcut: ["mod", "shift", "d"],
      disabled: none,
    },
    { id: "reveal", label: "Request: Reveal in Browser", disabled: none },
    {
      id: "delete-request",
      label: "Request: Delete request",
      disabled: none || Boolean(current?.busy),
    },
    {
      id: "close-tab",
      label: "Tabs: Close tab",
      shortcut: ["mod", "w"],
      disabled: none,
    },
    {
      id: "close-other-tabs",
      label: "Tabs: Close other tabs",
      disabled: openIds.value.length < 2,
    },
    {
      id: "close-all-tabs",
      label: "Tabs: Close all tabs",
      disabled: !openIds.value.length,
    },
    {
      id: "reopen-tab",
      label: "Tabs: Reopen closed tab",
      shortcut: ["mod", "shift", "t"],
    },
    {
      id: "find",
      label: "Response: Find",
      shortcut: ["mod", "f"],
      disabled: !response || response.binary,
    },
    { id: "copy-response", label: "Response: Copy", disabled: !response },
    { id: "toggle-history", label: "Response: Toggle history", disabled: none },
    {
      id: "save-response",
      label: "Response: Save body…",
      disabled: !response,
    },
    {
      id: "toggle-wrap",
      label: "Response: Toggle line wrap",
      disabled: !response || response.binary,
    },
    {
      id: "toggle-pretty",
      label: "Response: Toggle pretty",
      disabled: !response || response.binary || response.truncated,
    },
    { id: "new-group", label: "Browser: New group" },
    { id: "manage-cookies", label: "Cookies: Manage cookies" },
    {
      id: "clear-cookies",
      label: "Cookies: Clear all cookies",
      disabled: !nativeTransport,
    },
    {
      id: "collapse-groups",
      label: "Browser: Collapse all groups",
      disabled: !groups.value.length,
    },
    {
      id: "undo-delete",
      label: "Edit: Undo delete",
      shortcut: ["mod", "z"],
      disabled: !lastDeletion.value,
    },
    {
      id: "open-settings",
      label: "Preferences: Application settings",
      shortcut: ["mod", ","],
    },
  ];
});
function runCommand(id: string) {
  const workspace = activeWorkspace();
  const response = workspace?.response();
  if (id === "toggle-layout") toggleLayout();
  else if (id === "toggle-browser") toggleBrowser();
  else if (id === "zoom-in") setZoom(stepZoom(preferences.value.zoom, 1));
  else if (id === "zoom-out") setZoom(stepZoom(preferences.value.zoom, -1));
  else if (id === "zoom-reset") setZoom(1);
  else if (id === "next-tab") cycleTab(1);
  else if (id === "previous-tab") cycleTab(-1);
  else if (id === "new-request") create();
  else if (id === "send")
    active.value?.busy ? workspace?.cancel() : void workspace?.send();
  else if (id === "focus-url") void workspace?.focusUrl();
  else if (id === "show-code") workspace?.toggleCode();
  else if (id.startsWith("copy-as-")) {
    const target = codeTargets.find((t) => `copy-as-${t.id}` === id);
    if (target && workspace) void copyText(workspace.codeFor(target.id));
  } else if (id === "duplicate-request") duplicate();
  else if (id === "reveal" && activeId.value !== null)
    void reveal(activeId.value);
  else if (id === "delete-request" && activeId.value !== null)
    remove(activeId.value);
  else if (id === "close-tab" && activeId.value !== null)
    void close(activeId.value);
  else if (id === "close-other-tabs")
    void closeMany(openIds.value.filter((open) => open !== activeId.value));
  else if (id === "close-all-tabs") void closeMany([...openIds.value]);
  else if (id === "reopen-tab") void reopenTab();
  else if (id === "find") response?.find();
  else if (id === "copy-response") response?.copyResult();
  else if (id === "toggle-history") response?.toggleHistory();
  else if (id === "save-response") void response?.saveBody();
  else if (id === "toggle-wrap") response?.toggleWrap();
  else if (id === "toggle-pretty") response?.togglePretty();
  else if (id === "new-group") newGroup();
  else if (id === "manage-cookies") openCookies();
  else if (id === "clear-cookies") void clearCookies().catch(() => {});
  else if (id === "collapse-groups") collapseAllGroups();
  else if (id === "undo-delete") undoDeletion();
  else if (id === "open-settings") openApplicationSettings();
}
function toggleBrowser() {
  if (narrow.value) {
    mobileBrowserOpen.value = !mobileBrowserOpen.value;
    sidebarCollapsed.value = false;
  } else {
    sidebarCollapsed.value = !sidebarCollapsed.value;
  }
}
function select(id: number) {
  mobileBrowserOpen.value = false;
  openRequest(id);
  updateSelection([id], id);
}
function selectFromSearch(id: number) {
  select(id);
  void nextTick(() => document.getElementById(`request-tab-${id}`)?.focus());
}
function collapseAllGroups() {
  for (const group of groups.value) group.collapsed = true;
}
/** Place requests in the tab bar (drag or Alt+Arrow) and select them. */
function placeTabs(ids: number[], beforeId: number | null) {
  openRequests(ids, beforeId);
  updateSelection(ids, ids[0] ?? null);
}
function updateSelection(ids: number[], anchorId: number | null) {
  selectedRequestIds.value = ids;
  selectionAnchorId.value = anchorId;
}
function createGroup(
  name: string,
  parentId: number | null,
  sessionIds?: number[],
) {
  const group = addGroup(name, parentId);
  if (sessionIds?.length) moveRequests(sessionIds, group.id, null);
}
function create(duplicate = false, inGroupId?: number | null) {
  if (!ready.value) return;
  const groupId =
    inGroupId !== undefined ? inGroupId : (active.value?.groupId ?? null);
  const session = createSession(duplicate ? active.value?.draft : undefined);
  session.groupId = groupId;
  if (duplicate && active.value)
    session.view = { ...active.value.view, responseScroll: 0 };
  if (!duplicate)
    applyNewRequestDefaults(
      session,
      resolveNewRequestDefaults(groups.value, groupId, preferences.value),
    );
  sessions.value.push(session);
  expandAncestors(groupId);
  select(session.id);
  updateSelection([session.id], session.id);
}
/** Expand every group from `groupId` up to the root. */
function expandAncestors(groupId: number | null) {
  const byId = new Map(groups.value.map((group) => [group.id, group]));
  let cursor = groupId;
  while (cursor !== null) {
    const group = byId.get(cursor);
    if (!group) break;
    group.collapsed = false;
    cursor = group.parentId;
  }
}
function duplicate(sessionId?: number) {
  if (!ready.value) return;
  const source =
    sessionId !== undefined
      ? sessions.value.find((s) => s.id === sessionId)
      : active.value;
  if (!source) return;
  const session = createSession(source.draft);
  session.groupId = source.groupId;
  session.view = { ...source.view, responseScroll: 0 };
  sessions.value.push(session);
  expandAncestors(session.groupId);
  select(session.id);
  updateSelection([session.id], session.id);
}
async function close(id: number) {
  closeTab(id);
  await nextTick();
  document.getElementById(`request-tab-${activeId.value}`)?.focus();
}
async function closeMany(ids: number[]) {
  closeTabs(ids);
  await nextTick();
  document.getElementById(`request-tab-${activeId.value}`)?.focus();
}
/**
 * Show a request in the Browser: open the Browser, expand its groups, then
 * select, scroll to, and focus its row.
 */
async function reveal(id: number) {
  const session = sessions.value.find((candidate) => candidate.id === id);
  if (!session) return;
  if (narrow.value) mobileBrowserOpen.value = true;
  else sidebarCollapsed.value = false;
  expandAncestors(session.groupId);
  updateSelection([id], id);
  await nextTick();
  const row = document.querySelector<HTMLElement>(`[data-request-id="${id}"]`);
  row?.scrollIntoView?.({ block: "nearest" });
  row?.focus();
}
function remove(id: number) {
  deleteRequest(id);
  updateSelection(
    selectedRequestIds.value.filter((selectedId) => selectedId !== id),
    selectionAnchorId.value === id ? null : selectionAnchorId.value,
  );
}
function isEditable(target: EventTarget | null) {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable ||
      ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName))
  );
}
function onKey(event: KeyboardEvent) {
  if (
    !ready.value ||
    closing.value ||
    groupSettingsOpen.value ||
    applicationSettingsOpen.value ||
    cookiesOpen.value ||
    document.querySelector(
      '[data-surface="context-menu"], [data-surface="command-center"]',
    )
  )
    return;
  if (
    event.isComposing ||
    event.repeat ||
    event.defaultPrevented ||
    event.altKey
  )
    return;
  if (event.ctrlKey && event.key === "Tab") {
    event.preventDefault();
    cycleTab(event.shiftKey ? -1 : 1);
  } else if (event.metaKey || event.ctrlKey) {
    const key = event.key.toLowerCase();
    if (key === "t" && event.shiftKey) {
      event.preventDefault();
      void reopenTab();
    }
    if (key === "=" || key === "+") {
      event.preventDefault();
      setZoom(stepZoom(preferences.value.zoom, 1));
    }
    if (key === "-" && !event.shiftKey) {
      event.preventDefault();
      setZoom(stepZoom(preferences.value.zoom, -1));
    }
    if (key === "0" && !event.shiftKey) {
      event.preventDefault();
      setZoom(1);
    }
    // Text fields keep their own undo.
    if (
      key === "z" &&
      !event.shiftKey &&
      lastDeletion.value &&
      !isEditable(event.target)
    ) {
      event.preventDefault();
      undoDeletion();
    }
    if (key === "t" && !event.shiftKey) {
      event.preventDefault();
      create();
    }
    if (key === "w" && !event.shiftKey) {
      event.preventDefault();
      if (activeId.value !== null) void close(activeId.value);
    }
    if (key === "d" && event.shiftKey) {
      event.preventDefault();
      duplicate();
    }
    if (key === "," && !event.shiftKey) {
      event.preventDefault();
      openApplicationSettings();
    }
    if (key === "p") {
      event.preventDefault();
      void commandCenter.value?.show(event.shiftKey ? COMMAND_PREFIX : "");
    }
    if (event.key === "\\" && !event.shiftKey) {
      event.preventDefault();
      toggleLayout();
    }
  }
}
onMounted(() => window.addEventListener("keydown", onKey));
onUnmounted(() => window.removeEventListener("keydown", onKey));

let narrowQuery: MediaQueryList | undefined;
function updateNarrow(event: MediaQueryListEvent) {
  narrow.value = event.matches;
}
onMounted(() => {
  if (typeof window.matchMedia !== "function") return;
  narrowQuery = window.matchMedia("(max-width: 760px)");
  narrow.value = narrowQuery.matches;
  narrowQuery.addEventListener("change", updateNarrow);
});
onUnmounted(() => narrowQuery?.removeEventListener("change", updateNarrow));
</script>

<template>
  <main
    class="flex flex-col h-full min-h-100 bg-frame max-[760px]:h-auto max-[760px]:min-h-dvh"
    :inert="closing || undefined"
  >
    <header
      class="grid h-10 shrink-0 grid-cols-[1fr_auto_1fr] items-center"
      data-title-bar
      data-tauri-drag-region
    >
      <div
        class="flex justify-start pl-2"
        :class="{ 'pl-18': macOverlay }"
        data-tauri-drag-region
      >
        <Button
          variant="ghost"
          :aria-pressed="browserVisible"
          :aria-label="browserToggleLabel"
          :title="browserToggleLabel"
          aria-controls="request-browser"
          data-title-browser
          @click="toggleBrowser"
        >
          <PanelLeft :size="14" aria-hidden="true" />
        </Button>
      </div>
      <CommandCenter
        v-if="ready"
        ref="commandCenter"
        :sessions="sessions"
        :global-definitions="globalDefinitions"
        :groups="groups"
        :commands="commands"
        @select="selectFromSearch"
        @command="runCommand"
      />
      <div class="flex justify-end gap-0.5 pr-2" data-tauri-drag-region>
        <Button
          variant="ghost"
          :aria-pressed="stacked"
          :aria-label="layoutToggleLabel"
          :title="`${layoutToggleLabel} · ${shortcutLabel(['mod', '\\'])}`"
          data-title-layout
          @click="toggleLayout"
        >
          <Rows2 v-if="stacked" :size="14" aria-hidden="true" />
          <Columns2 v-else :size="14" aria-hidden="true" />
        </Button>
        <Button
          variant="ghost"
          aria-label="Application settings"
          title="Application settings · Cmd/Ctrl+,"
          data-title-settings
          @click="openApplicationSettings($event)"
        >
          <Settings :size="14" aria-hidden="true" />
        </Button>
      </div>
    </header>
    <WorkspaceStorageNotice
      :error="storageError"
      :ready="ready"
      :exit-blocked="exitBlocked"
      @retry="ready ? flush() : restore()"
      @reset="reset"
      @quit="quitWithoutSaving"
    />
    <div
      v-if="ready"
      class="relative flex min-w-0 min-h-0 flex-1 gap-1.5 px-1.5 max-[760px]:gap-0 max-[760px]:px-0"
    >
      <button
        v-if="mobileBrowserOpen"
        type="button"
        class="absolute inset-0 z-30 bg-black/50 min-[761px]:hidden"
        aria-label="Close request browser"
        @click="mobileBrowserOpen = false"
      />
      <RequestBrowser
        v-show="!sidebarCollapsed || mobileBrowserOpen"
        :mobile-open="mobileBrowserOpen"
        :sessions="sessions"
        :open-ids="openIds"
        :active-id="activeId"
        :groups="groups"
        :global-definitions="globalDefinitions"
        :confirm-delete="preferences.confirmCloseDrafts"
        :selected-ids="selectedRequestIds"
        :selection-anchor-id="selectionAnchorId"
        :curl-for="curlFor"
        @select="select"
        @update-selection="updateSelection"
        @create-group="createGroup"
        @rename-group="renameGroup"
        @toggle-group="toggleGroup"
        @move-request="moveRequest"
        @move-requests="moveRequests"
        @move-group="moveGroup"
        @delete-group="deleteGroup"
        @collapse-all-groups="collapseAllGroups"
        @open-group-settings="openGroupSettings"
        @create-request="(groupId) => create(false, groupId)"
        @duplicate-request="(id) => duplicate(id)"
        @close-request="(id) => close(id)"
        @delete-request="remove"
        @set-request-local-auth="(id, auth) => setRequestLocalAuth(id, auth)"
        @copy="copyText"
      />
      <div
        class="relative flex flex-col min-w-0 min-h-0 flex-1 overflow-hidden rounded-lg border border-border bg-background max-[760px]:rounded-none max-[760px]:border-x-0"
      >
        <RequestTabs
          :sessions="openSessions"
          :active-id="activeId"
          :groups="groups"
          :global-definitions="globalDefinitions"
          :curl-for="curlFor"
          @select="select"
          @create="create()"
          @close="close"
          @close-many="closeMany"
          @duplicate="(id) => duplicate(id)"
          @copy="copyText"
          @reveal="reveal"
          @open-requests="placeTabs"
        />
        <div
          v-if="activeId === null"
          class="flex flex-1 flex-col items-center justify-center gap-3 text-xs text-muted-foreground"
          data-no-open-requests
        >
          <p>No open requests. Select a request in the browser.</p>
          <Button variant="secondary" @click="create()">New request</Button>
        </div>
        <RequestWorkspace
          v-for="session in sessions"
          :key="session.id"
          :session="session"
          :active="session.id === activeId"
          :groups="groups"
          :global-definitions="globalDefinitions"
          :transport="transport"
          :layout="preferences.paneLayout"
          :ref="(handle) => setWorkspace(session.id, handle)"
          :code-target="preferences.codeTarget"
          @capture="
            (values) =>
              setGlobalDefinitions({ ...globalDefinitions, ...values })
          "
          @update:code-target="
            (codeTarget) => setPreferences({ ...preferences, codeTarget })
          "
        />
      </div>
    </div>
    <footer
      class="flex items-center gap-4.5 min-h-6 shrink-0 px-3.5 font-mono text-[0.5625rem] tracking-[0.07em] text-muted-foreground bg-frame max-[760px]:gap-3 max-[760px]:flex-wrap max-[760px]:px-3 max-[760px]:py-2"
      data-status-bar
    >
      <HelpTooltip
        text="Saved on this device, including credentials and response content. Not encrypted."
      >
        <button
          type="button"
          aria-label="Local storage information"
          class="flex items-center gap-1.5 hover:text-foreground"
        >
          <HardDrive :size="11" aria-hidden="true" /><span role="status">{{
            storageStatus
          }}</span>
        </button>
      </HelpTooltip>
      <span>
        {{ sessions.length }}
        {{ sessions.length === 1 ? "REQUEST" : "REQUESTS" }}
      </span>
      <span
        v-if="active"
        class="flex min-w-0 items-center gap-1.5 max-[760px]:hidden"
        data-active-summary
      >
        <span class="method" :data-method="active.draft.method">{{
          active.draft.method
        }}</span>
        <template v-if="active.busy">
          · {{ (active.elapsed / 1000).toFixed(1) }} s
        </template>
        <template v-else-if="active.error"
          ><span class="text-destructive">· FAILED</span></template
        >
        <template v-else-if="active.response">
          ·
          <span
            :class="
              active.response.status >= 400
                ? 'text-destructive'
                : active.response.status >= 300
                  ? 'text-warning'
                  : 'text-success'
            "
            >{{ active.response.status }}</span
          >
          · {{ active.response.durationMs }} ms ·
          {{ formatBytes(active.response.sizeBytes) }}
          <template v-if="active.stale"> · EDITED</template>
        </template>
        <span v-if="activeGroupPath" class="truncate"
          >· {{ activeGroupPath.toUpperCase() }}</span
        >
      </span>
      <span v-if="sending" class="text-primary" role="status">
        {{ sending }} SENDING
      </span>
      <span v-if="copyError" class="text-destructive" role="alert">
        {{ copyError }}
      </span>
      <span v-else-if="copied" role="status">COPIED</span>
      <span
        v-if="lastDeletion"
        class="flex items-center gap-2"
        role="status"
        data-undo-delete
      >
        <span class="max-w-60 truncate">{{ deletionLabel }}</span>
        <button
          type="button"
          class="text-foreground underline decoration-dotted underline-offset-3"
          :title="`Undo · ${shortcutLabel(['mod', 'z'])}`"
          @click="undoDeletion"
        >
          UNDO
        </button>
        <button
          type="button"
          class="hover:text-foreground"
          aria-label="Dismiss"
          @click="discardDeletion"
        >
          ×
        </button>
      </span>
      <span v-if="!transport.verifyTls" class="text-warning" role="status">
        TLS VERIFY OFF
      </span>
      <span class="ml-auto max-[760px]:hidden">
        <template v-if="transport.proxyUrl">PROXY ·</template>
        {{ transport.timeoutSeconds }} s TIMEOUT ·
        {{ transport.inspectionLimitMiB }} MiB LIMIT
      </span>
      <button
        v-if="preferences.zoom !== 1"
        type="button"
        class="hover:text-foreground"
        data-zoom
        :title="`Reset zoom · ${shortcutLabel(['mod', '0'])}`"
        @click="setZoom(1)"
      >
        {{ zoomPercent }}%
      </button>
      <span data-theme-name>{{ themeName }}</span>
    </footer>
    <GroupSettingsDialog
      :group="groupSettingsGroup"
      :preferences="preferences"
      :groups="groups"
      :sessions="sessions"
      :open="groupSettingsOpen"
      @update:open="groupSettingsOpen = $event"
      @save="handleSaveGroupSettings"
    />
    <ApplicationSettingsDialog
      :definitions="globalDefinitions"
      :preferences="preferences"
      :open="applicationSettingsOpen"
      @update:open="applicationSettingsOpen = $event"
      @manage-cookies="openCookies"
      @save="
        (definitions, next: WorkspacePreferences) => {
          setGlobalDefinitions(definitions);
          setPreferences(next);
        }
      "
    />
    <CookiesDialog
      :open="cookiesOpen"
      :enabled="preferences.storeCookies"
      @update:open="cookiesOpen = $event"
    />
    <DragPreview />
  </main>
</template>
