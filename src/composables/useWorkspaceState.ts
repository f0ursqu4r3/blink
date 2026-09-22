import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { createSession, type RequestSession } from "@/lib/session";
import {
  createGroup,
  deleteGroupAndPromoteContents,
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

export function useWorkspaceState() {
  const sessions = ref<RequestSession[]>([createSession()]);
  const groups = ref<RequestGroup[]>([]);
  const activeId = ref(sessions.value[0].id);
  const ready = ref(false);
  const error = ref("");
  const saving = ref(false);
  const exitBlocked = ref(false);
  /** Workspace-global token definitions, shared across all requests. */
  const globalDefinitions = ref<Record<string, string>>({});
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
      activeId.value = restored.activeId;
      globalDefinitions.value = restored.globalDefinitions ?? {};
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
  watch(
    () =>
      ready.value
        ? encodeWorkspace(
            sessions.value,
            activeId.value,
            groups.value,
            globalDefinitions.value,
          )
        : null,
    () => {
      if (ready.value && sessions.value.length) void flush();
    },
  );
  async function reset() {
    const session = createSession();
    try {
      await writer.save(encodeWorkspace([session], session.id, [], {}));
      sessions.value = [session];
      groups.value = [];
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
  function addGroup(name: string, parentId: number | null) {
    const group = createGroup(name, parentId);
    groups.value.push(group);
    return group;
  }
  function renameGroup(id: number, name: string) {
    const group = groups.value.find((candidate) => candidate.id === id);
    if (group && name.trim()) group.name = name.trim();
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
    const ids = new Set(sessionIds);
    if (!ids.size) return;
    if (groupId !== null && !groups.value.some((group) => group.id === groupId))
      return;

    const moving = sessions.value.filter((session) => ids.has(session.id));
    if (!moving.length) return;
    const remaining = sessions.value.filter((session) => !ids.has(session.id));
    const beforeIndex =
      beforeSessionId === null
        ? -1
        : remaining.findIndex((session) => session.id === beforeSessionId);
    const endIndex = remaining.reduce(
      (index, session, current) =>
        session.groupId === groupId ? current + 1 : index,
      -1,
    );
    const insertAt = beforeIndex >= 0 ? beforeIndex : endIndex + 1;
    moving.forEach((session) => {
      session.groupId = groupId;
    });
    remaining.splice(insertAt, 0, ...moving);
    sessions.value = remaining;
  }
  function reorderGroup(groupId: number, beforeGroupId: number) {
    const sourceIndex = groups.value.findIndex((group) => group.id === groupId);
    const targetIndex = groups.value.findIndex(
      (group) => group.id === beforeGroupId,
    );
    const source = groups.value[sourceIndex];
    const target = groups.value[targetIndex];
    if (
      !source ||
      !target ||
      source.id === target.id ||
      source.parentId !== target.parentId
    )
      return;
    const next = [...groups.value];
    next.splice(sourceIndex, 1);
    next.splice(
      next.findIndex((group) => group.id === target.id),
      0,
      source,
    );
    groups.value = next;
  }
  function deleteGroup(id: number) {
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
    if (group && name.trim()) group.name = name.trim();
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

  return {
    sessions,
    groups,
    activeId,
    ready,
    closing,
    error,
    status,
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
  };
}
