<script setup lang="ts">
import {
  computed,
  nextTick,
  onMounted,
  onUnmounted,
  reactive,
  ref,
  watch,
} from "vue";
import { ChevronDown, CopyPlus, Plus, X } from "lucide-vue-next";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuPortal,
  DropdownMenuRoot,
  DropdownMenuTrigger,
} from "reka-ui";
import {
  sessionLabel,
  sessionHost,
  sessionStatus,
  type RequestSession,
} from "@/lib/session";
import { useDragDrop, type DropHit } from "@/composables/useDragDrop";
import {
  hitZone,
  resolveTabDrop,
  stepTab,
  type DragPayload,
  type Point,
} from "@/lib/drag-drop";
const props = defineProps<{
  sessions: RequestSession[];
  activeId: number | null;
}>();
const emit = defineEmits<{
  select: [id: number];
  close: [id: number];
  create: [];
  duplicate: [];
  openRequests: [ids: number[], beforeId: number | null];
}>();
const strip = ref<HTMLElement>();
// Overflow state. The strip has no scrollbar, so fades and a count show
// that more tabs exist.
const overflow = reactive({ left: false, right: false, hidden: 0 });
function measure() {
  const el = strip.value;
  if (!el) return;
  const start = el.scrollLeft;
  const end = start + el.clientWidth;
  overflow.left = start > 1;
  overflow.right = end < el.scrollWidth - 1;
  overflow.hidden = Array.from(
    el.querySelectorAll<HTMLElement>("[data-tab-id]"),
  ).filter(
    (cell) =>
      cell.offsetLeft < start - 1 ||
      cell.offsetLeft + cell.offsetWidth > end + 1,
  ).length;
}
const fadeMask = computed(() => {
  const left = overflow.left ? "transparent, #000 24px" : "#000, #000";
  const right = overflow.right
    ? "#000 calc(100% - 24px), transparent"
    : "#000, #000";
  return overflow.left || overflow.right
    ? { maskImage: `linear-gradient(to right, ${left}, ${right})` }
    : {};
});
// A vertical wheel scrolls the strip sideways. A horizontal gesture is left
// to the browser.
function wheel(event: WheelEvent) {
  const el = strip.value;
  if (!el || Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
  if (el.scrollWidth <= el.clientWidth) return;
  event.preventDefault();
  el.scrollLeft += event.deltaY;
  measure();
}
let resize: ResizeObserver | undefined;
onMounted(() => {
  measure();
  // jsdom has no ResizeObserver.
  if (typeof ResizeObserver === "undefined" || !strip.value) return;
  resize = new ResizeObserver(measure);
  resize.observe(strip.value);
});
onUnmounted(() => resize?.disconnect());
watch(
  () => props.sessions.length,
  () => void nextTick(measure),
);
const drag = useDragDrop();
// The bar is a reka `as-child` trigger, whose template ref binds only on the
// first mount. Reach it through the strip instead.
drag.registerSurface({
  el: () => strip.value?.parentElement,
  scroller: () => strip.value,
  axis: "x",
  resolve: resolveTabsDrop,
});
function resolveTabsDrop(payload: DragPayload, point: Point): DropHit | null {
  const openIds = props.sessions.map((session) => session.id);
  const cell = Array.from(
    strip.value?.querySelectorAll<HTMLElement>("[data-tab-id]") ?? [],
  ).find((candidate) => {
    const box = candidate.getBoundingClientRect();
    return point.x >= box.left && point.x < box.right;
  });
  // Past the last tab (over the buttons or empty bar) appends.
  const targetId = cell
    ? Number(cell.dataset.tabId)
    : (openIds[openIds.length - 1] ?? null);
  const zone = cell
    ? hitZone(cell.getBoundingClientRect(), point, "tab")
    : "after";
  const drop = resolveTabDrop(payload, targetId, zone, openIds);
  if (!drop) return null;
  return {
    key: drop.key,
    zone: drop.zone,
    commit: () => emit("openRequests", drop.ids, drop.beforeId),
  };
}
function pressTab(session: RequestSession, event: PointerEvent) {
  drag.startPress(event, {
    payload: () => ({ kind: "requests", ids: [session.id] }),
    preview: () => ({
      label: sessionLabel(session),
      method: session.draft.method,
    }),
  });
}
function dropZoneFor(id: number) {
  const hit = drag.state.hit;
  return hit?.key === `tab-${id}` ? hit.zone : null;
}
function isDragged(id: number) {
  const payload = drag.state.payload;
  return payload?.kind === "requests" && payload.ids.includes(id);
}
async function reveal() {
  await nextTick();
  strip.value
    ?.querySelector('[aria-selected="true"]')
    ?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  measure();
}
watch(() => props.activeId, reveal);
function navigate(event: KeyboardEvent, index: number) {
  if (
    event.altKey &&
    (event.key === "ArrowLeft" || event.key === "ArrowRight")
  ) {
    event.preventDefault();
    const id = props.sessions[index].id;
    const step = stepTab(
      props.sessions.map((session) => session.id),
      id,
      event.key === "ArrowLeft" ? -1 : 1,
    );
    if (step) emit("openRequests", [id], step.beforeId);
    void nextTick(() => document.getElementById(`request-tab-${id}`)?.focus());
    return;
  }
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
      <div
        class="flex min-w-0 shrink-0 h-9 bg-muted border-b border-border pointer-coarse:h-11"
        data-tab-bar
      >
        <div
          ref="strip"
          class="relative flex min-w-0 overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
          role="tablist"
          aria-label="Requests"
          :data-overflow-left="overflow.left"
          :data-overflow-right="overflow.right"
          :style="fadeMask"
          @scroll.passive="measure"
          @wheel="wheel"
        >
          <ContextMenu v-for="(session, index) in sessions" :key="session.id">
            <ContextMenuTrigger
              as-child
              :data-testid="`tab-ctx-trigger-${session.id}`"
            >
              <div
                class="tab-cell relative flex items-stretch shrink-0 w-52.5 border-r border-border text-muted-foreground max-[760px]:w-46.25"
                :class="{
                  selected: activeId === session.id,
                  'opacity-40': isDragged(session.id),
                }"
                role="presentation"
                :data-drop-key="`tab-${session.id}`"
                :data-tab-id="session.id"
                @pointerdown="pressTab(session, $event)"
              >
                <span
                  v-if="dropZoneFor(session.id)"
                  class="pointer-events-none absolute inset-y-0 z-10 w-0.5 bg-primary"
                  :class="
                    dropZoneFor(session.id) === 'before'
                      ? 'left-0'
                      : '-right-px'
                  "
                  :data-drop-indicator="dropZoneFor(session.id)"
                />
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
                  data-no-drag
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
        <DropdownMenuRoot v-if="overflow.hidden > 0">
          <DropdownMenuTrigger
            class="flex items-center justify-center gap-0.5 shrink-0 px-2 font-mono text-[0.625rem] text-muted-foreground border-x border-border cursor-pointer hover:bg-accent hover:text-foreground data-[state=open]:bg-accent data-[state=open]:text-foreground pointer-coarse:min-w-11"
            data-tab-overflow
            :aria-label="`${overflow.hidden} more tabs`"
            title="Show all open tabs"
          >
            <ChevronDown :size="13" aria-hidden="true" />
            {{ overflow.hidden }}
          </DropdownMenuTrigger>
          <DropdownMenuPortal>
            <DropdownMenuContent
              align="end"
              :side-offset="4"
              class="z-50 max-h-[60dvh] min-w-56 max-w-80 overflow-y-auto rounded border border-border bg-secondary p-1 font-mono text-xs text-foreground shadow-sm"
              data-surface="context-menu"
            >
              <DropdownMenuItem
                v-for="session in sessions"
                :key="session.id"
                data-tab-overflow-item
                class="flex items-center gap-2 cursor-default select-none rounded-sm px-2 py-1.5 outline-none data-highlighted:bg-accent data-[active=true]:text-foreground pointer-coarse:py-3"
                :class="session.id === activeId ? '' : 'text-muted-foreground'"
                @select="emit('select', session.id)"
              >
                <span
                  class="method w-11 shrink-0 text-[0.5625rem] font-bold tracking-[0.04em]"
                  :data-method="session.draft.method"
                  >{{ session.draft.method }}</span
                >
                <span class="truncate">{{ sessionLabel(session) }}</span>
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenuPortal>
        </DropdownMenuRoot>
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
  content: "";
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
