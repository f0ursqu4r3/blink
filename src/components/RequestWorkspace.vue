<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { ArrowUpRight, Check, Plug, Square, Terminal, Unplug, X } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuCheckboxItem,
  ContextMenuShortcut,
  ContextMenuSub,
  ContextMenuSubTrigger,
  ContextMenuSubContent,
} from '@/components/ui/context-menu'
import { shortcutLabel } from '@/lib/shortcut'
import RequestEditor from './RequestEditor.vue'
import ResponsePanel from './ResponsePanel.vue'
import WebSocketPanel from './WebSocketPanel.vue'
import { isWebSocketUrl } from '@/lib/websocket'
import { useWebSocket } from '@/composables/useWebSocket'
import type { Environment } from '@/lib/environments'
import { methods } from '@/lib/request'
import {
  codeTargets,
  codeLanguage,
  codeTargetLabel,
  generateCode,
  type CodeTarget,
} from '@/lib/codegen'
import { isCurlCommand, parseCurl } from '@/lib/curl-import'
import { highlightSource } from '@/lib/response-content'
import type { RequestSession } from '@/lib/session'
import type { RequestGroup } from '@/lib/groups'
import type { TransportOptions } from '@/lib/transport-options'
import type { PaneLayout } from '@/lib/preferences'
import { buildResolvedRequestContext, resolveAuthorization } from '@/lib/authorization'
import { useRequestRunner } from '@/composables/useRequestRunner'
import { useClipboard } from '@/composables/useClipboard'
import HelpTooltip from './HelpTooltip.vue'
import TokenInput from './TokenInput.vue'
const props = defineProps<{
  session: RequestSession
  active: boolean
  groups?: RequestGroup[]
  globalDefinitions?: Record<string, string>
  transport?: TransportOptions
  /** Side by side (default), or request above response. */
  layout?: PaneLayout
  /** Language in the code panel. */
  codeTarget?: CodeTarget
  /** The active environment of the request's root group. */
  environment?: Environment
  /** A protected environment was confirmed since it became active. */
  environmentConfirmed?: boolean
}>()
const emit = defineEmits<{
  'update:codeTarget': [target: CodeTarget]
  capture: [values: Record<string, string>]
  confirmEnvironment: [id: number]
}>()

const resolvedCtx = computed(() =>
  buildResolvedRequestContext(
    props.session.draft,
    props.session.groupId ?? null,
    props.groups ?? [],
    props.globalDefinitions ?? {},
  ),
)

const effectiveAuth = computed(() =>
  resolveAuthorization(
    props.session.draft.localAuth,
    props.session.groupId ?? null,
    props.groups ?? [],
  ),
)

const inheritedSource = computed(() => {
  if (props.session.draft.localAuth !== undefined) return undefined
  const groups = props.groups ?? []
  const byId = new Map(groups.map((g) => [g.id, g]))
  let cursor: number | null = props.session.groupId ?? null
  const seen = new Set<number>()
  while (cursor !== null) {
    if (seen.has(cursor)) break
    seen.add(cursor)
    const g = byId.get(cursor)
    if (!g) break
    if (g.localAuth !== undefined) {
      const authLabel =
        g.localAuth.type === 'bearer' ? 'Bearer' : g.localAuth.type === 'basic' ? 'Basic' : 'None'
      return `${g.name} · ${authLabel}`
    }
    cursor = g.parentId
  }
  return undefined
})

const { prepared, curl, stale, send, cancel, sentUrl, recheck } = useRequestRunner(
  props.session,
  resolvedCtx,
  () => props.transport,
  {
    onCapture: (values) => emit('capture', values),
  },
)
watch(stale, (value) => (props.session.stale = value), { immediate: true })
/** A ws:// or wss:// URL makes this a WebSocket request. */
const websocket = computed(() => isWebSocketUrl(props.session.draft.url))
const socket = useWebSocket(props.session, resolvedCtx, () => props.transport)
const validationError = computed(() =>
  websocket.value ? socket.prepared.value.error : prepared.value.error,
)
/** A protected environment waits for this before the first send. */
const confirming = ref(false)
function confirmAndSend() {
  if (props.environment) emit('confirmEnvironment', props.environment.id)
  confirming.value = false
  return run()
}
watch(
  () => props.environment?.id,
  () => (confirming.value = false),
)
/** Send an HTTP request, or connect and disconnect a WebSocket. */
function primary() {
  const connecting = !websocket.value || !socket.active.value
  if (connecting && props.environment?.protected && !props.environmentConfirmed) {
    confirming.value = true
    return
  }
  return run()
}
function run() {
  if (!websocket.value) return send()
  if (socket.active.value) socket.disconnect()
  else socket.connect()
}
async function sendSocketMessage(text: string) {
  if (await socket.send(text)) props.session.draft.body = ''
}
const { copied, copyError, copy } = useClipboard()
const showCurl = ref(false)
const target = computed(() => props.codeTarget ?? 'curl')
/** Code for `id`, or "" when the draft does not build. */
function codeFor(id: CodeTarget) {
  return prepared.value.request ? generateCode(id, prepared.value.request, props.transport) : ''
}
const code = computed(() => codeFor(target.value))
// highlight.js escapes the text, so v-html shows it as code only.
const codeHtml = computed(() =>
  showCurl.value ? highlightSource(code.value, codeLanguage(target.value)) : '',
)
const urlInput = ref<InstanceType<typeof TokenInput>>()
const workspace = ref<HTMLElement>()
const panels = ref<HTMLElement>()
const requestPanelWidth = ref(420)
const requestPanelHeight = ref(280)
const stacked = computed(() => props.layout === 'vertical')
const resizing = ref(false)
const prefix = `request-${props.session.id}`
const shortcut = /Mac/i.test(navigator.platform) ? '⌘' : 'Ctrl'
const minimumPanelWidth = 280
const minimumResponseWidth = 340
const minimumPanelHeight = 160
const minimumResponseHeight = 200
const resizeHandleWidth = 8
const maximumPanelWidth = computed(() =>
  Math.max(
    minimumPanelWidth,
    (workspace.value?.clientWidth ?? 900) - minimumResponseWidth - resizeHandleWidth,
  ),
)
const panelWidth = computed(() =>
  Math.round(
    Math.min(maximumPanelWidth.value, Math.max(minimumPanelWidth, requestPanelWidth.value)),
  ),
)
const maximumPanelHeight = computed(() =>
  Math.max(
    minimumPanelHeight,
    (panels.value?.clientHeight ?? 600) - minimumResponseHeight - resizeHandleWidth,
  ),
)
const panelHeight = computed(() =>
  Math.round(
    Math.min(maximumPanelHeight.value, Math.max(minimumPanelHeight, requestPanelHeight.value)),
  ),
)
const panelStyle = computed(() => ({
  '--request-panel-width': `${panelWidth.value}px`,
  '--request-panel-height': `${panelHeight.value}px`,
}))
/** The handle's range and value along the split axis. */
const resizeRange = computed(() =>
  stacked.value
    ? {
        min: minimumPanelHeight,
        max: maximumPanelHeight.value,
        now: panelHeight.value,
      }
    : {
        min: minimumPanelWidth,
        max: maximumPanelWidth.value,
        now: panelWidth.value,
      },
)
const importNotice = ref('')
const importError = ref('')
/** Pasting a cURL command into the URL field replaces the draft with it. */
function pasteUrl(event: ClipboardEvent) {
  const text = event.clipboardData?.getData('text/plain') ?? ''
  if (!isCurlCommand(text)) return
  event.preventDefault()
  importNotice.value = ''
  importError.value = ''
  try {
    const { draft, ignored } = parseCurl(text)
    Object.assign(props.session.draft, draft)
    importNotice.value = ignored.length
      ? `Imported cURL command. Ignored: ${ignored.join(' ')}`
      : 'Imported cURL command.'
  } catch (cause) {
    importError.value = cause instanceof Error ? cause.message : String(cause)
  }
}
watch(
  () => props.session.draft.url,
  () => {
    importError.value = ''
  },
)

/** Methods are case-sensitive tokens; type them as uppercase. */
function setMethod(value: string) {
  props.session.draft.method = value.trim().toUpperCase()
}
async function focusUrl() {
  await nextTick()
  urlInput.value?.focus()
  urlInput.value?.select()
}
function onKey(event: KeyboardEvent) {
  if (
    !props.active ||
    event.isComposing ||
    event.repeat ||
    event.defaultPrevented ||
    event.altKey ||
    document.querySelector('[role="dialog"], [data-surface="context-menu"]')
  )
    return
  if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
    event.preventDefault()
    void primary()
  }
  if (event.key === '.' && (event.metaKey || event.ctrlKey) && props.session.busy) {
    event.preventDefault()
    cancel()
  }
  if (event.key.toLowerCase() === 'l' && (event.metaKey || event.ctrlKey)) {
    event.preventDefault()
    void focusUrl()
  }
  if (event.key === 'Escape' && showCurl.value) {
    event.preventDefault()
    showCurl.value = false
  }
}

const responsePanel = ref<InstanceType<typeof ResponsePanel>>()
defineExpose({
  send: primary,
  cancel,
  focusUrl,
  toggleCode: () => (showCurl.value = !showCurl.value),
  /** Code for the request in `target`, or "" when it does not build. */
  codeFor,
  response: () => responsePanel.value,
})

onMounted(() => {
  window.addEventListener('keydown', onKey)
  if (props.active) void focusUrl()
})
onUnmounted(() => {
  window.removeEventListener('keydown', onKey)
  stopResize()
})

function setPanelWidth(width: number) {
  requestPanelWidth.value = Math.min(maximumPanelWidth.value, Math.max(minimumPanelWidth, width))
}
function setPanelHeight(height: number) {
  requestPanelHeight.value = Math.min(
    maximumPanelHeight.value,
    Math.max(minimumPanelHeight, height),
  )
}
function setPanelSize(size: number) {
  if (stacked.value) setPanelHeight(size)
  else setPanelWidth(size)
}
function resizePointer(event: PointerEvent) {
  const box = panels.value?.getBoundingClientRect()
  if (stacked.value) setPanelHeight(event.clientY - (box?.top ?? 0))
  else setPanelWidth(event.clientX - (workspace.value?.getBoundingClientRect().left ?? 0))
}
function stopResize() {
  resizing.value = false
  window.removeEventListener('pointermove', resizePointer)
  window.removeEventListener('pointerup', stopResize)
}
function startResize(event: PointerEvent) {
  if (event.button !== 0) return
  event.preventDefault()
  resizing.value = true
  window.addEventListener('pointermove', resizePointer)
  window.addEventListener('pointerup', stopResize, { once: true })
}
function resizeWithKeyboard(event: KeyboardEvent) {
  const step = event.shiftKey ? 48 : 16
  const [less, more] = stacked.value ? ['ArrowUp', 'ArrowDown'] : ['ArrowLeft', 'ArrowRight']
  const { min, max, now } = resizeRange.value
  if (event.key === less) setPanelSize(now - step)
  else if (event.key === more) setPanelSize(now + step)
  else if (event.key === 'Home') setPanelSize(min)
  else if (event.key === 'End') setPanelSize(max)
  else return
  event.preventDefault()
}
</script>

<template>
  <section
    ref="workspace"
    class="flex flex-col flex-1 min-h-0 min-w-0 data-[active=false]:absolute data-[active=false]:inset-0 data-[active=false]:invisible data-[active=false]:pointer-events-none"
    data-request-pane
    :data-active="active"
    :aria-hidden="!active"
    :id="`request-pane-${session.id}`"
    role="tabpanel"
    :aria-labelledby="`request-tab-${session.id}`">
    <ContextMenu>
      <ContextMenuTrigger as-child>
        <form
          class="request-bar flex items-center gap-2 px-3.5 py-3 border-b border-border bg-secondary max-[900px]:px-2.5 max-[900px]:gap-1.5"
          @submit.prevent="primary">
          <div
            class="flex items-stretch flex-1 min-w-0 h-8.5 border border-input rounded bg-background focus-within:border-primary pointer-coarse:min-h-11">
            <div
              v-if="websocket"
              class="flex w-23.5 shrink-0 items-center border-r border-border px-3 font-mono text-[0.6875rem] font-semibold text-info"
              data-websocket-method
              title="WebSocket">
              WS
            </div>
            <div
              v-else
              class="method relative border-r border-border shrink-0"
              :data-http-method="session.draft.method">
              <label :for="`${prefix}-method`" class="sr-only">HTTP method</label>
              <input
                :id="`${prefix}-method`"
                data-method
                :value="session.draft.method"
                :list="`${prefix}-methods`"
                spellcheck="false"
                autocomplete="off"
                autocapitalize="characters"
                class="h-full w-23.5 px-3 border-0 bg-transparent text-inherit font-mono font-semibold text-[0.6875rem]"
                @input="setMethod(($event.target as HTMLInputElement).value)"
                @focus="($event.target as HTMLInputElement).select()" />
              <datalist :id="`${prefix}-methods`">
                <option v-for="method in methods" :key="method" :value="method" />
              </datalist>
            </div>
            <label :for="`${prefix}-url`" class="sr-only">Request URL</label>
            <TokenInput
              :id="`${prefix}-url`"
              data-request-url
              ref="urlInput"
              v-model="session.draft.url"
              :tokens="resolvedCtx"
              :aria-describedby="
                session.draft.url && validationError ? `${prefix}-validation` : undefined
              "
              type="text"
              inputmode="url"
              spellcheck="false"
              autocomplete="off"
              placeholder="https://api.example.com/v1/resource or paste a cURL command"
              @paste="pasteUrl"
              class="flex-1 min-w-0 bg-transparent border-0 px-3 font-mono text-xs pointer-coarse:text-base" />
          </div>
          <Button
            variant="secondary"
            class="curl-button h-8.5 max-[900px]:px-2"
            :disabled="!prepared.request || websocket"
            :aria-expanded="showCurl"
            :aria-controls="`${prefix}-curl`"
            aria-label="Code"
            title="Show the request as code"
            @click="showCurl = !showCurl">
            <Terminal :size="14" aria-hidden="true" />
            <span class="max-[900px]:hidden">Code</span>
          </Button>
          <Button
            v-if="websocket"
            type="submit"
            data-connect
            :variant="socket.active.value ? 'secondary' : 'default'"
            class="send-button h-8.5 px-3"
            :disabled="!socket.active.value && !socket.prepared.value.request">
            <Unplug v-if="socket.active.value" :size="14" aria-hidden="true" />
            <Plug v-else :size="14" aria-hidden="true" />
            <span>{{ socket.active.value ? 'Disconnect' : 'Connect' }}</span>
            <kbd class="opacity-65 text-[0.625rem] ml-2.5 max-[900px]:hidden">{{ shortcut }} ↵</kbd>
          </Button>
          <Button
            v-else-if="session.busy"
            type="button"
            data-cancel
            variant="secondary"
            class="send-button h-8.5 px-3"
            @click="cancel()">
            <Square :size="12" aria-hidden="true" />
            <span>Cancel</span>
            <kbd class="opacity-65 text-[0.625rem] ml-2.5 max-[900px]:hidden">{{ shortcut }} .</kbd>
          </Button>
          <Button
            v-else
            type="submit"
            data-send
            class="send-button h-8.5 px-3"
            :disabled="!prepared.request">
            <ArrowUpRight :size="15" aria-hidden="true" />
            <span>Send</span>
            <kbd class="opacity-65 text-[0.625rem] ml-2.5 max-[900px]:hidden">{{ shortcut }} ↵</kbd>
          </Button>
        </form>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem v-if="session.busy" data-testid="ctx-cancel" @select="cancel()">
          Cancel request
          <ContextMenuShortcut>{{ shortcutLabel(['mod', '.']) }}</ContextMenuShortcut>
        </ContextMenuItem>
        <ContextMenuItem
          v-else
          data-testid="ctx-send"
          :disabled="!prepared.request"
          @select="primary()">
          Send
          <ContextMenuShortcut>{{ shortcutLabel(['mod', 'enter']) }}</ContextMenuShortcut>
        </ContextMenuItem>
        <ContextMenuItem data-testid="ctx-focus-url" @select="focusUrl()">
          Focus URL
          <ContextMenuShortcut>{{ shortcutLabel(['mod', 'l']) }}</ContextMenuShortcut>
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem
          data-testid="ctx-copy-url"
          :disabled="!session.draft.url"
          @select="copy(session.draft.url)">
          Copy URL
        </ContextMenuItem>
        <ContextMenuItem data-testid="ctx-copy-curl-bar" :disabled="!curl" @select="copy(curl)">
          Copy as cURL
        </ContextMenuItem>
        <ContextMenuSub>
          <ContextMenuSubTrigger data-testid="ctx-copy-as" :disabled="!prepared.request">
            Copy as
          </ContextMenuSubTrigger>
          <ContextMenuSubContent>
            <ContextMenuItem
              v-for="option in codeTargets"
              :key="option.id"
              :data-testid="`ctx-copy-as-${option.id}`"
              @select="copy(codeFor(option.id))">
              {{ option.label }}
            </ContextMenuItem>
          </ContextMenuSubContent>
        </ContextMenuSub>
        <ContextMenuCheckboxItem v-model="showCurl" data-testid="ctx-show-curl">
          Show code
        </ContextMenuCheckboxItem>
      </ContextMenuContent>
    </ContextMenu>
    <div
      v-if="confirming && environment"
      class="flex items-center gap-3 border-b border-border px-3.5 py-2 font-mono text-[0.6875rem]"
      role="alert"
      data-environment-confirm>
      <span class="min-w-0 flex-1">
        <span class="font-semibold" :style="{ color: `var(--${environment.color})` }">
          {{ environment.name }}
        </span>
        is protected. {{ websocket ? 'Connect' : 'Send' }} to {{ environment.name }}?
      </span>
      <Button variant="ghost" @click="confirming = false">Cancel</Button>
      <Button data-confirm-environment @click="confirmAndSend">
        {{ websocket ? 'Connect' : 'Send' }}
      </Button>
    </div>
    <p
      v-if="importError"
      data-import-error
      class="px-3.5 py-2 text-destructive border-b border-border font-mono text-[0.6875rem] leading-[1.6]"
      role="alert">
      {{ importError }}
    </p>
    <p
      v-else-if="importNotice"
      data-import-notice
      class="flex items-center gap-2 px-3.5 py-2 text-muted-foreground border-b border-border font-mono text-[0.6875rem] leading-[1.6]"
      role="status">
      <span class="min-w-0 flex-1">{{ importNotice }}</span>
      <button
        type="button"
        class="hover:text-foreground"
        aria-label="Dismiss import notice"
        @click="importNotice = ''">
        <X :size="12" aria-hidden="true" />
      </button>
    </p>
    <p
      v-if="session.draft.url && validationError"
      :id="`${prefix}-validation`"
      data-request-validation
      class="px-3.5 py-2 text-destructive border-b border-border font-mono text-[0.6875rem] leading-[1.6]"
      role="status">
      {{ validationError }}
    </p>
    <ContextMenu>
      <ContextMenuTrigger as-child>
        <section
          v-if="showCurl"
          :id="`${prefix}-curl`"
          data-curl-preview
          class="max-h-50 overflow-auto px-3.5 pt-2 pb-3 border-b border-border bg-muted"
          aria-label="Request as code">
          <div class="flex items-center gap-2">
            <div
              class="flex min-w-0 flex-1 flex-wrap items-center gap-0.5"
              role="group"
              aria-label="Code language">
              <button
                v-for="option in codeTargets"
                :key="option.id"
                type="button"
                :data-code-target="option.id"
                :aria-pressed="option.id === target"
                class="h-6 rounded px-2 text-[0.6875rem] text-muted-foreground hover:bg-accent hover:text-foreground aria-pressed:bg-accent aria-pressed:text-foreground"
                @click="emit('update:codeTarget', option.id)">
                {{ option.label }}
              </button>
            </div>
            <Button variant="ghost" data-copy-code @click="copy(code)">
              <Check v-if="copied" :size="13" aria-hidden="true" />
              {{ copied ? 'Copied' : 'Copy' }}
            </Button>
          </div>
          <pre
            class="mt-1 font-mono text-[0.6875rem] leading-[1.8] whitespace-pre-wrap break-anywhere"
            tabindex="0"
            :aria-label="codeTargetLabel(target)"
            :data-language="codeLanguage(target)"
            data-code-snippet
            v-html="codeHtml" />
          <HelpTooltip text="The generated code includes credentials. Review it before sharing.">
            <button
              type="button"
              class="mt-1 font-mono text-[0.5625rem] tracking-[0.08em] text-warning underline decoration-dotted underline-offset-3">
              INCLUDES CREDENTIALS
            </button>
          </HelpTooltip>
          <p v-if="copyError" role="alert" class="text-destructive text-[0.6875rem]">
            {{ copyError }}
          </p>
        </section>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem data-testid="ctx-copy-curl" @select="copy(code)">
          Copy {{ codeTargetLabel(target) }}
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem data-testid="ctx-close-curl" @select="showCurl = false">
          Close
          <ContextMenuShortcut>{{ shortcutLabel(['esc']) }}</ContextMenuShortcut>
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
    <div
      ref="panels"
      class="panels grid min-h-0 flex-1"
      :class="{ resizing, stacked }"
      :data-layout="stacked ? 'vertical' : 'horizontal'"
      :style="panelStyle">
      <RequestEditor
        v-model="session.draft"
        v-model:tab="session.view.requestTab"
        :busy="session.busy"
        :effective-auth="effectiveAuth"
        :inherited-source="inheritedSource"
        :ctx="resolvedCtx"
        :transport="transport" />
      <div
        class="panel-resize relative z-1 -outline-offset-2 max-[900px]:hidden"
        :class="stacked ? '-my-0.75 cursor-row-resize' : '-mx-0.75 cursor-col-resize'"
        data-panel-resize
        role="separator"
        aria-label="Resize panels"
        :aria-orientation="stacked ? 'horizontal' : 'vertical'"
        :aria-valuemin="resizeRange.min"
        :aria-valuemax="resizeRange.max"
        :aria-valuenow="resizeRange.now"
        tabindex="0"
        @pointerdown="startResize"
        @keydown="resizeWithKeyboard" />
      <WebSocketPanel
        v-if="websocket"
        v-model:message="session.draft.body"
        :socket="session.socket"
        @send="sendSocketMessage"
        @clear="socket.clear()" />
      <ResponsePanel
        v-else
        ref="responsePanel"
        v-model:view="session.view"
        :active="active"
        :response="session.response"
        :busy="session.busy"
        :error="session.error"
        :elapsed="session.elapsed"
        :stale="stale"
        :request-url="sentUrl || session.draft.url"
        :timeout-seconds="transport?.timeoutSeconds"
        :history="session.history"
        :stream="session.stream"
        :test-results="session.testResults"
        :capture-errors="session.captureErrors"
        :has-checks="Boolean(session.draft.assertions?.length)"
        @recheck="recheck"
        @clear-history="session.history = []" />
    </div>
  </section>
</template>

<style scoped>
/* panels grid: desktop 3-col with resize handle, mobile single-col */
.panels {
  grid-template-columns: minmax(280px, var(--request-panel-width)) 8px minmax(340px, 1fr);
}
/* first child border — border-r only on desktop */
.panels > :first-child {
  border-right: 1px solid var(--border);
}
@media (max-width: 900px) {
  .panels {
    grid-template-columns: minmax(0, 1fr);
    overflow-y: auto;
  }
  .panels > :first-child {
    border-right: 0;
    border-bottom: 1px solid var(--border);
    min-height: 300px;
    max-height: 480px;
  }
  .panels > :last-child {
    min-height: 360px;
    max-height: 700px;
  }
}
/* resize handle pseudo-element line — cannot be expressed as a utility */
.panel-resize::after {
  position: absolute;
  top: 0;
  bottom: 0;
  left: 3px;
  width: 1px;
  background: var(--border);
  content: '';
}
.panel-resize:hover::after,
.panel-resize:focus-visible::after,
.panels.resizing .panel-resize::after {
  width: 2px;
  background: var(--primary);
}
.panels.resizing {
  user-select: none;
}
/* Stacked: request above response, the handle between them. Narrow
   windows already stack without a handle. */
@media (min-width: 901px) {
  .panels.stacked {
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows:
      minmax(160px, var(--request-panel-height)) 8px
      minmax(200px, 1fr);
  }
  .panels.stacked > :first-child {
    border-right: 0;
    border-bottom: 1px solid var(--border);
  }
  .panels.stacked .panel-resize::after {
    top: 3px;
    bottom: auto;
    left: 0;
    right: 0;
    width: auto;
    height: 1px;
  }
  .panels.stacked .panel-resize:hover::after,
  .panels.stacked .panel-resize:focus-visible::after,
  .panels.stacked.resizing .panel-resize::after {
    width: auto;
    height: 2px;
  }
}
</style>
