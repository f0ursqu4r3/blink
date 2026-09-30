import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { createSession, hasDraft, type RequestSession } from "@/lib/session";
import { releaseResponse } from "@/lib/response-body";
import { isMethod } from "@/lib/request";
import {
  createGroup,
  deleteGroupAndPromoteContents,
  groupsAfterMove,
  sessionsAfterMove,
  type RequestGroup,
} from "@/lib/groups";
import { nativeTransport } from "@/lib/transport";
import {
  decodeWorkspace,
  encodeWorkspace,
  validateWorkspace,
  WORKSPACE_KEY,
} from "@/lib/workspace";
import {
  readBrowserWorkspace,
  readWorkspace,
  writeWorkspace,
  WorkspaceWriter,
} from "@/lib/workspace-storage";
import {
  defaultPreferences,
  validPreferences,
  type WorkspacePreferences,
} from "@/lib/preferences";

/** How long a deletion can be undone. */
export const UNDO_WINDOW_MS = 10_000;
export type Deletion =
  | {
      kind: "requests";
      items: {
        session: RequestSession;
        index: number;
        /** -1 when the request was not open as a tab. */
        openIndex: number;
        /** True when deleting it emptied the workspace. */
        last: boolean;
      }[];
    }
  | {
      kind: "group";
      group: RequestGroup;
      index: number;
      /** Groups and requests promoted to the parent of the deleted group. */
      childGroupIds: number[];
      sessionIds: number[];
    };

export function useWorkspaceState() {
  const sessions = ref<RequestSession[]>([createSession()]);
  const groups = ref<RequestGroup[]>([]);
  /** Ids of requests open as tabs, in tab order. The browser tree owns the requests. */
  const openIds = ref<number[]>([sessions.value[0].id]);
  /** Null when no tab is open. */
  const activeId = ref<number | null>(sessions.value[0].id);
  const ready = ref(false);
  const error = ref("");
  const saving = ref(false);
  const exitBlocked = ref(false);
  /** Workspace-global token definitions, shared across all requests. */
  const globalDefinitions = ref<Record<string, string>>({});
  const preferences = ref<WorkspacePreferences>(defaultPreferences());
  let revision = 0;
  let unlisten: UnlistenFn | undefined;
  const closing = ref(false);
  const writer = new WorkspaceWriter(writeWorkspace);
  const status = computed(() =>
    error.value
      ? "NOT SAVED"
      : !ready.value
        ? "RESTORING"
        : saving.value
          ? "SAVING"
          : "SAVED LOCALLY",
  );
  function accept(content: string | null) {
    if (content !== null) {
      const restored = decodeWorkspace(content);
      sessions.value = restored.sessions;
      groups.value = restored.groups;
      openIds.value = restored.openIds;
      activeId.value = restored.activeId;
      globalDefinitions.value = restored.globalDefinitions ?? {};
      preferences.value = restored.preferences ?? defaultPreferences();
    }
    error.value = "";
    ready.value = true;
  }
  const message = (cause: unknown) =>
    typeof cause === "string"
      ? cause
      : cause instanceof Error
        ? cause.message
        : "Workspace storage is unavailable.";
  if (!nativeTransport) {
    try {
      accept(readBrowserWorkspace());
    } catch (cause) {
      error.value = message(cause);
    }
  }
  async function restore() {
    ready.value = false;
    try {
      accept(await readWorkspace());
    } catch (cause) {
      error.value = message(cause);
    }
  }
  async function flush() {
    if (!ready.value) return false;
    const current = ++revision;
    saving.value = true;
    try {
      await writer.save(
        encodeWorkspace(
          sessions.value,
          activeId.value,
          groups.value,
          globalDefinitions.value,
          preferences.value,
          openIds.value,
        ),
      );
      if (current === revision) error.value = "";
      return true;
    } catch (cause) {
      if (current === revision) error.value = message(cause);
      return false;
    } finally {
      if (current === revision) saving.value = false;
    }
  }
  // Do not observe elapsed time. A running request must not cause timer-driven disk writes.
  // Do not observe the response scroll offset either: it changes on every
  // scroll event. The next real change or quit saves the latest offset.
  watch(
    () =>
      ready.value
        ? encodeWorkspace(
            sessions.value.map((session) => ({
              ...session,
              view: { ...session.view, responseScroll: 0 },
            })),
            activeId.value,
            groups.value,
            globalDefinitions.value,
            preferences.value,
            openIds.value,
          )
        : null,
    () => {
      if (ready.value && sessions.value.length) void flush();
    },
  );
  async function reset() {
    const session = createSession();
    try {
      await writer.save(
        encodeWorkspace([session], session.id, [], {}, defaultPreferences()),
      );
      sessions.value = [session];
      openIds.value = [session.id];
      groups.value = [];
      globalDefinitions.value = {};
      preferences.value = defaultPreferences();
      activeId.value = session.id;
      error.value = "";
      ready.value = true;
    } catch (cause) {
      error.value = message(cause);
    }
  }
  async function quitWithoutSaving() {
    await invoke("finish_app_exit");
  }
  async function beforeExit() {
    if (closing.value) return;
    closing.value = true;
    await nextTick();
    // A failed restore never overwrites the existing file, including when quitting.
    if (!ready.value) {
      await quitWithoutSaving();
      return;
    }
    let snapshot: string;
    do {
      snapshot = encodeWorkspace(
        sessions.value,
        activeId.value,
        groups.value,
        globalDefinitions.value,
        preferences.value,
        openIds.value,
      );
      if (!(await flush())) {
        exitBlocked.value = true;
        closing.value = false;
        return;
      }
      await nextTick();
    } while (
      snapshot !==
      encodeWorkspace(
        sessions.value,
        activeId.value,
        groups.value,
        globalDefinitions.value,
        preferences.value,
        openIds.value,
      )
    );
    await quitWithoutSaving();
  }
  function beforeUnload(event: BeforeUnloadEvent) {
    if (ready.value) {
      try {
        const content = encodeWorkspace(
          sessions.value,
          activeId.value,
          groups.value,
          globalDefinitions.value,
          preferences.value,
          openIds.value,
        );
        validateWorkspace(content);
        localStorage.setItem(WORKSPACE_KEY, content);
        error.value = "";
        saving.value = false;
      } catch (cause) {
        error.value = message(cause);
      }
    }
    if (error.value || saving.value) {
      event.preventDefault();
      event.returnValue = "";
    }
  }
  onMounted(async () => {
    if (nativeTransport) {
      try {
        unlisten = await listen("workspace:before-exit", () => {
          void beforeExit();
        });
        await invoke("app_state_ready", { ready: true });
        await restore();
      } catch (cause) {
        error.value = message(cause);
      }
    } else window.addEventListener("beforeunload", beforeUnload);
  });
  onUnmounted(() => {
    unlisten?.();
    if (nativeTransport)
      void invoke("app_state_ready", { ready: false }).catch(() => {});
    window.removeEventListener("beforeunload", beforeUnload);
  });
  /** Open a request as a tab, if not open already, and make it active. */
  function openRequest(id: number) {
    if (!sessions.value.some((session) => session.id === id)) return;
    if (!openIds.value.includes(id)) openIds.value.push(id);
    activeId.value = id;
  }
  /** Recently closed tabs, most recent last. Not saved. */
  const closedIds: number[] = [];
  function rememberClosed(ids: number[]) {
    closedIds.push(...ids);
    if (closedIds.length > 50) closedIds.splice(0, closedIds.length - 50);
  }
  /** Open the most recently closed tab that still exists. */
  function reopenClosedTab() {
    while (closedIds.length) {
      const id = closedIds.pop()!;
      if (
        sessions.value.some((session) => session.id === id) &&
        !openIds.value.includes(id)
      ) {
        openRequest(id);
        return id;
      }
    }
    return null;
  }
  /** Close a tab. The request stays in the browser tree. */
  function closeTab(id: number) {
    const index = openIds.value.indexOf(id);
    if (index < 0) return;
    rememberClosed([id]);
    openIds.value.splice(index, 1);
    if (activeId.value === id)
      activeId.value =
        openIds.value[Math.min(index, openIds.value.length - 1)] ?? null;
  }
  /**
   * Close several tabs. The active tab stays active when it stays open;
   * otherwise the next open tab at its position becomes active.
   */
  function closeTabs(ids: number[]) {
    const closing = new Set(ids);
    const before = openIds.value;
    const remaining = before.filter((id) => !closing.has(id));
    if (remaining.length === before.length) return;
    rememberClosed(before.filter((id) => closing.has(id)));
    openIds.value = remaining;
    if (activeId.value === null || !closing.has(activeId.value)) return;
    const position = before
      .slice(0, before.indexOf(activeId.value))
      .filter((id) => !closing.has(id)).length;
    activeId.value =
      remaining[Math.min(position, remaining.length - 1)] ?? null;
  }
  /**
   * Open requests as tabs before `beforeId`, or at the end. Open requests
   * move there. The first request becomes active.
   */
  function openRequests(ids: number[], beforeId: number | null) {
    const known = [...new Set(ids)].filter((id) =>
      sessions.value.some((session) => session.id === id),
    );
    if (!known.length) return;
    const moving = new Set(known);
    const remaining = openIds.value.filter((id) => !moving.has(id));
    const index = beforeId === null ? -1 : remaining.indexOf(beforeId);
    remaining.splice(index < 0 ? remaining.length : index, 0, ...known);
    openIds.value = remaining;
    activeId.value = known[0];
  }
  /**
   * The last deletion, kept so it can be undone. Deletions in the same task
   * (such as a multi-select delete) are one deletion.
   */
  const lastDeletion = ref<Deletion | null>(null);
  let batchOpen = false;
  let expiry: ReturnType<typeof setTimeout> | undefined;
  function discardDeletion() {
    clearTimeout(expiry);
    const deletion = lastDeletion.value;
    lastDeletion.value = null;
    if (deletion?.kind === "requests")
      deletion.items.forEach((item) => releaseResponse(item.session.response));
  }
  function recordDeletion(next: Deletion) {
    const current = lastDeletion.value;
    if (batchOpen && current?.kind === "requests" && next.kind === "requests")
      current.items.push(...next.items);
    else {
      discardDeletion();
      lastDeletion.value = next;
    }
    if (!batchOpen) {
      batchOpen = true;
      setTimeout(() => (batchOpen = false));
    }
    clearTimeout(expiry);
    expiry = setTimeout(discardDeletion, UNDO_WINDOW_MS);
  }
  /** Restore the last deleted requests or group. */
  function undoDelete() {
    const deletion = lastDeletion.value;
    if (!deletion) return false;
    clearTimeout(expiry);
    lastDeletion.value = null;
    if (deletion.kind === "requests") {
      // Each index is from the moment of its deletion, so undo in reverse.
      const items = [...deletion.items].reverse();
      // A request created because the workspace became empty goes away again.
      const placeholder =
        sessions.value.length === 1 && !hasDraft(sessions.value[0])
          ? sessions.value[0]
          : null;
      if (placeholder && deletion.items.some((item) => item.last)) {
        sessions.value = [];
        openIds.value = openIds.value.filter((id) => id !== placeholder.id);
      }
      const groupIds = new Set(groups.value.map((group) => group.id));
      for (const item of items) {
        if (
          item.session.groupId !== null &&
          !groupIds.has(item.session.groupId)
        )
          item.session.groupId = null;
        sessions.value.splice(
          Math.min(item.index, sessions.value.length),
          0,
          item.session,
        );
      }
      for (const item of items)
        if (item.openIndex >= 0)
          openIds.value.splice(
            Math.min(item.openIndex, openIds.value.length),
            0,
            item.session.id,
          );
      const first =
        deletion.items.find((item) => item.openIndex >= 0) ?? deletion.items[0];
      if (first) openRequest(first.session.id);
    } else {
      const { group, index, childGroupIds, sessionIds } = deletion;
      const next = [...groups.value];
      if (group.parentId !== null && !next.some((g) => g.id === group.parentId))
        group.parentId = null;
      next.splice(Math.min(index, next.length), 0, group);
      groups.value = next.map((candidate) =>
        childGroupIds.includes(candidate.id) &&
        candidate.parentId === group.parentId
          ? { ...candidate, parentId: group.id }
          : candidate,
      );
      for (const session of sessions.value)
        if (
          sessionIds.includes(session.id) &&
          session.groupId === group.parentId
        )
          session.groupId = group.id;
    }
    return true;
  }
  /** Remove a request from the workspace. The workspace always keeps one request. */
  function deleteRequest(id: number) {
    const index = sessions.value.findIndex((session) => session.id === id);
    if (index < 0 || sessions.value[index].busy) return;
    const openIndex = openIds.value.indexOf(id);
    const session = sessions.value[index];
    closeTab(id);
    sessions.value.splice(index, 1);
    recordDeletion({
      kind: "requests",
      items: [{ session, index, openIndex, last: !sessions.value.length }],
    });
    if (!sessions.value.length) {
      const session = createSession();
      sessions.value.push(session);
      openRequest(session.id);
    }
  }
  function addGroup(name: string, parentId: number | null) {
    const group = createGroup(name, parentId);
    groups.value.push(group);
    return group;
  }
  function renameGroup(id: number, name: string) {
    setGroupName(id, name);
  }
  function toggleGroup(id: number) {
    const group = groups.value.find((candidate) => candidate.id === id);
    if (group) group.collapsed = !group.collapsed;
  }
  function moveRequest(sessionId: number, groupId: number | null) {
    const session = sessions.value.find(
      (candidate) => candidate.id === sessionId,
    );
    if (!session) return;
    if (groupId !== null && !groups.value.some((group) => group.id === groupId))
      return;
    session.groupId = groupId;
  }
  function moveRequests(
    sessionIds: number[],
    groupId: number | null,
    beforeSessionId: number | null,
  ) {
    if (groupId !== null && !groups.value.some((group) => group.id === groupId))
      return;
    const ids = new Set(sessionIds);
    const moving = sessions.value.filter((session) => ids.has(session.id));
    if (!moving.length) return;
    sessions.value = sessionsAfterMove(
      sessions.value,
      sessionIds,
      groupId,
      beforeSessionId,
    );
    moving.forEach((session) => {
      session.groupId = groupId;
    });
  }
  /** Reorder, nest, or un-nest a group. Rejects cycles and unknown ids. */
  function moveGroup(
    groupId: number,
    parentId: number | null,
    beforeGroupId: number | null,
  ) {
    const group = groups.value.find((candidate) => candidate.id === groupId);
    const next = groupsAfterMove(
      groups.value,
      groupId,
      parentId,
      beforeGroupId,
    );
    if (!group || !next) return;
    group.parentId = parentId;
    groups.value = next;
  }
  function deleteGroup(id: number) {
    const index = groups.value.findIndex((group) => group.id === id);
    if (index < 0) return;
    recordDeletion({
      kind: "group",
      group: { ...groups.value[index] },
      index,
      childGroupIds: groups.value
        .filter((group) => group.parentId === id)
        .map((group) => group.id),
      sessionIds: sessions.value
        .filter((session) => session.groupId === id)
        .map((session) => session.id),
    });
    const result = deleteGroupAndPromoteContents(
      groups.value,
      sessions.value,
      id,
    );
    groups.value = result.groups;
    sessions.value = result.sessions;
  }

  // ── Narrow state setters ───────────────────────────────────────────────────

  /**
   * Set the local auth config on a request session draft.
   * Pass `undefined` to revert to "inherit from group".
   */
  function setRequestLocalAuth(
    sessionId: number,
    localAuth: import("@/lib/authorization").AuthorizationConfig | undefined,
  ) {
    const session = sessions.value.find((s) => s.id === sessionId);
    if (!session) return;
    session.draft.localAuth = localAuth;
  }

  /**
   * Set the name of a group.  Trims whitespace; no-op if blank or not found.
   */
  function setGroupName(groupId: number, name: string) {
    const group = groups.value.find((g) => g.id === groupId);
    const trimmed = name.trim();
    if (group && trimmed && trimmed.length <= 80) group.name = trimmed;
  }

  /**
   * Set the local auth override for a group.
   * Pass `undefined` to clear (revert to inherit from parent).
   */
  function setGroupLocalAuth(
    groupId: number,
    localAuth: import("@/lib/authorization").AuthorizationConfig | undefined,
  ) {
    const group = groups.value.find((g) => g.id === groupId);
    if (!group) return;
    group.localAuth = localAuth;
  }

  /**
   * Set (or clear) the token definitions for a group.
   * Pass `undefined` to remove local definitions entirely.
   */
  function setGroupLocalDefinitions(
    groupId: number,
    localDefinitions: Record<string, string> | undefined,
  ) {
    const group = groups.value.find((g) => g.id === groupId);
    if (!group) return;
    group.localDefinitions = localDefinitions;
  }

  /**
   * Replace the workspace-wide global definitions map.
   * Merges shallowly: keys absent from the new map are removed.
   */
  function setGlobalDefinitions(defs: Record<string, string>) {
    globalDefinitions.value = { ...defs };
  }
  function setPreferences(next: WorkspacePreferences) {
    if (!validPreferences(next)) return;
    preferences.value = { ...next };
  }
  function setGroupParent(groupId: number, parentId: number | null) {
    const group = groups.value.find((g) => g.id === groupId);
    if (group && group.parentId !== parentId)
      moveGroup(groupId, parentId, null);
  }
  function setGroupNewRequestDefaults(
    groupId: number,
    defaultMethod: import("@/lib/request").Method | undefined,
    defaultUrl: string | undefined,
  ) {
    const group = groups.value.find((g) => g.id === groupId);
    if (!group) return;
    if (defaultMethod !== undefined && !isMethod(defaultMethod)) return;
    if (
      defaultUrl !== undefined &&
      (typeof defaultUrl !== "string" || defaultUrl.length > 65536)
    )
      return;
    group.defaultMethod = defaultMethod;
    group.defaultUrl = defaultUrl;
  }

  return {
    sessions,
    groups,
    openIds,
    activeId,
    ready,
    closing,
    error,
    status,
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
  };
}
