<script setup lang="ts">
import { computed, ref } from "vue";
import GroupActionsMenu from "./GroupActionsMenu.vue";
import HelpTooltip from "./HelpTooltip.vue";
import {
  ChevronDown,
  ChevronRight,
  Folder,
  FolderPlus,
  Lock,
  MoveRight,
  PanelLeftClose,
  PanelLeftOpen,
  Plus,
  Settings,
} from "lucide-vue-next";
import type { RequestGroup } from "@/lib/groups";
import { sessionLabel, type RequestSession } from "@/lib/session";
import {
  resolveAuthorization,
  type AuthorizationConfig,
} from "@/lib/authorization";
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubTrigger,
  ContextMenuSubContent,
} from "@/components/ui/context-menu/index";

const props = defineProps<{
  sessions: RequestSession[];
  activeId: number;
  groups: RequestGroup[];
  collapsed?: boolean;
  mobileOpen?: boolean;
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
  openGroupSettings: [groupId: number];
  createRequest: [groupId: number | null];
  duplicateRequest: [sessionId: number];
  closeRequest: [sessionId: number];
  setRequestLocalAuth: [
    sessionId: number,
    auth: AuthorizationConfig | undefined,
  ];
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
function levelPadding(level: number): string {
  const pxMap = [12, 28, 44, 60, 76, 92, 108];
  return `padding-left: ${pxMap[Math.min(level, 6)]}px`;
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

/** Called when a context menu is opened on a request row. */
function handleRequestContextMenu(sessionId: number) {
  // If the right-clicked request is already in the multi-selection, keep it.
  if (selected.value.has(sessionId)) return;
  // Otherwise select only this request.
  emit("select", sessionId);
  emit("updateSelection", [sessionId], sessionId);
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

/** Effective auth for a session (used for lock indicator). */
function effectiveSessionAuth(session: RequestSession): AuthorizationConfig {
  return resolveAuthorization(
    (session.draft as { localAuth?: AuthorizationConfig }).localAuth,
    session.groupId,
    props.groups,
  );
}

/** Effective auth for a group (the group itself, not its children). */
function effectiveGroupAuth(group: RequestGroup): AuthorizationConfig {
  return resolveAuthorization(group.localAuth, group.parentId, props.groups);
}
</script>

<template>
  <aside
    id="request-browser"
    class="flex flex-col border-r border-border bg-muted overflow-hidden w-61 min-w-47 data-[collapsed=true]:w-10.5 data-[collapsed=true]:min-w-10.5 max-[760px]:absolute max-[760px]:inset-y-0 max-[760px]:left-0 max-[760px]:z-40"
    :class="{ 'max-[760px]:hidden': !mobileOpen }"
    data-request-browser
    :data-collapsed="Boolean(collapsed)"
    :aria-label="collapsed ? 'Request browser collapsed' : 'Request browser'"
  >
    <header
      class="flex h-10.5 items-center justify-between border-b border-border px-2.5 pl-3 data-[collapsed=true]:justify-center data-[collapsed=true]:px-0"
      :data-collapsed="Boolean(collapsed)"
    >
      <div v-if="!collapsed" class="flex items-baseline gap-1.75">
        <strong class="font-mono text-[11px] font-bold tracking-[0.08em]"
          >REQUESTS</strong
        >
      </div>
      <button
        type="button"
        class="inline-flex items-center justify-center w-5.5 h-5.5 shrink-0 text-muted-foreground hover:text-primary hover:bg-accent pointer-coarse:w-8 pointer-coarse:h-8 ml-auto data-[collapsed=true]:ml-0"
        :data-collapsed="Boolean(collapsed)"
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
        class="inline-flex items-center justify-center w-5.5 h-5.5 shrink-0 text-muted-foreground hover:text-primary hover:bg-accent pointer-coarse:w-8 pointer-coarse:h-8"
        aria-label="Add top-level group"
        title="Add group"
        @click="startCreating(null)"
      >
        <Plus :size="14" aria-hidden="true" />
      </button>
      <button
        v-if="!collapsed && selectedIds?.length"
        type="button"
        class="inline-flex items-center justify-center w-5.5 h-5.5 shrink-0 text-muted-foreground hover:text-primary hover:bg-accent pointer-coarse:w-8 pointer-coarse:h-8"
        aria-label="Group selected requests"
        title="Group selected requests"
        @click="startCreating(null, selectedIds)"
      >
        <FolderPlus :size="14" aria-hidden="true" />
      </button>
    </header>

    <form
      v-if="!collapsed && creatingParent === null"
      class="top-level-form flex items-center gap-1.25 px-2 py-1.25 border-b border-border bg-secondary"
      @submit.prevent="submitCreate"
    >
      <label class="sr-only" for="top-level-group-name">
        Top-level group name
      </label>
      <input
        id="top-level-group-name"
        v-model="draftName"
        class="min-w-0 h-6.25 flex-1 border border-input rounded-sm px-1.5 bg-background text-foreground font-mono text-[10px]"
        maxlength="80"
        placeholder="Group name"
        aria-label="Top-level group name"
        autofocus
      />
      <button
        type="submit"
        class="text-muted-foreground font-mono text-[9px] hover:text-primary"
        aria-label="Create top-level group"
      >
        Add
      </button>
      <button
        type="button"
        class="text-muted-foreground font-mono text-[9px] hover:text-primary"
        aria-label="Cancel group creation"
        @click="creatingParent = undefined"
      >
        Cancel
      </button>
    </form>

    <ContextMenu>
      <ContextMenuTrigger as-child>
        <div v-if="!collapsed" class="min-h-0 flex-1 overflow-auto py-2">
          <!-- UNGROUPED row -->
          <ContextMenu>
            <ContextMenuTrigger as-child>
              <div
                class="flex min-w-0 items-center gap-1.5 h-7 pl-3 pr-2.25"
                @dragover="allowDrop"
                @drop="dropOnUngrouped"
              >
                <span
                  class="flex-1 text-muted-foreground font-mono text-[9px] tracking-[0.12em]"
                >
                  UNGROUPED
                </span>
                <button
                  type="button"
                  class="inline-flex items-center justify-center w-5.5 h-5.5 shrink-0 text-muted-foreground hover:text-primary hover:bg-accent disabled:opacity-30 pointer-coarse:w-8 pointer-coarse:h-8"
                  :aria-label="`Move ${selectedIds?.length ? 'selected requests' : 'active request'} to Ungrouped`"
                  :title="`Move ${selectedIds?.length ? 'selected requests' : 'active request'} here`"
                  :disabled="selectionAlreadyIn(null)"
                  @click="moveSelection(null)"
                >
                  <MoveRight :size="13" aria-hidden="true" />
                </button>
              </div>
            </ContextMenuTrigger>
            <ContextMenuContent>
              <ContextMenuItem @select="moveSelection(null)">
                Move selection here
              </ContextMenuItem>
              <ContextMenuItem @select="emit('createRequest', null)">
                New request in ungrouped
              </ContextMenuItem>
            </ContextMenuContent>
          </ContextMenu>

          <template
            v-for="row in rows"
            :key="
              row.type === 'group'
                ? `group-${row.group.id}`
                : `request-${row.session.id}`
            "
          >
            <!-- Request row -->
            <div
              v-if="row.type === 'request'"
              :data-request-context="row.session.id"
            >
              <ContextMenu>
                <ContextMenuTrigger as-child>
                  <button
                    type="button"
                    class="flex w-full min-w-0 items-center gap-1.5 min-h-6.75 pr-2.25 overflow-hidden text-left text-muted-foreground font-mono text-[10px] cursor-grab [-webkit-user-drag:element] hover:bg-accent hover:text-foreground pointer-coarse:min-h-9.5"
                    :class="{
                      'bg-accent text-foreground': activeId === row.session.id,
                      'shadow-[inset_2px_0_0_var(--color-primary)]':
                        selected.has(row.session.id),
                    }"
                    :style="levelPadding(row.level)"
                    :aria-selected="selected.has(row.session.id)"
                    :data-request-id="row.session.id"
                    draggable="true"
                    :title="sessionLabel(row.session)"
                    @click="selectRequest(row.session.id, $event)"
                    @contextmenu="handleRequestContextMenu(row.session.id)"
                    @dragstart="startRequestDrag(row.session.id, $event)"
                    @dragover="allowDrop"
                    @drop="dropOnRequest($event, row.session)"
                  >
                    <span
                      class="w-8.5 shrink-0 text-primary text-[8px] font-bold"
                      :class="{
                        'text-success': row.session.draft.method === 'GET',
                        'text-destructive':
                          row.session.draft.method === 'DELETE',
                      }"
                      :data-method="row.session.draft.method"
                    >
                      {{ row.session.draft.method }}
                    </span>
                    <strong
                      class="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap font-medium"
                    >
                      {{ sessionLabel(row.session) }}
                    </strong>
                    <Lock
                      v-if="effectiveSessionAuth(row.session).type !== 'none'"
                      :size="10"
                      aria-hidden="true"
                      data-auth-indicator
                      class="shrink-0 text-primary opacity-70"
                    />
                  </button>
                </ContextMenuTrigger>
                <ContextMenuContent>
                  <ContextMenuItem @select="emit('select', row.session.id)">
                    Select
                  </ContextMenuItem>
                  <ContextMenuItem
                    @select="emit('duplicateRequest', row.session.id)"
                  >
                    Duplicate
                  </ContextMenuItem>
                  <ContextMenuItem
                    @select="emit('closeRequest', row.session.id)"
                  >
                    Close
                  </ContextMenuItem>
                  <ContextMenuSeparator />
                  <ContextMenuSub>
                    <ContextMenuSubTrigger>Set auth</ContextMenuSubTrigger>
                    <ContextMenuSubContent>
                      <ContextMenuItem
                        @select="
                          emit('setRequestLocalAuth', row.session.id, undefined)
                        "
                      >
                        Inherit
                      </ContextMenuItem>
                      <ContextMenuItem
                        @select="
                          emit('setRequestLocalAuth', row.session.id, {
                            type: 'none',
                          })
                        "
                      >
                        No auth
                      </ContextMenuItem>
                      <ContextMenuItem
                        @select="
                          emit('setRequestLocalAuth', row.session.id, {
                            type: 'bearer',
                            token: '',
                          })
                        "
                      >
                        Bearer
                      </ContextMenuItem>
                      <ContextMenuItem
                        @select="
                          emit('setRequestLocalAuth', row.session.id, {
                            type: 'basic',
                            username: '',
                            password: '',
                          })
                        "
                        >Basic</ContextMenuItem
                      >
                    </ContextMenuSubContent>
                  </ContextMenuSub>
                  <ContextMenuSeparator />
                  <ContextMenuSub>
                    <ContextMenuSubTrigger>Move to group</ContextMenuSubTrigger>
                    <ContextMenuSubContent>
                      <ContextMenuItem
                        v-for="group in groups"
                        :key="group.id"
                        @select="emit('moveRequest', row.session.id, group.id)"
                      >
                        {{ group.name }}
                      </ContextMenuItem>
                      <ContextMenuItem
                        @select="emit('moveRequest', row.session.id, null)"
                      >
                        Ungrouped
                      </ContextMenuItem>
                    </ContextMenuSubContent>
                  </ContextMenuSub>
                </ContextMenuContent>
              </ContextMenu>
            </div>

            <!-- Group row -->
            <template v-else>
              <div :data-group-context="row.group.id">
                <ContextMenu>
                  <ContextMenuTrigger as-child>
                    <div
                      class="group/row flex min-w-0 items-center gap-1.5 min-h-7 pr-1.75 text-muted-foreground cursor-grab [-webkit-user-drag:element] pointer-coarse:min-h-9.5"
                      :style="levelPadding(row.level)"
                      :data-group-id="row.group.id"
                      draggable="true"
                      @dragstart="startGroupDrag(row.group.id, $event)"
                      @dragover="allowDrop"
                      @drop="dropOnGroup($event, row.group)"
                    >
                      <button
                        type="button"
                        class="inline-flex items-center justify-center w-5.5 h-5.5 shrink-0 text-muted-foreground hover:text-primary hover:bg-accent pointer-coarse:w-8 pointer-coarse:h-8"
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
                      <Folder :size="13" class="shrink-0" aria-hidden="true" />
                      <span
                        v-if="editingId !== row.group.id"
                        class="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-foreground font-mono text-[11px] flex items-center gap-1"
                      >
                        <span
                          class="min-w-0 truncate"
                          :title="row.group.name"
                          >{{ row.group.name }}</span
                        >
                        <Lock
                          v-if="effectiveGroupAuth(row.group).type !== 'none'"
                          :size="10"
                          aria-hidden="true"
                          data-auth-indicator
                          class="shrink-0 text-primary opacity-70"
                        />
                      </span>
                      <form
                        v-else
                        class="flex min-w-0 flex-1 items-center gap-1.25"
                        @submit.prevent="submitRename(row.group)"
                      >
                        <label
                          class="sr-only"
                          :for="`rename-group-${row.group.id}`"
                        >
                          Rename {{ row.group.name }}
                        </label>
                        <input
                          :id="`rename-group-${row.group.id}`"
                          v-model="draftName"
                          class="min-w-0 h-6.25 flex-1 border border-input rounded-sm px-1.5 bg-background text-foreground font-mono text-[10px]"
                          maxlength="80"
                          autofocus
                        />
                        <button
                          type="submit"
                          class="text-muted-foreground font-mono text-[9px] hover:text-primary"
                          :aria-label="`Save ${row.group.name}`"
                        >
                          Save
                        </button>
                        <button
                          type="button"
                          class="text-muted-foreground font-mono text-[9px] hover:text-primary"
                          :aria-label="`Cancel rename ${row.group.name}`"
                          @click="editingId = null"
                        >
                          Cancel
                        </button>
                      </form>
                      <div
                        v-if="editingId !== row.group.id"
                        class="flex opacity-0 group-hover/row:opacity-100 group-focus-within/row:opacity-100 pointer-coarse:opacity-100"
                      >
                        <HelpTooltip
                          :text="`Group settings for ${row.group.name}`"
                          :help-only="false"
                          ><button
                            type="button"
                            class="inline-flex items-center justify-center w-5.5 h-5.5 shrink-0 text-muted-foreground hover:text-primary hover:bg-accent pointer-coarse:w-8 pointer-coarse:h-8"
                            :aria-label="`Group settings for ${row.group.name}`"
                            @click="emit('openGroupSettings', row.group.id)"
                          >
                            <Settings :size="12" aria-hidden="true" /></button
                        ></HelpTooltip>
                        <HelpTooltip
                          :text="`New request in ${row.group.name}`"
                          :help-only="false"
                          ><button
                            type="button"
                            class="inline-flex items-center justify-center size-5.5 shrink-0 text-muted-foreground hover:text-primary hover:bg-accent pointer-coarse:size-8"
                            :aria-label="`New request in ${row.group.name}`"
                            @click="emit('createRequest', row.group.id)"
                          >
                            <Plus :size="12" aria-hidden="true" /></button
                        ></HelpTooltip>
                        <GroupActionsMenu
                          :name="row.group.name"
                          :can-move="!selectionAlreadyIn(row.group.id)"
                          @action="
                            (action) => {
                              if (action === 'createGroup')
                                startCreating(row.group.id);
                              else if (action === 'move')
                                moveSelection(row.group.id);
                              else if (action === 'rename')
                                startRename(row.group);
                              else deletingId = row.group.id;
                            }
                          "
                        />
                      </div>
                    </div>
                  </ContextMenuTrigger>
                  <ContextMenuContent>
                    <ContextMenuItem
                      @select="emit('openGroupSettings', row.group.id)"
                    >
                      Settings
                    </ContextMenuItem>
                    <ContextMenuItem @select="startCreating(row.group.id)">
                      New child group
                    </ContextMenuItem>
                    <ContextMenuItem
                      @select="emit('createRequest', row.group.id)"
                    >
                      New request in {{ row.group.name }}
                    </ContextMenuItem>
                    <ContextMenuItem @select="startRename(row.group)">
                      Rename
                    </ContextMenuItem>
                    <ContextMenuItem
                      @select="emit('toggleGroup', row.group.id)"
                    >
                      {{ row.group.collapsed ? "Expand" : "Collapse" }}
                    </ContextMenuItem>
                    <ContextMenuItem @select="moveSelection(row.group.id)">
                      Move selection here
                    </ContextMenuItem>
                    <ContextMenuSeparator />
                    <ContextMenuItem @select="deletingId = row.group.id">
                      Delete
                    </ContextMenuItem>
                  </ContextMenuContent>
                </ContextMenu>
              </div>

              <form
                v-if="creatingParent === row.group.id"
                class="child-form flex items-center gap-1.25 px-2 py-1.25 border-b border-border bg-secondary pr-2.25"
                :style="levelPadding(row.level + 1)"
                @submit.prevent="submitCreate"
              >
                <label class="sr-only" :for="`group-name-${row.group.id}`">
                  Group name in {{ row.group.name }}
                </label>
                <input
                  :id="`group-name-${row.group.id}`"
                  v-model="draftName"
                  class="min-w-0 h-6.25 flex-1 border border-input rounded-sm px-1.5 bg-background text-foreground font-mono text-[10px]"
                  maxlength="80"
                  :aria-label="`Group name in ${row.group.name}`"
                  autofocus
                />
                <button
                  type="submit"
                  class="text-muted-foreground font-mono text-[9px] hover:text-primary"
                  :aria-label="`Create group in ${row.group.name}`"
                >
                  Add
                </button>
                <button
                  type="button"
                  class="text-muted-foreground font-mono text-[9px] hover:text-primary"
                  aria-label="Cancel group creation"
                  @click="creatingParent = undefined"
                >
                  Cancel
                </button>
              </form>

              <div
                v-if="deletingId === row.group.id"
                class="flex flex-wrap items-start gap-1.25 px-2 py-1.25 pr-2.25 border-b border-border bg-secondary font-mono text-[9px] text-muted-foreground"
                :style="levelPadding(row.level + 1)"
              >
                <p class="w-full leading-[1.45]">
                  Delete {{ row.group.name }}? Requests move to
                  {{ parentName(row.group.parentId) }}. Child groups are
                  promoted.
                </p>
                <!-- prettier-ignore -->
                <button
                  type="button"
                  class="text-muted-foreground font-mono text-[9px] hover:text-primary"
                  @click="emit('deleteGroup', row.group.id); deletingId = null"
                >
                  Delete
                </button>
                <button
                  type="button"
                  class="text-muted-foreground font-mono text-[9px] hover:text-primary"
                  @click="deletingId = null"
                >
                  Cancel
                </button>
              </div>
            </template>
          </template>
        </div>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem @select="startCreating(null)">
          New group
        </ContextMenuItem>
        <ContextMenuItem @select="emit('createRequest', null)">
          New request
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  </aside>
</template>
