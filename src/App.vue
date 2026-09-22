<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue';
import { CopyPlus, HardDrive, ScanLine, Settings } from 'lucide-vue-next';
import { Button } from '@/components/ui/button';
import RequestTabs from '@/components/RequestTabs.vue';
import RequestBrowser from '@/components/RequestBrowser.vue';
import RequestWorkspace from '@/components/RequestWorkspace.vue';
import GroupSettingsDialog from '@/components/GroupSettingsDialog.vue';
import ApplicationSettingsDialog from '@/components/ApplicationSettingsDialog.vue';
import WorkspaceStorageNotice from '@/components/WorkspaceStorageNotice.vue';
import { useWorkspaceState } from '@/composables/useWorkspaceState';
import { createSession, hasDraft, sessionLabel } from '@/lib/session';
import type { AuthorizationConfig } from '@/lib/authorization';

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
} = useWorkspaceState();

const active = computed(() =>
  sessions.value.find((session) => session.id === activeId.value)!
);
const pendingClose = ref<number | null>(null);
const sidebarCollapsed = ref(false);
const selectedRequestIds = ref<number[]>([]);
const selectionAnchorId = ref<number | null>(null);
const closeTarget = computed(() =>
  sessions.value.find((session) => session.id === pendingClose.value)
);
const sending = computed(
  () => sessions.value.filter((session) => session.busy).length
);

// Group settings dialog state
const groupSettingsOpen = ref(false);
const groupSettingsId = ref<number | null>(null);
const applicationSettingsOpen = ref(false);
const groupSettingsGroup = computed(
  () => groups.value.find((g) => g.id === groupSettingsId.value) ?? null
);

function openGroupSettings(groupId: number) {
  groupSettingsId.value = groupId;
  groupSettingsOpen.value = true;
}

function handleSaveGroupSettings(
  groupId: number,
  changes: {
    name?: string;
    localAuth?: AuthorizationConfig | undefined;
    localDefinitions?: Record<string, string>;
  }
) {
  if (changes.name !== undefined) setGroupName(groupId, changes.name);
  if (Object.prototype.hasOwnProperty.call(changes, 'localAuth'))
    setGroupLocalAuth(groupId, changes.localAuth);
  if (changes.localDefinitions !== undefined)
    setGroupLocalDefinitions(groupId, changes.localDefinitions);
  groupSettingsOpen.value = false;
}

function select(id: number) {
  activeId.value = id;
  pendingClose.value = null;
  updateSelection([id], id);
}
function updateSelection(ids: number[], anchorId: number | null) {
  selectedRequestIds.value = ids;
  selectionAnchorId.value = anchorId;
}
function createGroup(
  name: string,
  parentId: number | null,
  sessionIds?: number[]
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
  sessions.value.push(session);
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
  sessions.value.push(session);
  select(session.id);
  updateSelection([session.id], session.id);
}
async function close(id: number, confirmed = false) {
  const index = sessions.value.findIndex((session) => session.id === id);
  const session = sessions.value[index];
  if (!session || session.busy) return;
  if (hasDraft(session) && !confirmed) {
    pendingClose.value = id;
    return;
  }
  pendingClose.value = null;
  sessions.value.splice(index, 1);
  updateSelection(
    selectedRequestIds.value.filter((selectedId) => selectedId !== id),
    selectionAnchorId.value === id ? null : selectionAnchorId.value
  );
  if (!sessions.value.length) create();
  else if (activeId.value === id)
    activeId.value =
      sessions.value[Math.min(index, sessions.value.length - 1)].id;
  await nextTick();
  document.getElementById(`request-tab-${activeId.value}`)?.focus();
}
function onKey(event: KeyboardEvent) {
  if (!ready.value || closing.value) return;
  if (
    event.isComposing ||
    event.repeat ||
    event.defaultPrevented ||
    event.altKey
  )
    return;
  if (event.key === 'Escape' && pendingClose.value !== null) {
    cancelClose();
  }
  if (event.ctrlKey && event.key === 'Tab') {
    event.preventDefault();
    const index = sessions.value.findIndex(
      (session) => session.id === activeId.value
    );
    select(
      sessions.value[
        (index + (event.shiftKey ? -1 : 1) + sessions.value.length) %
          sessions.value.length
      ].id
    );
    void nextTick(() =>
      document.getElementById(`request-tab-${activeId.value}`)?.focus()
    );
  } else if (event.metaKey || event.ctrlKey) {
    const key = event.key.toLowerCase();
    if (key === 't' && !event.shiftKey) {
      event.preventDefault();
      create();
    }
    if (key === 'w' && !event.shiftKey) {
      event.preventDefault();
      void close(activeId.value);
    }
    if (key === 'd' && event.shiftKey) {
      event.preventDefault();
      duplicate();
    }
  }
}
onMounted(() => window.addEventListener('keydown', onKey));
onUnmounted(() => window.removeEventListener('keydown', onKey));
</script>

<template>
  <main
    class="flex flex-col h-dvh min-h-100 border-t-2 border-primary max-[760px]:h-auto max-[760px]:min-h-dvh"
    :inert="closing || undefined"
  >
    <header
      class="h-10.5 shrink-0 px-3.5 flex items-center justify-between gap-3 border-b border-border bg-muted"
    >
      <div class="flex items-center gap-2.25 whitespace-nowrap">
        <span
          class="flex items-center justify-center h-6 w-6.5 text-primary-foreground bg-primary"
        >
          <ScanLine :size="19" aria-hidden="true" />
        </span>
        <h1 class="text-[0.9375rem] font-extrabold tracking-[0.17em]">BLINK</h1>
      </div>
      <div class="flex items-center gap-4">
        <Button
          variant="ghost"
          aria-label="Application settings"
          title="Application settings"
          @click="applicationSettingsOpen = true"
        >
          <Settings :size="14" aria-hidden="true" />
        </Button>
        <Button
          variant="ghost"
          data-duplicate-request
          :disabled="!ready"
          aria-label="Duplicate request"
          title="Duplicate request · Cmd/Ctrl+Shift+D"
          @click="duplicate()"
        >
          <CopyPlus :size="14" aria-hidden="true" />
          <span>Duplicate</span>
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
    <div v-if="ready" class="flex min-w-0 min-h-0 flex-1">
      <RequestBrowser
        :sessions="sessions"
        :active-id="activeId"
        :groups="groups"
        :collapsed="sidebarCollapsed"
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
        @toggle-sidebar="sidebarCollapsed = !sidebarCollapsed"
        @open-group-settings="openGroupSettings"
        @create-request="create(false, null)"
        @duplicate-request="(id) => duplicate(id)"
        @close-request="(id) => close(id)"
        @set-request-local-auth="(id, auth) => setRequestLocalAuth(id, auth)"
      />
      <div class="relative flex flex-col min-w-0 min-h-0 flex-1">
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
              class="overflow-hidden text-ellipsis whitespace-nowrap text-primary font-medium"
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
      class="flex items-center gap-4.5 min-h-6.5 shrink-0 border-t border-border px-3.5 font-mono text-[0.5625rem] tracking-[0.07em] text-muted-foreground bg-muted max-[760px]:gap-3 max-[760px]:flex-wrap max-[760px]:px-3 max-[760px]:py-2"
    >
      <span
        class="flex items-center gap-1.5"
        role="status"
        title="Saved on this device, including credentials and response content. Not encrypted."
      >
        <HardDrive :size="11" aria-hidden="true" />{{ storageStatus }}
      </span>
      <span>
        {{ sessions.length }}
        {{ sessions.length === 1 ? 'REQUEST' : 'REQUESTS' }}
      </span>
      <span v-if="sending" class="text-primary" role="status">
        {{ sending }} SENDING
      </span>
      <span v-else class="max-[760px]:hidden">30 s TIMEOUT · 4 MiB LIMIT</span>
    </footer>
    <GroupSettingsDialog
      :group="groupSettingsGroup"
      :groups="groups"
      :sessions="sessions"
      :open="groupSettingsOpen"
      @update:open="groupSettingsOpen = $event"
      @save="handleSaveGroupSettings"
    />
    <ApplicationSettingsDialog
      :definitions="globalDefinitions"
      :open="applicationSettingsOpen"
      @update:open="applicationSettingsOpen = $event"
      @save="setGlobalDefinitions"
    />
  </main>
</template>
