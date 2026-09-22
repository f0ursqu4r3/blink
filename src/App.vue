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
  <main class="console-shell" :inert="closing || undefined">
    <header class="console-header">
      <div class="brand">
        <span class="brand-mark">
          <ScanLine :size="19" aria-hidden="true" />
        </span>
        <h1>BLINK</h1>
      </div>
      <div class="header-actions">
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
          <span class="duplicate-label">Duplicate</span>
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
    <div v-if="ready" class="console-content">
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
      <div class="workspace-content">
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
          class="close-confirmation"
          role="group"
          aria-label="Confirm close request"
        >
          <p>
            Discard
            <strong>{{ sessionLabel(closeTarget) }}</strong>
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
            >Discard tab</Button
          >
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
    <footer class="console-footer">
      <span
        class="privacy-label"
        role="status"
        title="Saved on this device, including credentials and response content. Not encrypted."
      >
        <HardDrive :size="11" aria-hidden="true" />{{ storageStatus }}
      </span>
      <span>
        {{ sessions.length }}
        {{ sessions.length === 1 ? 'REQUEST' : 'REQUESTS' }}
      </span>
      <span v-if="sending" class="sending-count" role="status">
        {{ sending }} SENDING
      </span>
      <span v-else class="limit-note">30 s TIMEOUT · 4 MiB LIMIT</span>
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

<style scoped>
.console-shell {
  height: 100dvh;
  min-height: 400px;
  display: flex;
  flex-direction: column;
  border-top: 2px solid var(--primary);
}
.console-content,
.workspace-content {
  display: flex;
  min-width: 0;
  min-height: 0;
  flex: 1;
}
.workspace-content {
  position: relative;
  flex-direction: column;
}
.console-header {
  height: 42px;
  flex-shrink: 0;
  padding: 0 14px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  border-bottom: 1px solid var(--border);
  background: var(--muted);
}
.brand {
  display: flex;
  align-items: center;
  gap: 9px;
  white-space: nowrap;
}
.brand-mark {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 24px;
  width: 26px;
  color: var(--primary-foreground);
  background: var(--primary);
}
h1 {
  font-size: 0.9375rem;
  font-weight: 800;
  letter-spacing: 0.17em;
}
.header-actions {
  display: flex;
  align-items: center;
  gap: 16px;
}

.close-confirmation {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 14px;
  background: var(--secondary);
  border-bottom: 1px solid var(--primary);
}
.close-confirmation p {
  display: flex;
  gap: 5px;
  min-width: 0;
  margin-right: auto;
  font-size: 0.75rem;
}
.close-confirmation strong {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--primary);
  font-weight: 500;
}
.console-footer {
  display: flex;
  align-items: center;
  gap: 18px;
  min-height: 26px;
  flex-shrink: 0;
  border-top: 1px solid var(--border);
  padding: 0 14px;
  font: 0.5625rem var(--font-mono);
  letter-spacing: 0.07em;
  color: var(--muted-foreground);
  background: var(--muted);
}
.privacy-label {
  display: flex;
  align-items: center;
  gap: 6px;
}
.sending-count {
  color: var(--primary);
}
@media (max-width: 760px) {
  .console-shell {
    height: auto;
    min-height: 100dvh;
  }
  .limit-note {
    display: none;
  }
  .console-footer {
    gap: 12px;
    flex-wrap: wrap;
    padding: 8px 12px;
  }
}
</style>
