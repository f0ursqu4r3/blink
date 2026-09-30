<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import GroupActionsMenu from "./GroupActionsMenu.vue";
import HelpTooltip from "./HelpTooltip.vue";
import {
  ChevronDown,
  ChevronRight,
  FilePlus,
  Import,
  Folder,
  FolderInput,
  FolderPlus,
  ListCollapse,
  Lock,
  MoveRight,
  Plus,
  Settings,
} from "lucide-vue-next";
import type { RequestGroup } from "@/lib/groups";
import {
  displayMethod,
  hasDraft,
  sessionLabel,
  type RequestSession,
} from "@/lib/session";
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
  ContextMenuRadioGroup,
  ContextMenuRadioItem,
  ContextMenuShortcut,
} from "@/components/ui/context-menu";
import GroupMenuTree from "./GroupMenuTree.vue";
import GroupMenuItems, { type GroupAction } from "./GroupMenuItems.vue";
import { shortcutLabel } from "@/lib/shortcut";
import { useDragDrop, type DropHit } from "@/composables/useDragDrop";
import {
  hitZone,
  resolveTreeDrop,
  stepRequests,
  type DragPayload,
  type Point,
  type TreeCommand,
  type TreeTarget,
} from "@/lib/drag-drop";

const props = defineProps<{
  sessions: RequestSession[];
  /** Ids of requests open as tabs. */
  openIds?: number[];
  activeId: number | null;
  groups: RequestGroup[];
  globalDefinitions?: Record<string, string>;
  /** cURL text for a request; empty when its draft does not build. */
  curlFor?: (id: number) => string;
  /** Ask before deleting a request that has content. */
  confirmDelete?: boolean;
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
  moveGroup: [
    groupId: number,
    parentId: number | null,
    beforeGroupId: number | null,
  ];
  deleteGroup: [id: number];
  collapseAllGroups: [];
  import: [];
  openGroupSettings: [groupId: number];
  createRequest: [groupId: number | null];
  duplicateRequest: [sessionId: number];
  closeRequest: [sessionId: number];
  deleteRequest: [sessionId: number];
  copy: [text: string];
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
const deletingRequestId = ref<number | null>(null);
/** The requests that the open delete confirmation removes. */
const deletingRequestIds = ref<number[]>([]);
const draftName = ref("");
const groupingSelection = ref<number[] | null>(null);

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
/** True when requests are selected and not all of them are in `groupId`. */
function hasMovableSelection(groupId: number | null) {
  return (props.selectedIds?.length ?? 0) > 0 && !selectionAlreadyIn(groupId);
}
function onGroupAction(group: RequestGroup, action: GroupAction) {
  if (action === "createRequest") emit("createRequest", group.id);
  else if (action === "createGroup") startCreating(group.id);
  else if (action === "rename") startRename(group);
  else if (action === "settings") emit("openGroupSettings", group.id);
  else if (action === "toggle") emit("toggleGroup", group.id);
  else if (action === "collapseAll") emit("collapseAllGroups");
  else if (action === "moveSelection") moveSelection(group.id);
  else deletingId.value = group.id;
}
function indent(level: number) {
  const pxMap = [12, 28, 44, 60, 76, 92, 108];
  return pxMap[Math.min(level, 6)];
}
function levelPadding(level: number): string {
  return `padding-left: ${indent(level)}px`;
}
function rowKey(row: BrowserRow) {
  return row.type === "group"
    ? `group-${row.group.id}`
    : `request-${row.session.id}`;
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

type AuthMode = "inherit" | "none" | "bearer" | "basic";
const sessionById = computed(
  () => new Map(props.sessions.map((session) => [session.id, session])),
);
/** The requests a request-row menu acts on: the selection when the target is in it. */
function menuTargets(sessionId: number) {
  const ids = props.selectedIds ?? [];
  return ids.length > 1 && selected.value.has(sessionId)
    ? [...ids]
    : [sessionId];
}
function countLabel(verb: string, ids: number[], suffix = "") {
  return ids.length > 1
    ? `${verb} ${ids.length} requests${suffix}`
    : `${verb}${suffix}`;
}
function anyBusy(ids: number[]) {
  return ids.some((id) => sessionById.value.get(id)?.busy);
}
function authModeOf(id: number): AuthMode {
  return sessionById.value.get(id)?.draft.localAuth?.type ?? "inherit";
}
/** The shared mode of the targets, or undefined when they differ. */
function authMode(ids: number[]) {
  const modes = new Set(ids.map(authModeOf));
  return modes.size === 1 ? [...modes][0] : undefined;
}
function authConfig(mode: AuthMode): AuthorizationConfig | undefined {
  if (mode === "inherit") return undefined;
  if (mode === "none") return { type: "none" };
  if (mode === "bearer") return { type: "bearer", token: "" };
  return { type: "basic", username: "", password: "" };
}
/** Set the mode on each target. A target that already has it keeps its credentials. */
function setAuthMode(ids: number[], mode: AuthMode) {
  for (const id of ids)
    if (authModeOf(id) !== mode)
      emit("setRequestLocalAuth", id, authConfig(mode));
}
function moveTargets(ids: number[], groupId: number | null) {
  if (ids.length > 1) emit("moveRequests", ids, groupId, null);
  else emit("moveRequest", ids[0], groupId);
}

function requestDelete(session: RequestSession) {
  const ids = menuTargets(session.id);
  if (anyBusy(ids)) return;
  if (ids.length > 1 || (props.confirmDelete && hasDraft(session))) {
    deletingRequestId.value = session.id;
    deletingRequestIds.value = ids;
  } else emit("deleteRequest", session.id);
}
function confirmDeleteRequests() {
  for (const id of deletingRequestIds.value) emit("deleteRequest", id);
  cancelDeleteRequests();
}
function cancelDeleteRequests() {
  deletingRequestId.value = null;
  deletingRequestIds.value = [];
}

/** Called when a context menu is opened on a request row. */
function handleRequestContextMenu(sessionId: number) {
  // If the right-clicked request is already in the multi-selection, keep it.
  if (selected.value.has(sessionId)) return;
  // Otherwise select only this request. Do not open it as a tab.
  emit("updateSelection", [sessionId], sessionId);
}

const rootEl = ref<HTMLElement>();
// A template ref on a reka `as-child` trigger binds only on the first mount,
// so find the list from the root instead.
function listEl() {
  return rootEl.value?.querySelector<HTMLElement>("[data-browser-list]");
}
const drag = useDragDrop();
drag.registerSurface({
  el: listEl,
  axis: "y",
  resolve: resolveBrowserDrop,
});

function treeTarget(key: string): TreeTarget | null {
  const [type, value] = key.split("-");
  if (type === "root")
    return { type: "root", position: value === "start" ? "start" : "end" };
  const id = Number(value);
  if (!Number.isSafeInteger(id)) return null;
  if (type === "group") return { type: "group", id };
  return type === "request" ? { type: "request", id } : null;
}

/** The keyed row under the pointer. Below the last row counts as root-end. */
function rowAt(point: Point) {
  const rows = Array.from(
    listEl()?.querySelectorAll<HTMLElement>("[data-drop-key]") ?? [],
  );
  const row = rows.find((candidate) => {
    const box = candidate.getBoundingClientRect();
    return point.y >= box.top && point.y < box.bottom;
  });
  if (row) return row;
  const last = rows[rows.length - 1];
  return last && point.y >= last.getBoundingClientRect().top ? last : null;
}

function resolveBrowserDrop(
  payload: DragPayload,
  point: Point,
): DropHit | null {
  const row = rowAt(point);
  const target = row ? treeTarget(row.dataset.dropKey ?? "") : null;
  if (!row || !target) return null;
  const zone =
    target.type === "root"
      ? "into"
      : hitZone(
          row.getBoundingClientRect(),
          point,
          target.type === "group" ? "group" : "request",
        );
  const drop = resolveTreeDrop(payload, target, zone, {
    sessions: props.sessions,
    groups: props.groups,
  });
  if (!drop) return null;
  const group =
    target.type === "group" ? groupById.value.get(target.id) : undefined;
  return {
    key: drop.key,
    zone: drop.zone,
    commit: () => commitTreeDrop(drop.command),
    ...(group?.collapsed
      ? { expand: () => emit("toggleGroup", group.id) }
      : {}),
  };
}

function commitTreeDrop(command: TreeCommand) {
  if (command.type === "moveRequests")
    emit("moveRequests", command.ids, command.groupId, command.beforeId);
  else
    emit("moveGroup", command.groupId, command.parentId, command.beforeGroupId);
}

function dragIds(id: number) {
  return selected.value.has(id) ? (props.selectedIds ?? []) : [id];
}

const label = (session: RequestSession) =>
  sessionLabel(session, {
    groups: props.groups,
    globalDefinitions: props.globalDefinitions ?? {},
  });

function pressRequest(session: RequestSession, event: PointerEvent) {
  if (deletingRequestId.value === session.id) return;
  drag.startPress(event, {
    payload: () => ({ kind: "requests", ids: dragIds(session.id) }),
    preview: () => {
      const count = dragIds(session.id).length;
      return count > 1
        ? { label: `${count} requests` }
        : { label: label(session), method: displayMethod(session) };
    },
    onStart: () => {
      if (!selected.value.has(session.id))
        emit("updateSelection", [session.id], session.id);
    },
  });
}

function pressGroup(group: RequestGroup, event: PointerEvent) {
  if (editingId.value === group.id) return;
  drag.startPress(event, {
    payload: () => ({ kind: "group", id: group.id }),
    preview: () => ({ label: group.name, folder: true }),
  });
}

/** The indicator zone for a row key, when the drag targets it. */
function dropZoneFor(key: string) {
  const hit = drag.state.hit;
  return hit?.key === key ? hit.zone : null;
}
const draggedKeys = computed(() => {
  const payload = drag.state.payload;
  if (!payload) return new Set<string>();
  return new Set(
    payload.kind === "requests"
      ? payload.ids.map((id) => `request-${id}`)
      : [`group-${payload.id}`],
  );
});
/** Rows inside the folder that the drag targets with "into". */
const intoRows = computed(() => {
  const keys = new Set<string>();
  const hit = drag.state.hit;
  if (hit?.zone !== "into" || !hit.key.startsWith("group-")) return keys;
  const start = rows.value.findIndex((row) => rowKey(row) === hit.key);
  if (start < 0) return keys;
  const level = rows.value[start].level;
  for (const row of rows.value.slice(start + 1)) {
    if (row.level <= level) break;
    keys.add(rowKey(row));
  }
  return keys;
});

function stepSelection(session: RequestSession, event: KeyboardEvent) {
  if (!event.altKey || (event.key !== "ArrowUp" && event.key !== "ArrowDown"))
    return;
  event.preventDefault();
  const ids = dragIds(session.id);
  const step = stepRequests(
    props.sessions,
    ids,
    event.key === "ArrowUp" ? -1 : 1,
  );
  if (!step) return;
  emit("moveRequests", ids, step.groupId, step.beforeId);
  void nextTick(() => {
    const row = listEl()?.querySelector<HTMLElement>(
      `[data-request-id="${session.id}"]`,
    );
    row?.focus();
    row?.scrollIntoView?.({ block: "nearest" });
  });
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
    ref="rootEl"
    id="request-browser"
    class="flex flex-col overflow-hidden rounded-lg border border-border bg-muted w-61 min-w-47 max-[760px]:absolute max-[760px]:inset-y-0 max-[760px]:left-0 max-[760px]:z-40 max-[760px]:rounded-none"
    :class="{ 'max-[760px]:hidden': !mobileOpen }"
    data-request-browser
    aria-label="Request browser"
  >
    <header
      class="flex h-9 shrink-0 items-center gap-0.5 border-b border-border pl-3 pr-1.5"
    >
      <strong class="mr-auto font-mono text-[11px] font-bold tracking-[0.08em]"
        >REQUESTS</strong
      >
      <button
        type="button"
        class="browser-action"
        data-browser-new-request
        aria-label="Add request"
        title="Add request · Cmd/Ctrl+T"
        @click="emit('createRequest', active?.groupId ?? null)"
      >
        <FilePlus :size="14" aria-hidden="true" />
      </button>
      <button
        type="button"
        class="browser-action"
        aria-label="Add top-level group"
        title="Add group"
        @click="startCreating(null)"
      >
        <FolderPlus :size="14" aria-hidden="true" />
      </button>
      <button
        v-if="selectedIds?.length"
        type="button"
        class="browser-action"
        aria-label="Group selected requests"
        title="Group selected requests"
        @click="startCreating(null, selectedIds)"
      >
        <FolderInput :size="14" aria-hidden="true" />
      </button>
      <button
        type="button"
        class="browser-action"
        aria-label="Collapse all groups"
        title="Collapse all groups"
        :disabled="!groups.length"
        @click="emit('collapseAllGroups')"
      >
        <ListCollapse :size="14" aria-hidden="true" />
      </button>
      <button
        type="button"
        class="browser-action"
        aria-label="Import requests"
        title="Import OpenAPI, Postman, or .http file…"
        data-import-requests
        @click="emit('import')"
      >
        <Import :size="14" aria-hidden="true" />
      </button>
    </header>

    <form
      v-if="creatingParent === null"
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
        class="text-muted-foreground font-mono text-[9px] hover:text-foreground"
        aria-label="Create top-level group"
      >
        Add
      </button>
      <button
        type="button"
        class="text-muted-foreground font-mono text-[9px] hover:text-foreground"
        aria-label="Cancel group creation"
        @click="creatingParent = undefined"
      >
        Cancel
      </button>
    </form>

    <ContextMenu>
      <ContextMenuTrigger as-child>
        <div class="min-h-0 flex-1 overflow-auto py-2" data-browser-list>
          <!-- UNGROUPED row -->
          <ContextMenu>
            <ContextMenuTrigger as-child>
              <div
                class="flex min-w-0 items-center gap-1.5 h-7 pl-3 pr-2.25"
                :class="{
                  'bg-accent shadow-[inset_0_0_0_1px_var(--color-primary)]':
                    dropZoneFor('root') === 'into',
                }"
                data-drop-key="root-start"
                :data-drop-target="dropZoneFor('root') ?? undefined"
              >
                <span
                  class="flex-1 text-muted-foreground font-mono text-[9px] tracking-[0.12em]"
                >
                  UNGROUPED
                </span>
                <button
                  type="button"
                  class="inline-flex items-center justify-center w-5.5 h-5.5 shrink-0 text-muted-foreground hover:text-foreground hover:bg-accent disabled:opacity-30 pointer-coarse:w-8 pointer-coarse:h-8"
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
              <ContextMenuItem @select="emit('createRequest', null)">
                New request
              </ContextMenuItem>
              <ContextMenuItem
                v-if="hasMovableSelection(null)"
                @select="moveSelection(null)"
              >
                Move selection here
              </ContextMenuItem>
            </ContextMenuContent>
          </ContextMenu>

          <template v-for="row in rows" :key="rowKey(row)">
            <!-- Request row -->
            <div
              v-if="row.type === 'request'"
              :data-request-context="row.session.id"
            >
              <ContextMenu>
                <ContextMenuTrigger as-child>
                  <button
                    type="button"
                    class="relative flex w-full min-w-0 items-center gap-1.5 min-h-6.75 pr-2.25 overflow-hidden text-left text-muted-foreground font-mono text-[10px] hover:bg-accent hover:text-foreground pointer-coarse:min-h-9.5"
                    :class="{
                      'bg-accent text-foreground': activeId === row.session.id,
                      'shadow-[inset_2px_0_0_var(--color-primary)]':
                        selected.has(row.session.id),
                      'opacity-40': draggedKeys.has(
                        `request-${row.session.id}`,
                      ),
                      'bg-accent/40': intoRows.has(`request-${row.session.id}`),
                    }"
                    :style="levelPadding(row.level)"
                    :aria-selected="selected.has(row.session.id)"
                    :data-request-id="row.session.id"
                    :data-drop-key="`request-${row.session.id}`"
                    :title="label(row.session)"
                    @click="selectRequest(row.session.id, $event)"
                    @contextmenu="handleRequestContextMenu(row.session.id)"
                    @pointerdown="pressRequest(row.session, $event)"
                    @keydown="stepSelection(row.session, $event)"
                  >
                    <span
                      v-if="
                        dropZoneFor(`request-${row.session.id}`) === 'before' ||
                        dropZoneFor(`request-${row.session.id}`) === 'after'
                      "
                      class="pointer-events-none absolute right-0 h-0.5 bg-primary"
                      :class="
                        dropZoneFor(`request-${row.session.id}`) === 'before'
                          ? 'top-0'
                          : 'bottom-0'
                      "
                      :style="{ left: `${indent(row.level)}px` }"
                      :data-drop-indicator="
                        dropZoneFor(`request-${row.session.id}`)
                      "
                    />
                    <span
                      class="method w-8.5 shrink-0 text-[8px] font-bold"
                      :data-method="displayMethod(row.session)"
                    >
                      {{ displayMethod(row.session) }}
                    </span>
                    <strong
                      class="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap font-medium"
                    >
                      {{ label(row.session) }}
                    </strong>
                    <Lock
                      v-if="effectiveSessionAuth(row.session).type !== 'none'"
                      :size="10"
                      aria-hidden="true"
                      data-auth-indicator
                      class="shrink-0 text-muted-foreground"
                    />
                  </button>
                </ContextMenuTrigger>
                <ContextMenuContent>
                  <ContextMenuItem @select="emit('select', row.session.id)">
                    Open
                  </ContextMenuItem>
                  <ContextMenuItem
                    @select="emit('duplicateRequest', row.session.id)"
                  >
                    Duplicate
                    <ContextMenuShortcut>{{
                      shortcutLabel(["mod", "shift", "d"])
                    }}</ContextMenuShortcut>
                  </ContextMenuItem>
                  <ContextMenuItem
                    v-if="openIds?.includes(row.session.id)"
                    @select="emit('closeRequest', row.session.id)"
                  >
                    Close tab
                  </ContextMenuItem>
                  <template v-if="menuTargets(row.session.id).length === 1">
                    <ContextMenuSeparator />
                    <ContextMenuItem
                      data-request-copy-url
                      :disabled="!row.session.draft.url"
                      @select="emit('copy', row.session.draft.url)"
                    >
                      Copy URL
                    </ContextMenuItem>
                    <ContextMenuItem
                      data-request-copy-curl
                      :disabled="!curlFor?.(row.session.id)"
                      @select="emit('copy', curlFor?.(row.session.id) ?? '')"
                    >
                      Copy as cURL
                    </ContextMenuItem>
                  </template>
                  <ContextMenuSeparator />
                  <ContextMenuSub>
                    <ContextMenuSubTrigger data-auth-menu>
                      Authorization
                    </ContextMenuSubTrigger>
                    <ContextMenuSubContent>
                      <ContextMenuRadioGroup
                        :model-value="authMode(menuTargets(row.session.id))"
                        @update:model-value="
                          (mode) =>
                            setAuthMode(
                              menuTargets(row.session.id),
                              mode as AuthMode,
                            )
                        "
                      >
                        <ContextMenuRadioItem
                          value="inherit"
                          data-auth-mode="inherit"
                        >
                          Inherit
                        </ContextMenuRadioItem>
                        <ContextMenuRadioItem
                          value="none"
                          data-auth-mode="none"
                        >
                          No auth
                        </ContextMenuRadioItem>
                        <ContextMenuRadioItem
                          value="bearer"
                          data-auth-mode="bearer"
                        >
                          Bearer
                        </ContextMenuRadioItem>
                        <ContextMenuRadioItem
                          value="basic"
                          data-auth-mode="basic"
                        >
                          Basic
                        </ContextMenuRadioItem>
                      </ContextMenuRadioGroup>
                    </ContextMenuSubContent>
                  </ContextMenuSub>
                  <ContextMenuSub>
                    <ContextMenuSubTrigger data-move-menu>
                      {{
                        countLabel("Move", menuTargets(row.session.id), " to")
                      }}
                    </ContextMenuSubTrigger>
                    <ContextMenuSubContent>
                      <GroupMenuTree
                        :groups="groups"
                        root-label="Ungrouped"
                        :current-id="
                          menuTargets(row.session.id).length === 1
                            ? row.session.groupId
                            : undefined
                        "
                        @pick="
                          (groupId) =>
                            moveTargets(menuTargets(row.session.id), groupId)
                        "
                      />
                    </ContextMenuSubContent>
                  </ContextMenuSub>
                  <ContextMenuSeparator />
                  <ContextMenuItem
                    data-request-delete
                    variant="destructive"
                    :disabled="anyBusy(menuTargets(row.session.id))"
                    @select="requestDelete(row.session)"
                  >
                    {{ countLabel("Delete", menuTargets(row.session.id)) }}
                  </ContextMenuItem>
                </ContextMenuContent>
              </ContextMenu>
              <div
                v-if="deletingRequestId === row.session.id"
                class="flex flex-wrap items-start gap-1.25 px-2 py-1.25 pr-2.25 border-b border-border bg-secondary font-mono text-[9px] text-muted-foreground"
                :style="levelPadding(row.level + 1)"
                role="group"
                aria-label="Confirm delete request"
              >
                <p class="w-full leading-[1.45]">
                  <template v-if="deletingRequestIds.length > 1">
                    Delete {{ deletingRequestIds.length }} requests? Their
                    drafts and responses are lost.
                  </template>
                  <template v-else>
                    Delete {{ label(row.session) }}? Its draft and response are
                    lost.
                  </template>
                </p>
                <!-- prettier-ignore -->
                <button
                  type="button"
                  class="text-muted-foreground font-mono text-[9px] hover:text-foreground"
                  data-confirm-delete-request
                  :disabled="anyBusy(deletingRequestIds)"
                  @click="confirmDeleteRequests"
                >
                  Delete
                </button>
                <button
                  type="button"
                  class="text-muted-foreground font-mono text-[9px] hover:text-foreground"
                  data-cancel-delete-request
                  @click="cancelDeleteRequests"
                >
                  Cancel
                </button>
              </div>
            </div>

            <!-- Group row -->
            <template v-else>
              <div :data-group-context="row.group.id">
                <ContextMenu>
                  <ContextMenuTrigger as-child>
                    <div
                      class="group/row relative flex min-w-0 items-center gap-1.5 min-h-7 pr-1.75 text-muted-foreground pointer-coarse:min-h-9.5"
                      :style="levelPadding(row.level)"
                      :data-group-id="row.group.id"
                      :data-drop-key="`group-${row.group.id}`"
                      :data-drop-target="
                        dropZoneFor(`group-${row.group.id}`) ?? undefined
                      "
                      :class="{
                        'opacity-40': draggedKeys.has(`group-${row.group.id}`),
                        'bg-accent/40': intoRows.has(`group-${row.group.id}`),
                        'bg-accent shadow-[inset_0_0_0_1px_var(--color-primary)]':
                          dropZoneFor(`group-${row.group.id}`) === 'into',
                      }"
                      @pointerdown="pressGroup(row.group, $event)"
                    >
                      <span
                        v-if="
                          dropZoneFor(`group-${row.group.id}`) === 'before' ||
                          dropZoneFor(`group-${row.group.id}`) === 'after'
                        "
                        class="pointer-events-none absolute right-0 h-0.5 bg-primary"
                        :class="
                          dropZoneFor(`group-${row.group.id}`) === 'before'
                            ? 'top-0'
                            : 'bottom-0'
                        "
                        :style="{ left: `${indent(row.level)}px` }"
                        :data-drop-indicator="
                          dropZoneFor(`group-${row.group.id}`)
                        "
                      />
                      <button
                        type="button"
                        class="inline-flex items-center justify-center w-5.5 h-5.5 shrink-0 text-muted-foreground hover:text-foreground hover:bg-accent pointer-coarse:w-8 pointer-coarse:h-8"
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
                          class="shrink-0 text-muted-foreground"
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
                          class="text-muted-foreground font-mono text-[9px] hover:text-foreground"
                          :aria-label="`Save ${row.group.name}`"
                        >
                          Save
                        </button>
                        <button
                          type="button"
                          class="text-muted-foreground font-mono text-[9px] hover:text-foreground"
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
                            class="inline-flex items-center justify-center w-5.5 h-5.5 shrink-0 text-muted-foreground hover:text-foreground hover:bg-accent pointer-coarse:w-8 pointer-coarse:h-8"
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
                            class="inline-flex items-center justify-center size-5.5 shrink-0 text-muted-foreground hover:text-foreground hover:bg-accent pointer-coarse:size-8"
                            :aria-label="`New request in ${row.group.name}`"
                            @click="emit('createRequest', row.group.id)"
                          >
                            <Plus :size="12" aria-hidden="true" /></button
                        ></HelpTooltip>
                        <GroupActionsMenu
                          :group="row.group"
                          :groups="groups"
                          :can-move-selection="
                            hasMovableSelection(row.group.id)
                          "
                          @action="(action) => onGroupAction(row.group, action)"
                          @move-to="
                            (parentId) =>
                              emit('moveGroup', row.group.id, parentId, null)
                          "
                        />
                      </div>
                    </div>
                  </ContextMenuTrigger>
                  <ContextMenuContent>
                    <GroupMenuItems
                      :group="row.group"
                      :groups="groups"
                      :can-move-selection="hasMovableSelection(row.group.id)"
                      @action="(action) => onGroupAction(row.group, action)"
                      @move-to="
                        (parentId) =>
                          emit('moveGroup', row.group.id, parentId, null)
                      "
                    />
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
                  class="text-muted-foreground font-mono text-[9px] hover:text-foreground"
                  :aria-label="`Create group in ${row.group.name}`"
                >
                  Add
                </button>
                <button
                  type="button"
                  class="text-muted-foreground font-mono text-[9px] hover:text-foreground"
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
                  class="text-muted-foreground font-mono text-[9px] hover:text-foreground"
                  @click="emit('deleteGroup', row.group.id); deletingId = null"
                >
                  Delete
                </button>
                <button
                  type="button"
                  class="text-muted-foreground font-mono text-[9px] hover:text-foreground"
                  @click="deletingId = null"
                >
                  Cancel
                </button>
              </div>
            </template>
          </template>
          <div data-drop-key="root-end" class="min-h-8" aria-hidden="true" />
        </div>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem @select="emit('createRequest', null)">
          New request
        </ContextMenuItem>
        <ContextMenuItem @select="startCreating(null)">
          New group
        </ContextMenuItem>
        <ContextMenuItem
          :disabled="!groups.length"
          @select="emit('collapseAllGroups')"
        >
          Collapse all
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  </aside>
</template>

<style scoped>
@reference "../style.css";
.browser-action {
  @apply inline-flex size-6 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-30 pointer-coarse:size-8;
}
</style>
