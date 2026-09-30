<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import type { SseEvent } from '@/lib/sse'

const props = defineProps<{ events: SseEvent[]; live?: boolean }>()
const element = ref<HTMLElement>()
/** Rendered rows. The stream keeps more; the newest show. */
const RENDER_LIMIT = 1000
const shown = computed(() => props.events.slice(-RENDER_LIMIT))
const hidden = computed(() => props.events.length - shown.value.length)

/** Pretty JSON data on one line stays readable; other data as sent. */
function display(data: string) {
  try {
    return JSON.stringify(JSON.parse(data))
  } catch {
    return data
  }
}
const seconds = (ms?: number) => (ms === undefined ? '' : `${(ms / 1000).toFixed(2)} s`)

// Follow new events while the view is at the bottom, like a terminal.
let pinned = true
function onScroll() {
  const el = element.value
  if (el) pinned = el.scrollHeight - el.scrollTop - el.clientHeight < 24
}
watch(
  () => props.events.length,
  async () => {
    if (!props.live || !pinned) return
    await nextTick()
    element.value?.scrollTo({ top: element.value.scrollHeight })
  },
)
</script>

<template>
  <div
    ref="element"
    class="min-h-0 flex-1 overflow-auto font-mono text-[0.75rem]"
    data-event-list
    tabindex="0"
    aria-label="Server-sent events"
    @scroll.passive="onScroll">
    <p v-if="hidden" class="border-b border-border px-4 py-1 text-[0.625rem] text-muted-foreground">
      {{ hidden }} earlier events not shown
    </p>
    <div
      v-for="(event, index) in shown"
      :key="hidden + index"
      class="grid grid-cols-[4.5rem_7rem_1fr] gap-3 border-b border-border px-4 py-1.5 leading-[1.6]"
      data-event>
      <span class="text-muted-foreground tabular-nums">
        {{ seconds(event.at) || `#${hidden + index + 1}` }}
      </span>
      <span class="truncate text-info" :title="event.id ? `id ${event.id}` : undefined">
        {{ event.event }}
      </span>
      <span class="min-w-0 whitespace-pre-wrap wrap-anywhere">{{ display(event.data) }}</span>
    </div>
    <p v-if="!events.length" class="p-4 text-xs text-muted-foreground">
      {{ live ? 'Connected. Waiting for events…' : 'No events in this response.' }}
    </p>
  </div>
</template>
