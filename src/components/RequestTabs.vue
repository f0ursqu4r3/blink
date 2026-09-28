<script setup lang="ts">
import { nextTick, ref, watch } from 'vue';
import { CopyPlus, Plus, X } from 'lucide-vue-next';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import {
  sessionLabel,
  sessionHost,
  sessionStatus,
  type RequestSession,
} from '@/lib/session';
const props = defineProps<{
  sessions: RequestSession[];
  activeId: number | null;
}>();
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
    ?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' });
}
watch(() => props.activeId, reveal);
function navigate(event: KeyboardEvent, index: number) {
  if (event.altKey || event.ctrlKey || event.metaKey) return;
  let next = index;
  if (event.key === 'ArrowRight') next = (index + 1) % props.sessions.length;
  else if (event.key === 'ArrowLeft')
    next = (index + props.sessions.length - 1) % props.sessions.length;
  else if (event.key === 'Home') next = 0;
  else if (event.key === 'End') next = props.sessions.length - 1;
  else if (event.key === 'Delete') {
    event.preventDefault();
    emit('close', props.sessions[index].id);
    return;
  } else return;
  event.preventDefault();
  emit('select', props.sessions[next].id);
  void nextTick(() =>
    document.getElementById(`request-tab-${props.sessions[next].id}`)?.focus()
  );
}
</script>

<template>
  <ContextMenu>
    <ContextMenuTrigger as-child data-testid="tab-strip-ctx-trigger">
      <div
        class="flex min-w-0 shrink-0 h-9 bg-muted border-b border-border pointer-coarse:h-11"
      >
        <div
          ref="strip"
          class="flex min-w-0 overflow-x-auto scrollbar-thin"
          role="tablist"
          aria-label="Requests"
        >
          <ContextMenu v-for="(session, index) in sessions" :key="session.id">
            <ContextMenuTrigger
              as-child
              :data-testid="`tab-ctx-trigger-${session.id}`"
            >
              <div
                class="tab-cell relative flex items-stretch shrink-0 w-52.5 border-r border-border text-muted-foreground max-[760px]:w-46.25"
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
                  class="flex items-center gap-2.25 pl-3.5 pr-2 min-w-0 flex-1 text-left font-mono text-[0.6875rem] cursor-pointer"
                  @click="emit('select', session.id)"
                  @keydown="navigate($event, index)"
                >
                  <span
                    class="method text-[0.5625rem] font-bold tracking-[0.04em]"
                    :data-method="session.draft.method"
                  >
                    {{ session.draft.method }}
                  </span>
                  <span
                    class="flex flex-col justify-center min-w-0 flex-1 leading-[1.3]"
                  >
                    <span class="truncate">{{ sessionLabel(session) }}</span>
                    <span
                      v-if="
                        sessionHost(session) &&
                        sessionHost(session) !== sessionLabel(session)
                      "
                      class="truncate text-[0.5625rem] text-muted-foreground"
                    >
                      {{ sessionHost(session) }}
                    </span>
                  </span>
                  <span
                    v-if="session.busy"
                    class="text-[0.5625rem] text-primary sending"
                    aria-label="Sending"
                  >
                    ↗
                  </span>
                  <span
                    v-else-if="session.error"
                    class="text-[0.5625rem] text-destructive"
                    aria-label="Request failed"
                  >
                    !
                  </span>
                  <span
                    v-else-if="session.response"
                    class="text-[0.5625rem]"
                    :class="
                      session.response.status >= 400
                        ? 'text-destructive'
                        : 'text-success'
                    "
                  >
                    {{ sessionStatus(session) }}
                  </span>
                </button>
                <button
                  type="button"
                  class="flex items-center justify-center w-6.5 shrink-0 text-muted-foreground cursor-pointer hover:text-foreground hover:bg-accent pointer-coarse:w-11"
                  data-close-request
                  :aria-label="`Close ${sessionLabel(session)}`"
                  title="Close tab · Cmd/Ctrl+W"
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
              >
                Select
              </ContextMenuItem>
              <ContextMenuItem
                :data-testid="`tab-ctx-duplicate-${session.id}`"
                @select="
                  () => {
                    emit('select', session.id);
                    emit('duplicate');
                  }
                "
              >
                Duplicate
              </ContextMenuItem>
              <ContextMenuSeparator />
              <ContextMenuItem
                :data-testid="`tab-ctx-close-${session.id}`"
                @select="emit('close', session.id)"
              >
                Close
              </ContextMenuItem>
            </ContextMenuContent>
          </ContextMenu>
        </div>
        <button
          type="button"
          class="flex items-center justify-center shrink-0 w-9.5 text-muted-foreground border-r border-border cursor-pointer hover:bg-accent hover:text-foreground pointer-coarse:w-11"
          data-new-request
          aria-label="New request"
          title="New request · Cmd/Ctrl+T"
          @click="emit('create')"
        >
          <Plus :size="15" aria-hidden="true" />
        </button>
        <button
          type="button"
          class="flex items-center justify-center shrink-0 w-9.5 text-muted-foreground border-r border-border cursor-pointer hover:bg-accent hover:text-foreground pointer-coarse:w-11"
          data-duplicate-request
          aria-label="Duplicate request"
          title="Duplicate request · Cmd/Ctrl+Shift+D"
          @click="emit('duplicate')"
        >
          <CopyPlus :size="14" aria-hidden="true" />
        </button>
      </div>
    </ContextMenuTrigger>
    <ContextMenuContent>
      <ContextMenuItem data-testid="tab-strip-ctx-new" @select="emit('create')">
        New request
      </ContextMenuItem>
      <ContextMenuItem
        data-testid="tab-strip-ctx-duplicate"
        @select="emit('duplicate')"
      >
        Duplicate active
      </ContextMenuItem>
    </ContextMenuContent>
  </ContextMenu>
</template>

<style scoped>
/* Selected tab top-border indicator — pseudo-element, cannot be a utility */
.tab-cell.selected {
  background: var(--secondary);
  color: var(--foreground);
}
.tab-cell.selected::before {
  position: absolute;
  content: '';
  inset: 0 0 auto;
  height: 2px;
  background: var(--primary);
}
.tab-cell:not(.selected):hover {
  background: var(--accent);
}
/* Sending animation */
@keyframes activity {
  from {
    opacity: 0.4;
  }
  to {
    opacity: 1;
  }
}
.sending {
  animation: activity 1s ease-in-out infinite alternate;
}
@media (prefers-reduced-motion: reduce) {
  .sending {
    animation: none;
  }
}
</style>
