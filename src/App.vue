<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { HardDrive, PanelLeft, Settings } from "lucide-vue-next";
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
const sidebarCollapsed = ref(false);
const commandCenter = ref<InstanceType<typeof CommandCenter>>();
const { copied, copyError, copy: copyText } = useClipboard();
/** cURL text for a request; empty when its draft does not build. */
function curlFor(id: number) {
  const session = sessions.value.find((candidate) => candidate.id === id);
  return session
    ? sessionCurl(session, groups.value, globalDefinitions.value)
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
  [groupSettingsOpen, applicationSettingsOpen],
  ([groupOpen, appOpen], [wasGroupOpen, wasAppOpen]) => {
    if ((wasGroupOpen || wasAppOpen) && !groupOpen && !appOpen)
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
function onKey(event: KeyboardEvent) {
  if (
    !ready.value ||
    closing.value ||
    groupSettingsOpen.value ||
    applicationSettingsOpen.value ||
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
    if (!openIds.value.length) return;
    const index = openIds.value.indexOf(activeId.value ?? -1);
    select(
      openIds.value[
        (index + (event.shiftKey ? -1 : 1) + openIds.value.length) %
          openIds.value.length
      ],
    );
    void nextTick(() =>
      document.getElementById(`request-tab-${activeId.value}`)?.focus(),
    );
  } else if (event.metaKey || event.ctrlKey) {
    const key = event.key.toLowerCase();
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
    if (key === "p" && !event.shiftKey) {
      event.preventDefault();
      void commandCenter.value?.show();
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
        :groups="groups"
        @select="selectFromSearch"
      />
      <div class="flex justify-end pr-2" data-tauri-drag-region>
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
        :confirm-delete="preferences.confirmCloseDrafts"
        :selected-ids="selectedRequestIds"
        :selection-anchor-id="selectionAnchorId"
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
      />
      <div
        class="relative flex flex-col min-w-0 min-h-0 flex-1 overflow-hidden rounded-lg border border-border bg-background max-[760px]:rounded-none max-[760px]:border-x-0"
      >
        <RequestTabs
          :sessions="openSessions"
          :active-id="activeId"
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
      <span v-if="sending" class="text-primary" role="status">
        {{ sending }} SENDING
      </span>
      <span v-if="copyError" class="text-destructive" role="alert">
        {{ copyError }}
      </span>
      <span v-else-if="copied" role="status">COPIED</span>
      <span class="ml-auto max-[760px]:hidden">
        {{ transport.timeoutSeconds }} s TIMEOUT ·
        {{ transport.inspectionLimitMiB }} MiB LIMIT
      </span>
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
      @save="
        (definitions, next: WorkspacePreferences) => {
          setGlobalDefinitions(definitions);
          setPreferences(next);
        }
      "
    />
    <DragPreview />
  </main>
</template>
