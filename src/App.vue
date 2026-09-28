<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { HardDrive, PanelLeft, Settings } from "lucide-vue-next";
import { Button } from "@/components/ui/button";
import RequestTabs from "@/components/RequestTabs.vue";
import RequestBrowser from "@/components/RequestBrowser.vue";
import RequestWorkspace from "@/components/RequestWorkspace.vue";
import GroupSettingsDialog from "@/components/GroupSettingsDialog.vue";
import ApplicationSettingsDialog from "@/components/ApplicationSettingsDialog.vue";
import WorkspaceStorageNotice from "@/components/WorkspaceStorageNotice.vue";
import HelpTooltip from "@/components/HelpTooltip.vue";
import ActivityBar from "@/components/ActivityBar.vue";
import CommandCenter from "@/components/CommandCenter.vue";
import { useTheme } from "@/composables/useTheme";
import { nativeTransport } from "@/lib/transport";
import { useWorkspaceState } from "@/composables/useWorkspaceState";
import { createSession, hasDraft, sessionLabel } from "@/lib/session";
import type { AuthorizationConfig } from "@/lib/authorization";
import {
  applyNewRequestDefaults,
  resolveNewRequestDefaults,
  type WorkspacePreferences,
} from "@/lib/preferences";

const {
  sessions,
  groups,
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
  addGroup,
  renameGroup,
  toggleGroup,
  moveRequest,
  moveRequests,
  reorderGroup,
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
  sessions.value.find((session) => session.id === activeId.value)!,
);
const pendingClose = ref<number | null>(null);
const sidebarCollapsed = ref(false);
const commandCenter = ref<InstanceType<typeof CommandCenter>>();
const { name: themeName } = useTheme();
// Tauri draws the macOS traffic lights over the title bar.
const macOverlay = nativeTransport && /Mac/.test(navigator.userAgent);
const mobileBrowserOpen = ref(false);
const selectedRequestIds = ref<number[]>([]);
const selectionAnchorId = ref<number | null>(null);
const closeTarget = computed(() =>
  sessions.value.find((session) => session.id === pendingClose.value),
);
const sending = computed(
  () => sessions.value.filter((session) => session.busy).length,
);

// Group settings dialog state
const groupSettingsOpen = ref(false);
const groupSettingsId = ref<number | null>(null);
const applicationSettingsOpen = ref(false);
const dialogOpener = ref<HTMLElement | null>(null);
const groupSettingsGroup = computed(
  () => groups.value.find((g) => g.id === groupSettingsId.value) ?? null,
);
watch(pendingClose, (id) => {
  if (id !== null)
    void nextTick(() =>
      document.querySelector<HTMLButtonElement>("[data-cancel-close]")?.focus(),
    );
});

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

function select(id: number) {
  mobileBrowserOpen.value = false;
  activeId.value = id;
  pendingClose.value = null;
  updateSelection([id], id);
}
function selectFromSearch(id: number) {
  select(id);
  void nextTick(() => document.getElementById(`request-tab-${id}`)?.focus());
}
function collapseAllGroups() {
  for (const group of groups.value) group.collapsed = true;
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
function cancelClose() {
  pendingClose.value = null;
  document.getElementById(`request-tab-${activeId.value}`)?.focus();
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
  if (groupId !== null) {
    const byId = new Map(groups.value.map((group) => [group.id, group]));
    let cursor: number | null = groupId;
    while (cursor !== null) {
      const group = byId.get(cursor);
      if (!group) break;
      group.collapsed = false;
      cursor = group.parentId;
    }
  }
  select(session.id);
  updateSelection([session.id], session.id);
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
  if (session.groupId !== null) {
    const byId = new Map(groups.value.map((group) => [group.id, group]));
    let cursor: number | null = session.groupId;
    while (cursor !== null) {
      const group = byId.get(cursor);
      if (!group) break;
      group.collapsed = false;
      cursor = group.parentId;
    }
  }
  select(session.id);
  updateSelection([session.id], session.id);
}
async function close(id: number, confirmed = false) {
  const index = sessions.value.findIndex((session) => session.id === id);
  const session = sessions.value[index];
  if (!session || session.busy) return;
  if (preferences.value.confirmCloseDrafts && hasDraft(session) && !confirmed) {
    pendingClose.value = id;
    return;
  }
  pendingClose.value = null;
  sessions.value.splice(index, 1);
  updateSelection(
    selectedRequestIds.value.filter((selectedId) => selectedId !== id),
    selectionAnchorId.value === id ? null : selectionAnchorId.value,
  );
  if (!sessions.value.length) create();
  else if (activeId.value === id)
    activeId.value =
      sessions.value[Math.min(index, sessions.value.length - 1)].id;
  await nextTick();
  document.getElementById(`request-tab-${activeId.value}`)?.focus();
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
  if (event.key === "Escape" && pendingClose.value !== null) {
    cancelClose();
  }
  if (event.ctrlKey && event.key === "Tab") {
    event.preventDefault();
    const index = sessions.value.findIndex(
      (session) => session.id === activeId.value,
    );
    select(
      sessions.value[
        (index + (event.shiftKey ? -1 : 1) + sessions.value.length) %
          sessions.value.length
      ].id,
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
      void close(activeId.value);
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
</script>

<template>
  <main
    class="flex flex-col h-full min-h-100 bg-frame max-[760px]:h-auto max-[760px]:min-h-dvh"
    :inert="closing || undefined"
  >
    <header
      class="relative flex h-10 shrink-0 items-center justify-center px-2"
      :class="{ 'px-21': macOverlay }"
      data-title-bar
      data-tauri-drag-region
    >
      <Button
        variant="ghost"
        class="absolute left-2 min-[761px]:hidden"
        :aria-label="
          mobileBrowserOpen ? 'Hide request browser' : 'Show request browser'
        "
        :aria-expanded="mobileBrowserOpen"
        aria-controls="request-browser"
        @click="
          mobileBrowserOpen = !mobileBrowserOpen;
          sidebarCollapsed = false;
        "
      >
        <PanelLeft :size="14" aria-hidden="true" />
      </Button>
      <Button
        variant="ghost"
        class="absolute right-2 min-[761px]:hidden"
        aria-label="Application settings"
        title="Application settings"
        data-title-settings
        @click="openApplicationSettings($event)"
      >
        <Settings :size="14" aria-hidden="true" />
      </Button>
      <CommandCenter
        v-if="ready"
        ref="commandCenter"
        :sessions="sessions"
        :groups="groups"
        @select="selectFromSearch"
      />
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
      class="relative flex min-w-0 min-h-0 flex-1 gap-1.5 pr-1.5 max-[760px]:gap-0 max-[760px]:pr-0"
    >
      <ActivityBar
        class="max-[760px]:hidden"
        :browser-open="!sidebarCollapsed"
        @toggle-browser="sidebarCollapsed = !sidebarCollapsed"
        @open-settings="openApplicationSettings($event)"
      />
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
        :active-id="activeId"
        :groups="groups"
        :selected-ids="selectedRequestIds"
        :selection-anchor-id="selectionAnchorId"
        @select="select"
        @update-selection="updateSelection"
        @create-group="createGroup"
        @rename-group="renameGroup"
        @toggle-group="toggleGroup"
        @move-request="moveRequest"
        @move-requests="moveRequests"
        @reorder-group="reorderGroup"
        @delete-group="deleteGroup"
        @collapse-all-groups="collapseAllGroups"
        @open-group-settings="openGroupSettings"
        @create-request="(groupId) => create(false, groupId)"
        @duplicate-request="(id) => duplicate(id)"
        @close-request="(id) => close(id)"
        @set-request-local-auth="(id, auth) => setRequestLocalAuth(id, auth)"
      />
      <div
        class="relative flex flex-col min-w-0 min-h-0 flex-1 overflow-hidden rounded-lg border border-border bg-background max-[760px]:rounded-none max-[760px]:border-x-0"
      >
        <RequestTabs
          :sessions="sessions"
          :active-id="activeId"
          @select="select"
          @create="create()"
          @close="close"
          @duplicate="duplicate()"
        />
        <div
          v-if="closeTarget"
          class="flex items-center gap-2 px-3.5 py-1.5 bg-secondary border-b border-primary"
          role="group"
          aria-label="Confirm close request"
        >
          <p class="flex gap-1.25 min-w-0 mr-auto text-xs">
            Discard
            <strong
              class="overflow-hidden text-ellipsis whitespace-nowrap text-foreground font-medium"
            >
              {{ sessionLabel(closeTarget) }}
            </strong>
            ?
          </p>
          <Button variant="ghost" data-cancel-close @click="cancelClose">
            Keep open
          </Button>
          <Button
            variant="secondary"
            data-confirm-close
            :disabled="closeTarget.busy"
            @click="close(closeTarget.id, true)"
          >
            Discard tab
          </Button>
        </div>
        <RequestWorkspace
          v-for="session in sessions"
          :key="session.id"
          :session="session"
          :active="session.id === activeId"
          :groups="groups"
          :global-definitions="globalDefinitions"
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
      <span class="ml-auto max-[760px]:hidden">30 s TIMEOUT · 4 MiB LIMIT</span>
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
  </main>
</template>
