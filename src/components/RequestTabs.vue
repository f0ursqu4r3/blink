<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { Plus, X } from "lucide-vue-next";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
  sessionLabel,
  sessionHost,
  sessionStatus,
  type RequestSession,
} from "@/lib/session";
const props = defineProps<{ sessions: RequestSession[]; activeId: number }>();
const emit = defineEmits<{
  select: [id: number];
  close: [id: number];
  create: [];
  duplicate: [];
}>();
const strip = ref<HTMLElement>();
async function reveal() {
  await nextTick();
  strip.value
    ?.querySelector('[aria-selected="true"]')
    ?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
}
watch(() => props.activeId, reveal);
function navigate(event: KeyboardEvent, index: number) {
  if (event.altKey || event.ctrlKey || event.metaKey) return;
  let next = index;
  if (event.key === "ArrowRight") next = (index + 1) % props.sessions.length;
  else if (event.key === "ArrowLeft")
    next = (index + props.sessions.length - 1) % props.sessions.length;
  else if (event.key === "Home") next = 0;
  else if (event.key === "End") next = props.sessions.length - 1;
  else if (event.key === "Delete") {
    event.preventDefault();
    emit("close", props.sessions[index].id);
    return;
  } else return;
  event.preventDefault();
  emit("select", props.sessions[next].id);
  void nextTick(() =>
    document.getElementById(`request-tab-${props.sessions[next].id}`)?.focus(),
  );
}
</script>

<template>
  <ContextMenu>
    <ContextMenuTrigger as-child data-testid="tab-strip-ctx-trigger">
      <div class="tab-strip">
        <div
          ref="strip"
          class="request-tabs"
          role="tablist"
          aria-label="Requests"
        >
          <ContextMenu v-for="(session, index) in sessions" :key="session.id">
            <ContextMenuTrigger
              as-child
              :data-testid="`tab-ctx-trigger-${session.id}`"
            >
              <div
                class="tab-cell"
                :class="{ selected: activeId === session.id }"
                role="presentation"
              >
                <button
                  type="button"
                  role="tab"
                  :id="`request-tab-${session.id}`"
                  :aria-controls="`request-pane-${session.id}`"
                  :aria-selected="activeId === session.id"
                  :tabindex="activeId === session.id ? 0 : -1"
                  :title="`${session.draft.method} ${sessionLabel(session)} · ${sessionHost(session)} · ${sessionStatus(session)}`"
                  @click="emit('select', session.id)"
                  @keydown="navigate($event, index)"
                >
                  <span
                    class="tab-method"
                    :data-method="session.draft.method"
                    >{{ session.draft.method }}</span
                  >
                  <span class="tab-label"
                    ><span class="tab-path">{{ sessionLabel(session) }}</span
                    ><span
                      v-if="
                        sessionHost(session) &&
                        sessionHost(session) !== sessionLabel(session)
                      "
                      class="tab-host"
                      >{{ sessionHost(session) }}</span
                    ></span
                  >
                  <span
                    v-if="session.busy"
                    class="tab-state sending"
                    aria-label="Sending"
                    >↗</span
                  >
                  <span
                    v-else-if="session.error"
                    class="tab-state failed"
                    aria-label="Request failed"
                    >!</span
                  >
                  <span
                    v-else-if="session.response"
                    class="tab-state"
                    :class="{ failed: session.response.status >= 400 }"
                    >{{ sessionStatus(session) }}</span
                  >
                </button>
                <button
                  type="button"
                  class="tab-close"
                  data-close-request
                  :disabled="session.busy"
                  :aria-label="`Close ${sessionLabel(session)}`"
                  :title="
                    session.busy
                      ? 'Wait for the request to finish'
                      : 'Close request'
                  "
                  @click="emit('close', session.id)"
                >
                  <X :size="12" aria-hidden="true" />
                </button>
              </div>
            </ContextMenuTrigger>
            <ContextMenuContent>
              <ContextMenuItem
                :data-testid="`tab-ctx-select-${session.id}`"
                @select="emit('select', session.id)"
                >Select</ContextMenuItem
              >
              <ContextMenuItem
                :data-testid="`tab-ctx-duplicate-${session.id}`"
                @select="
                  () => {
                    emit('select', session.id);
                    emit('duplicate');
                  }
                "
                >Duplicate</ContextMenuItem
              >
              <ContextMenuSeparator />
              <ContextMenuItem
                :data-testid="`tab-ctx-close-${session.id}`"
                :disabled="session.busy"
                @select="emit('close', session.id)"
                >Close</ContextMenuItem
              >
            </ContextMenuContent>
          </ContextMenu>
        </div>
        <button
          type="button"
          class="new-tab"
          data-new-request
          aria-label="New request"
          title="New request · Cmd/Ctrl+T"
          @click="emit('create')"
        >
          <Plus :size="15" aria-hidden="true" />
        </button>
      </div>
    </ContextMenuTrigger>
    <ContextMenuContent>
      <ContextMenuItem data-testid="tab-strip-ctx-new" @select="emit('create')"
        >New request</ContextMenuItem
      >
      <ContextMenuItem
        data-testid="tab-strip-ctx-duplicate"
        @select="emit('duplicate')"
        >Duplicate active</ContextMenuItem
      >
    </ContextMenuContent>
  </ContextMenu>
</template>

<style scoped>
.tab-strip {
  display: flex;
  min-width: 0;
  flex-shrink: 0;
  height: 36px;
  background: var(--muted);
  border-bottom: 1px solid var(--border);
}
.request-tabs {
  display: flex;
  min-width: 0;
  overflow-x: auto;
  scrollbar-width: thin;
}
.tab-cell {
  display: flex;
  align-items: stretch;
  flex-shrink: 0;
  width: 210px;
  border-right: 1px solid var(--border);
  position: relative;
  color: var(--muted-foreground);
}
.tab-cell.selected {
  background: var(--secondary);
  color: var(--foreground);
}
.tab-cell.selected::before {
  position: absolute;
  content: "";
  inset: 0 0 auto;
  height: 2px;
  background: var(--primary);
}
.tab-cell:not(.selected):hover {
  background: var(--accent);
}
[role="tab"] {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 0 8px 0 14px;
  min-width: 0;
  flex: 1;
  text-align: left;
  font: 0.6875rem var(--font-mono);
  cursor: pointer;
}
.tab-method {
  font-size: 0.5625rem;
  font-weight: 700;
  letter-spacing: 0.04em;
  color: var(--primary);
}
.tab-method[data-method="GET"] {
  color: var(--success);
}
.tab-method[data-method="DELETE"] {
  color: var(--destructive);
}
.tab-label {
  display: flex;
  flex-direction: column;
  justify-content: center;
  min-width: 0;
  flex: 1;
  line-height: 1.3;
}
.tab-path,
.tab-host {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tab-host {
  font-size: 0.5625rem;
  color: var(--muted-foreground);
}
.tab-state {
  font-size: 0.5625rem;
  color: var(--success);
}
.tab-state.failed {
  color: var(--destructive);
}
.sending {
  color: var(--primary);
  animation: activity 1s ease-in-out infinite alternate;
}
.tab-close {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  flex-shrink: 0;
  color: var(--muted-foreground);
  cursor: pointer;
}
.tab-close:hover:not(:disabled) {
  color: var(--foreground);
  background: var(--accent);
}
.tab-close:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}
.new-tab {
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  width: 38px;
  color: var(--muted-foreground);
  border-right: 1px solid var(--border);
  cursor: pointer;
}
.new-tab:hover {
  background: var(--accent);
  color: var(--primary);
}
@keyframes activity {
  from {
    opacity: 0.4;
  }
  to {
    opacity: 1;
  }
}
@media (prefers-reduced-motion: reduce) {
  .sending {
    animation: none;
  }
}
@media (max-width: 760px) {
  .tab-cell {
    width: 185px;
  }
}
@media (pointer: coarse) {
  .tab-strip {
    height: 44px;
  }
  .tab-close,
  .new-tab {
    width: 44px;
  }
}
</style>
