<script setup lang="ts">
import { computed, nextTick, ref, useId, watch } from 'vue'
import { ArrowDown, ArrowUp, Eraser, SendHorizontal } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import type { SocketSession } from '@/lib/websocket'
import { formatBytes } from '@/lib/request'
import { shortcutLabel } from '@/lib/shortcut'

const props = defineProps<{ socket?: SocketSession }>()
const message = defineModel<string>('message', { default: '' })
const emit = defineEmits<{ send: [text: string]; clear: [] }>()
const headingId = useId()
const log = ref<HTMLElement>()
const open = computed(() => props.socket?.state === 'open')
const counts = computed(() => {
  const messages = props.socket?.messages ?? []
  return {
    in: messages.filter((m) => m.direction === 'in').length,
    out: messages.filter((m) => m.direction === 'out').length,
  }
})
const stateLabel = computed(
  () =>
    ({
      connecting: 'CONNECTING',
      open: 'CONNECTED',
      closing: 'CLOSING',
      closed: props.socket ? 'CLOSED' : 'STANDBY',
    })[props.socket?.state ?? 'closed'],
)
const clock = (at: number) =>
  new Date(at).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  })
/** Show JSON on one line; keep other text as sent. */
function display(text: string) {
  try {
    return JSON.stringify(JSON.parse(text))
  } catch {
    return text
  }
}

// Follow new messages while the log is at the bottom.
let pinned = true
function onScroll() {
  const el = log.value
  if (el) pinned = el.scrollHeight - el.scrollTop - el.clientHeight < 24
}
watch(
  () => props.socket?.messages.length,
  async () => {
    if (!pinned) return
    await nextTick()
    log.value?.scrollTo({ top: log.value.scrollHeight })
  },
)
function submit(event?: KeyboardEvent) {
  if (event) {
    // The window shortcut would connect or disconnect instead.
    event.preventDefault()
    event.stopPropagation()
  }
  if (open.value && message.value) emit('send', message.value)
}
</script>

<template>
  <section
    class="flex min-h-0 min-w-0 flex-col bg-background"
    :aria-labelledby="headingId"
    data-websocket-panel>
    <header
      class="flex h-9 shrink-0 items-center gap-3 border-b border-border bg-muted px-4 font-mono text-[0.625rem] tracking-[0.12em]">
      <h2 :id="headingId" class="text-[0.6875rem] font-semibold uppercase">WebSocket</h2>
      <span
        class="ml-auto"
        :class="open ? 'text-success' : 'text-muted-foreground'"
        role="status"
        data-socket-state>
        {{ stateLabel }}
      </span>
    </header>
    <div
      class="flex min-h-9 shrink-0 items-center gap-4 border-b border-border px-4 font-mono text-[0.6875rem] tabular-nums">
      <span class="inline-flex items-center gap-1" title="Received">
        <ArrowDown :size="12" aria-hidden="true" class="text-info" />
        {{ counts.in }}
      </span>
      <span class="inline-flex items-center gap-1" title="Sent">
        <ArrowUp :size="12" aria-hidden="true" class="text-primary" />
        {{ counts.out }}
      </span>
      <Button
        variant="ghost"
        class="ml-auto size-7 p-0"
        aria-label="Clear messages"
        title="Clear messages"
        data-clear-socket
        :disabled="!socket?.messages.length"
        @click="emit('clear')">
        <Eraser :size="13" aria-hidden="true" />
      </Button>
    </div>
    <div
      ref="log"
      class="min-h-0 flex-1 overflow-auto font-mono text-[0.75rem]"
      tabindex="0"
      aria-label="WebSocket messages"
      aria-live="polite"
      data-socket-log
      @scroll.passive="onScroll">
      <div
        v-for="entry in socket?.messages ?? []"
        :key="entry.id"
        class="grid grid-cols-[4.5rem_1rem_1fr] gap-2 border-b border-border px-4 py-1.5 leading-[1.6]"
        :class="{ 'text-muted-foreground': entry.direction === 'system' }"
        :data-socket-message="entry.direction">
        <span class="text-muted-foreground tabular-nums">{{ clock(entry.at) }}</span>
        <span
          :class="
            entry.direction === 'in' ? 'text-info' : entry.direction === 'out' ? 'text-primary' : ''
          "
          :aria-label="
            entry.direction === 'in' ? 'Received' : entry.direction === 'out' ? 'Sent' : 'Status'
          ">
          {{ entry.direction === 'in' ? '↓' : entry.direction === 'out' ? '↑' : '·' }}
        </span>
        <!-- prettier-ignore -->
        <span class="min-w-0 whitespace-pre-wrap wrap-anywhere"><template v-if="entry.binary"><span class="text-muted-foreground">Binary · {{ formatBytes(entry.size) }} · </span></template>{{ entry.direction === 'system' ? entry.text : display(entry.text) }}</span>
      </div>
      <p v-if="!socket?.messages.length" class="p-4 text-xs text-muted-foreground">
        Connect to open the socket. Messages you send and receive show here.
      </p>
    </div>
    <form
      class="flex shrink-0 items-end gap-2 border-t border-border p-2"
      @submit.prevent="submit()">
      <label :for="`${headingId}-message`" class="sr-only">Message</label>
      <textarea
        :id="`${headingId}-message`"
        v-model="message"
        rows="3"
        spellcheck="false"
        placeholder="Message text or JSON"
        data-socket-message-input
        class="min-h-15 flex-1 resize-y rounded border border-input bg-background px-2.5 py-1.5 font-mono text-xs focus:border-primary"
        @keydown.enter.meta="submit($event)"
        @keydown.enter.ctrl="submit($event)" />
      <Button
        type="submit"
        data-socket-send
        :disabled="!open || !message"
        :title="`Send message · ${shortcutLabel(['mod', 'enter'])}`">
        <SendHorizontal :size="14" aria-hidden="true" />
        Send
      </Button>
    </form>
  </section>
</template>
