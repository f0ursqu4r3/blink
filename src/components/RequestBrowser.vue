<script setup lang="ts">
import { computed, ref } from "vue";
import {
  ChevronDown,
  ChevronRight,
  Folder,
  FolderPlus,
  MoveRight,
  PanelLeftClose,
  PanelLeftOpen,
  Pencil,
  Plus,
  Trash2,
} from "lucide-vue-next";
import type { RequestGroup } from "@/lib/groups";
import { sessionLabel, type RequestSession } from "@/lib/session";

const props = defineProps<{
  sessions: RequestSession[];
  activeId: number;
  groups: RequestGroup[];
  collapsed?: boolean;
  selectedIds?: number[];
  selectionAnchorId?: number | null;
}>();
const emit = defineEmits<{
  select: [id: number];
  updateSelection: [ids: number[], anchorId: number | null];
  createGroup: [name: string, parentId: number | null, sessionIds?: number[]];
  renameGroup: [id: number, name: string];
  toggleGroup: [id: number];
  moveRequest: [sessionId: number, groupId: number | null];
  moveRequests: [
    ids: number[],
    groupId: number | null,
    beforeId: number | null,
  ];
  reorderGroup: [groupId: number, beforeGroupId: number];
  deleteGroup: [id: number];
  toggleSidebar: [];
}>();

type BrowserRow =
  | { type: "group"; group: RequestGroup; level: number }
  | { type: "request"; session: RequestSession; level: number };
const creatingParent = ref<number | null | undefined>(undefined);
const editingId = ref<number | null>(null);
const deletingId = ref<number | null>(null);
const draftName = ref("");
const groupingSelection = ref<number[] | null>(null);
const requestMime = "application/x-blink-request-ids";
const groupMime = "application/x-blink-group-id";

const groupById = computed(
  () => new Map(props.groups.map((group) => [group.id, group])),
);
const active = computed(() =>
  props.sessions.find((session) => session.id === props.activeId),
);
const rows = computed<BrowserRow[]>(() => {
  const children = new Map<number | null, RequestGroup[]>();
  for (const group of props.groups) {
    const siblings = children.get(group.parentId) ?? [];
    siblings.push(group);
    children.set(group.parentId, siblings);
  }
  const items: BrowserRow[] = props.sessions
    .filter((session) => session.groupId === null)
    .map((session) => ({ type: "request", session, level: 0 }));
  const append = (parentId: number | null, level: number) => {
    for (const group of children.get(parentId) ?? []) {
      items.push({ type: "group", group, level });
      if (group.collapsed) continue;
      for (const session of props.sessions.filter(
        (candidate) => candidate.groupId === group.id,
      ))
        items.push({ type: "request", session, level: level + 1 });
      append(group.id, level + 1);
    }
  };
  append(null, 0);
  return items;
});
const selected = computed(() => new Set(props.selectedIds ?? []));
const visibleRequestIds = computed(() =>
  rows.value.flatMap((row) => (row.type === "request" ? [row.session.id] : [])),
);

function startCreating(parentId: number | null, sessionIds?: number[]) {
  creatingParent.value = parentId;
  editingId.value = null;
  deletingId.value = null;
  draftName.value = "";
  groupingSelection.value = sessionIds?.length ? sessionIds : null;
}
function submitCreate() {
  const name = draftName.value.trim();
  if (!name || creatingParent.value === undefined) return;
  if (groupingSelection.value)
    emit("createGroup", name, creatingParent.value, groupingSelection.value);
  else emit("createGroup", name, creatingParent.value);
  groupingSelection.value = null;
  creatingParent.value = undefined;
  draftName.value = "";
}
function startRename(group: RequestGroup) {
  editingId.value = group.id;
  creatingParent.value = undefined;
  deletingId.value = null;
  draftName.value = group.name;
}
function submitRename(group: RequestGroup) {
  const name = draftName.value.trim();
  if (!name) return;
  emit("renameGroup", group.id, name);
  editingId.value = null;
  draftName.value = "";
}
function moveSelection(groupId: number | null) {
  const ids = props.selectedIds?.length
    ? props.selectedIds
    : active.value
      ? [active.value.id]
      : [];
  if (ids.length > 1) emit("moveRequests", ids, groupId, null);
  else if (ids[0] !== undefined) emit("moveRequest", ids[0], groupId);
}
function selectionAlreadyIn(groupId: number | null) {
  const ids = props.selectedIds?.length
    ? props.selectedIds
    : active.value
      ? [active.value.id]
      : [];
  return (
    ids.length > 0 &&
    ids.every(
      (id) =>
        props.sessions.find((session) => session.id === id)?.groupId ===
        groupId,
    )
  );
}
function levelClass(level: number) {
  return `level-${Math.min(level, 6)}`;
}
function parentName(parentId: number | null) {
  return parentId === null
    ? "Browser"
    : (groupById.value.get(parentId)?.name ?? "Browser");
}

function selectRequest(id: number, event: MouseEvent) {
  let ids: number[];
  let anchorId = id;
  if (
    event.shiftKey &&
    props.selectionAnchorId !== null &&
    props.selectionAnchorId !== undefined
  ) {
    const start = visibleRequestIds.value.indexOf(props.selectionAnchorId);
    const end = visibleRequestIds.value.indexOf(id);
    ids =
      start < 0 || end < 0
        ? [id]
        : visibleRequestIds.value.slice(
            Math.min(start, end),
            Math.max(start, end) + 1,
          );
    anchorId = props.selectionAnchorId;
  } else if (event.metaKey || event.ctrlKey) {
    ids = selected.value.has(id)
      ? (props.selectedIds ?? []).filter((selectedId) => selectedId !== id)
      : [...(props.selectedIds ?? []), id];
  } else {
    ids = [id];
  }
  emit("select", id);
  emit("updateSelection", ids, anchorId);
}

function requestIdsFrom(event: DragEvent) {
  try {
    const value = event.dataTransfer?.getData(requestMime);
    const ids = value ? JSON.parse(value) : [];
    return Array.isArray(ids) && ids.every(Number.isSafeInteger) ? ids : [];
  } catch {
    return [];
  }
}

function hasType(event: DragEvent, type: string) {
  return Array.from(event.dataTransfer?.types ?? []).includes(type);
}

function startRequestDrag(id: number, event: DragEvent) {
  const ids = selected.value.has(id) ? (props.selectedIds ?? []) : [id];
  if (!selected.value.has(id)) emit("updateSelection", ids, id);
  event.dataTransfer?.setData(requestMime, JSON.stringify(ids));
  event.dataTransfer?.setData("text/plain", ids.join(","));
  if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
}

function allowDrop(event: DragEvent) {
  if (hasType(event, requestMime) || hasType(event, groupMime))
    event.preventDefault();
}

function dropOnRequest(event: DragEvent, session: RequestSession) {
  const ids = requestIdsFrom(event);
  if (!ids.length) return;
  event.preventDefault();
  emit("moveRequests", ids, session.groupId, session.id);
}

function dropOnGroup(event: DragEvent, group: RequestGroup) {
  const ids = requestIdsFrom(event);
  if (ids.length) {
    event.preventDefault();
    emit("moveRequests", ids, group.id, null);
    return;
  }
  const source = Number(event.dataTransfer?.getData(groupMime));
  if (Number.isSafeInteger(source)) {
    event.preventDefault();
    emit("reorderGroup", source, group.id);
  }
}

function dropOnUngrouped(event: DragEvent) {
  const ids = requestIdsFrom(event);
  if (!ids.length) return;
  event.preventDefault();
  emit("moveRequests", ids, null, null);
}

function startGroupDrag(id: number, event: DragEvent) {
  event.dataTransfer?.setData(groupMime, String(id));
  event.dataTransfer?.setData("text/plain", String(id));
  if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
}
</script>

<template>
  <aside
    class="request-browser"
    data-request-browser
    :data-collapsed="Boolean(collapsed)"
    :aria-label="collapsed ? 'Request browser collapsed' : 'Request browser'"
  >
    <header class="browser-header">
      <div v-if="!collapsed">
        <strong>REQUESTS</strong>
      </div>
      <button
        type="button"
        class="browser-action sidebar-toggle"
        :aria-label="
          collapsed ? 'Expand request browser' : 'Collapse request browser'
        "
        :aria-expanded="!collapsed"
        :title="
          collapsed ? 'Expand request browser' : 'Collapse request browser'
        "
        @click="emit('toggleSidebar')"
      >
        <PanelLeftOpen v-if="collapsed" :size="14" aria-hidden="true" />
        <PanelLeftClose v-else :size="14" aria-hidden="true" />
      </button>
      <button
        v-if="!collapsed"
        type="button"
        class="browser-action"
        aria-label="Add top-level group"
        title="Add group"
        @click="startCreating(null)"
      >
        <Plus :size="14" aria-hidden="true" />
      </button>
      <button
        v-if="!collapsed && selectedIds?.length"
        type="button"
        class="browser-action"
        aria-label="Group selected requests"
        title="Group selected requests"
        @click="startCreating(null, selectedIds)"
      >
        <FolderPlus :size="14" aria-hidden="true" />
      </button>
    </header>

    <form
      v-if="!collapsed && creatingParent === null"
      class="browser-form top-level-form"
      @submit.prevent="submitCreate"
    >
      <label class="sr-only" for="top-level-group-name"
        >Top-level group name</label
      >
      <input
        id="top-level-group-name"
        v-model="draftName"
        maxlength="80"
        placeholder="Group name"
        aria-label="Top-level group name"
        autofocus
      />
      <button type="submit" aria-label="Create top-level group">Add</button>
      <button
        type="button"
        aria-label="Cancel group creation"
        @click="creatingParent = undefined"
      >
        Cancel
      </button>
    </form>

    <div v-if="!collapsed" class="browser-tree">
      <div
        class="browser-root-row"
        @dragover="allowDrop"
        @drop="dropOnUngrouped"
      >
        <span class="browser-root-label">UNGROUPED</span>
        <button
          type="button"
          class="browser-action move-action"
          :aria-label="`Move ${selectedIds?.length ? 'selected requests' : 'active request'} to Ungrouped`"
          :title="`Move ${selectedIds?.length ? 'selected requests' : 'active request'} here`"
          :disabled="selectionAlreadyIn(null)"
          @click="moveSelection(null)"
        >
          <MoveRight :size="13" aria-hidden="true" />
        </button>
      </div>

      <template
        v-for="row in rows"
        :key="
          row.type === 'group'
            ? `group-${row.group.id}`
            : `request-${row.session.id}`
        "
      >
        <button
          v-if="row.type === 'request'"
          type="button"
          class="browser-request"
          :class="[
            levelClass(row.level),
            {
              active: activeId === row.session.id,
              selected: selected.has(row.session.id),
            },
          ]"
          :aria-selected="selected.has(row.session.id)"
          :data-request-id="row.session.id"
          draggable="true"
          :title="sessionLabel(row.session)"
          @click="selectRequest(row.session.id, $event)"
          @dragstart="startRequestDrag(row.session.id, $event)"
          @dragover="allowDrop"
          @drop="dropOnRequest($event, row.session)"
        >
          <span :data-method="row.session.draft.method">{{
            row.session.draft.method
          }}</span>
          <strong>{{ sessionLabel(row.session) }}</strong>
        </button>

        <template v-else>
          <div
            class="browser-group-row"
            :class="levelClass(row.level)"
            :data-group-id="row.group.id"
            draggable="true"
            @dragstart="startGroupDrag(row.group.id, $event)"
            @dragover="allowDrop"
            @drop="dropOnGroup($event, row.group)"
          >
            <button
              type="button"
              class="tree-toggle"
              :aria-label="`${row.group.collapsed ? 'Expand' : 'Collapse'} ${row.group.name}`"
              @click="emit('toggleGroup', row.group.id)"
            >
              <ChevronRight
                v-if="row.group.collapsed"
                :size="13"
                aria-hidden="true"
              />
              <ChevronDown v-else :size="13" aria-hidden="true" />
            </button>
            <Folder :size="13" aria-hidden="true" />
            <span v-if="editingId !== row.group.id" class="group-name">{{
              row.group.name
            }}</span>
            <form
              v-else
              class="browser-form rename-form"
              @submit.prevent="submitRename(row.group)"
            >
              <label class="sr-only" :for="`rename-group-${row.group.id}`"
                >Rename {{ row.group.name }}</label
              >
              <input
                :id="`rename-group-${row.group.id}`"
                v-model="draftName"
                maxlength="80"
                autofocus
              />
              <button type="submit" :aria-label="`Save ${row.group.name}`">
                Save
              </button>
              <button
                type="button"
                :aria-label="`Cancel rename ${row.group.name}`"
                @click="editingId = null"
              >
                Cancel
              </button>
            </form>
            <div v-if="editingId !== row.group.id" class="group-actions">
              <button
                type="button"
                class="browser-action"
                :aria-label="`Add group inside ${row.group.name}`"
                :title="`Add group inside ${row.group.name}`"
                @click="startCreating(row.group.id)"
              >
                <FolderPlus :size="13" aria-hidden="true" />
              </button>
              <button
                type="button"
                class="browser-action move-action"
                :aria-label="`Move ${selectedIds?.length ? 'selected requests' : 'active request'} to ${row.group.name}`"
                :title="`Move ${selectedIds?.length ? 'selected requests' : 'active request'} to ${row.group.name}`"
                :disabled="selectionAlreadyIn(row.group.id)"
                @click="moveSelection(row.group.id)"
              >
                <MoveRight :size="13" aria-hidden="true" />
              </button>
              <button
                type="button"
                class="browser-action"
                :aria-label="`Rename ${row.group.name}`"
                @click="startRename(row.group)"
              >
                <Pencil :size="12" aria-hidden="true" />
              </button>
              <button
                type="button"
                class="browser-action destructive-action"
                :aria-label="`Delete ${row.group.name}`"
                @click="deletingId = row.group.id"
              >
                <Trash2 :size="12" aria-hidden="true" />
              </button>
            </div>
          </div>

          <form
            v-if="creatingParent === row.group.id"
            class="browser-form child-form"
            :class="levelClass(row.level + 1)"
            @submit.prevent="submitCreate"
          >
            <label class="sr-only" :for="`group-name-${row.group.id}`"
              >Group name in {{ row.group.name }}</label
            >
            <input
              :id="`group-name-${row.group.id}`"
              v-model="draftName"
              maxlength="80"
              :aria-label="`Group name in ${row.group.name}`"
              autofocus
            />
            <button
              type="submit"
              :aria-label="`Create group in ${row.group.name}`"
            >
              Add
            </button>
            <button
              type="button"
              aria-label="Cancel group creation"
              @click="creatingParent = undefined"
            >
              Cancel
            </button>
          </form>

          <div
            v-if="deletingId === row.group.id"
            class="delete-confirmation"
            :class="levelClass(row.level + 1)"
          >
            <p>
              Delete {{ row.group.name }}? Requests move to
              {{ parentName(row.group.parentId) }}. Child groups are promoted.
            </p>
            <button
              type="button"
              @click="
                emit('deleteGroup', row.group.id);
                deletingId = null;
              "
            >
              Delete
            </button>
            <button type="button" @click="deletingId = null">Cancel</button>
          </div>
        </template>
      </template>
    </div>
  </aside>
</template>

<style scoped>
.request-browser {
  display: flex;
  width: 244px;
  min-width: 188px;
  flex-direction: column;
  border-right: 1px solid var(--border);
  background: var(--muted);
  overflow: hidden;
}
.request-browser[data-collapsed="true"] {
  width: 42px;
  min-width: 42px;
}
.browser-header {
  display: flex;
  height: 42px;
  align-items: center;
  justify-content: space-between;
  padding: 0 10px 0 12px;
  border-bottom: 1px solid var(--border);
}
.request-browser[data-collapsed="true"] .browser-header {
  justify-content: center;
  padding: 0;
}
.browser-header > div {
  display: flex;
  align-items: baseline;
  gap: 7px;
}
.browser-root-label {
  color: var(--muted-foreground);
  font: 0.5625rem var(--font-mono);
  letter-spacing: 0.12em;
}
.browser-header strong {
  font: 700 0.6875rem var(--font-mono);
  letter-spacing: 0.08em;
}
.browser-tree {
  min-height: 0;
  flex: 1;
  overflow: auto;
  padding: 8px 0;
}
.browser-root-row,
.browser-group-row,
.browser-request {
  display: flex;
  min-width: 0;
  align-items: center;
  gap: 6px;
}
.browser-root-row {
  height: 28px;
  padding: 0 9px 0 13px;
}
.browser-root-label {
  flex: 1;
}
.browser-group-row {
  min-height: 28px;
  padding-right: 7px;
  color: var(--muted-foreground);
  cursor: grab;
  -webkit-user-drag: element;
}
.group-name {
  min-width: 0;
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--foreground);
  font: 0.6875rem var(--font-mono);
}
.tree-toggle,
.browser-action {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  flex: none;
  color: var(--muted-foreground);
}
.sidebar-toggle {
  margin-left: auto;
}
.request-browser[data-collapsed="true"] .sidebar-toggle {
  margin-left: 0;
}
.tree-toggle:hover,
.browser-action:hover:not(:disabled) {
  color: var(--primary);
  background: var(--accent);
}
.browser-action:disabled {
  opacity: 0.3;
}
.group-actions {
  display: flex;
  opacity: 0;
}
.browser-group-row:hover .group-actions,
.browser-group-row:focus-within .group-actions {
  opacity: 1;
}
.destructive-action:hover:not(:disabled) {
  color: var(--destructive);
}
.browser-request {
  width: 100%;
  min-height: 27px;
  padding-right: 9px;
  overflow: hidden;
  text-align: left;
  color: var(--muted-foreground);
  font: 0.625rem var(--font-mono);
  cursor: grab;
  -webkit-user-drag: element;
}
.browser-request:hover,
.browser-request.active {
  background: var(--accent);
  color: var(--foreground);
}
.browser-request.selected {
  box-shadow: inset 2px 0 var(--primary);
}
.browser-request span {
  width: 34px;
  flex: none;
  color: var(--primary);
  font-size: 0.5rem;
  font-weight: 700;
}
.browser-request span[data-method="GET"] {
  color: var(--success);
}
.browser-request span[data-method="DELETE"] {
  color: var(--destructive);
}
.browser-request strong {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 500;
}
.level-0 {
  padding-left: 12px;
}
.level-1 {
  padding-left: 28px;
}
.level-2 {
  padding-left: 44px;
}
.level-3 {
  padding-left: 60px;
}
.level-4 {
  padding-left: 76px;
}
.level-5 {
  padding-left: 92px;
}
.level-6 {
  padding-left: 108px;
}
.browser-form,
.delete-confirmation {
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 5px 8px;
  border-bottom: 1px solid var(--border);
  background: var(--secondary);
}
.browser-form input {
  min-width: 0;
  height: 25px;
  flex: 1;
  border: 1px solid var(--input);
  border-radius: 2px;
  padding: 0 6px;
  background: var(--background);
  color: var(--foreground);
  font: 0.625rem var(--font-mono);
}
.browser-form button,
.delete-confirmation button {
  color: var(--muted-foreground);
  font: 0.5625rem var(--font-mono);
}
.browser-form button:hover,
.delete-confirmation button:hover {
  color: var(--primary);
}
.rename-form {
  min-width: 0;
  flex: 1;
  padding: 0;
  border: 0;
  background: transparent;
}
.child-form,
.delete-confirmation {
  padding-right: 9px;
}
.delete-confirmation {
  align-items: flex-start;
  flex-wrap: wrap;
  font: 0.5625rem var(--font-mono);
  color: var(--muted-foreground);
}
.delete-confirmation p {
  width: 100%;
  line-height: 1.45;
}
.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}
@media (pointer: coarse) {
  .tree-toggle,
  .browser-action {
    width: 32px;
    height: 32px;
  }
  .group-actions {
    opacity: 1;
  }
  .browser-group-row,
  .browser-request {
    min-height: 38px;
  }
}
@media (max-width: 760px) {
  .request-browser {
    display: none;
  }
}
</style>
