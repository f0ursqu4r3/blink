<script setup lang="ts">
import {
  computed,
  nextTick,
  onMounted,
  onUnmounted,
  ref,
  useId,
  watch,
} from "vue";
import { createView, type RequestView } from "@/lib/session";
import { TabsRoot, TabsList, TabsTrigger, TabsContent } from "reka-ui";
import {
  Check,
  Copy,
  AlertTriangle,
  Search,
  WrapText,
  X,
  Download,
  ChevronUp,
  ChevronDown,
  ListFilter,
  History,
} from "lucide-vue-next";
import { Button } from "@/components/ui/button";
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuCheckboxItem,
  ContextMenuShortcut,
} from "@/components/ui/context-menu";
import { shortcutLabel } from "@/lib/shortcut";
import { formatBytes, type ApiResponse, type Header } from "@/lib/request";
import { canSaveResponse, saveResponse } from "@/lib/response-body";
import { useClipboard } from "@/composables/useClipboard";
import { formatJson } from "@/lib/json";
import { runJq } from "@/lib/jq";
import { responseLanguage } from "@/lib/response-content";
import CodeView from "./CodeView.vue";
import JsonTreeView from "./JsonTreeView.vue";
import HelpTooltip from "./HelpTooltip.vue";
import TimingCard from "./TimingCard.vue";
import HistoryView from "./HistoryView.vue";
import EventList from "./EventList.vue";
import { isEventStream, parseSse } from "@/lib/sse";
import type { LiveStream } from "@/lib/session";
import type { HistoryEntry } from "@/lib/history";
import type { AssertionResult } from "@/lib/checks";
const props = withDefaults(
  defineProps<{
    response: ApiResponse | null;
    busy: boolean;
    error: string;
    elapsed: number;
    stale?: boolean;
    active?: boolean;
    requestUrl?: string;
    timeoutSeconds?: number;
    history?: HistoryEntry[];
    testResults?: AssertionResult[];
    captureErrors?: string[];
    /** The request has assertions, so the Tests tab shows. */
    hasChecks?: boolean;
    /** An event stream arriving now. */
    stream?: LiveStream;
  }>(),
  { timeoutSeconds: 30 },
);
const emit = defineEmits<{ clearHistory: []; recheck: [] }>();
const passed = computed(
  () => props.testResults?.filter((result) => result.pass).length ?? 0,
);
const showTests = computed(
  () =>
    Boolean(props.hasChecks || props.testResults?.length) ||
    Boolean(props.captureErrors?.length),
);
const headingId = useId();
const historyOpen = ref(false);
const view = defineModel<RequestView>("view", { default: createView });
const tab = computed({
  get: () => view.value.responseTab,
  set: (responseTab) => {
    view.value = { ...view.value, responseTab };
  },
});
const pretty = computed({
  get: () => view.value.pretty,
  set: (pretty) => {
    view.value = { ...view.value, pretty };
  },
});
const wrap = computed({
  get: () => view.value.wrap,
  set: (wrap) => {
    view.value = { ...view.value, wrap };
  },
});
const { copied, copyError, copy } = useClipboard();
const search = ref("");
/** Hide lines that do not match, instead of highlighting matches. */
const filterLines = ref(false);
const bodyView = ref<
  InstanceType<typeof CodeView> | InstanceType<typeof JsonTreeView>
>();
const findCount = computed(() => bodyView.value?.findCount ?? 0);
const findStatus = computed(() => {
  if (!search.value.trim() || filterLines.value) return "";
  return findCount.value
    ? `${(bodyView.value?.findCurrent ?? 0) + 1} of ${findCount.value}`
    : "No results";
});
const findStep = (direction: 1 | -1) => bodyView.value?.findStep(direction);
const eventStream = computed(() => {
  const type = props.response?.headers.find(
    (header) => header.key.toLowerCase() === "content-type",
  )?.value;
  return isEventStream(type);
});
const events = computed(() =>
  eventStream.value && props.response ? parseSse(props.response.body) : [],
);
function onSearchKey(event: KeyboardEvent) {
  if (event.key !== "Enter" || filterLines.value) return;
  event.preventDefault();
  findStep(event.shiftKey ? -1 : 1);
}
const inspectorVisible = ref(false);
const searchInput = ref<HTMLInputElement | null>(null);
const jqQuery = ref("");
const jqOutput = ref<string | null>(null);
const jqError = ref("");
// A truncated or binary preview cannot be parsed as a whole document.
const limited = computed(() =>
  Boolean(props.response?.truncated || props.response?.binary),
);
const unavailable = "Unavailable for truncated responses";
const sourceParsed = computed(() => {
  if (limited.value) return null;
  if (!props.response) return null;
  try {
    return { text: formatJson(props.response.body) };
  } catch {
    return null;
  }
});
const parsed = computed(() => {
  if (limited.value) return null;
  const body = jqOutput.value ?? props.response?.body;
  if (body === undefined) return null;
  try {
    return { text: formatJson(body) };
  } catch {
    return null;
  }
});
const savable = computed(() =>
  props.response ? canSaveResponse(props.response) : false,
);
// A truncated preview whose body did not survive a restart: nothing left to
// show or save until the request is sent again.
const restoredTruncated = computed(() =>
  Boolean(
    props.response?.truncated && !props.response.binary && !props.response.body,
  ),
);
// UTF-8 size of the preview, shown in the truncated bar.
const previewSize = computed(() =>
  props.response?.truncated ? new Blob([props.response.body]).size : 0,
);
const saving = ref(false);
const saveError = ref("");
async function saveBody() {
  if (!props.response || !savable.value || saving.value) return;
  saving.value = true;
  saveError.value = "";
  try {
    await saveResponse(props.response, props.requestUrl ?? "");
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error);
    saveError.value = `Cannot save the file: ${reason.replace(/\.$/, "")}.`;
  } finally {
    saving.value = false;
  }
}
const redirectLabel = computed(() => {
  const count = props.response?.redirectCount;
  return count === undefined
    ? "redirected"
    : `${count} ${count === 1 ? "redirect" : "redirects"}`;
});
const text = computed(() =>
  pretty.value && parsed.value
    ? parsed.value.text
    : (jqOutput.value ?? props.response?.body ?? ""),
);
const contentType = computed(
  () =>
    props.response?.headers
      .find((header) => header.key.toLowerCase() === "content-type")
      ?.value.split(";")[0] ?? "No Content-Type",
);
const language = computed(() =>
  parsed.value ? "json" : responseLanguage(contentType.value),
);
const showJsonTree = computed(() => pretty.value && Boolean(parsed.value));
const filteredHeaders = computed(() => {
  const query = search.value.trim().toLocaleLowerCase();
  if (!query) return props.response?.headers ?? [];
  return (props.response?.headers ?? []).filter(({ key, value }) =>
    `${key}: ${value}`.toLocaleLowerCase().includes(query),
  );
});
const tone = computed(() =>
  !props.response
    ? ""
    : props.response.status >= 400
      ? "error"
      : props.response.status >= 300
        ? "redirect"
        : "success",
);
watch(
  () => props.busy,
  (busy) => {
    if (busy) historyOpen.value = false;
  },
);
watch(
  () => props.response,
  () => {
    // Stay on Tests across sends, so a rerun shows its results.
    if (tab.value !== "tests" || !showTests.value)
      tab.value = eventStream.value ? "events" : "body";
    view.value.responseScroll = 0;
    search.value = "";
    inspectorVisible.value = false;
    jqQuery.value = "";
    jqOutput.value = null;
    jqError.value = "";
    saveError.value = "";
  },
);
function copyResult() {
  void copy(
    tab.value === "headers"
      ? (props.response?.headers
          .map(({ key, value }) => `${key}: ${value}`)
          .join("\n") ?? "")
      : text.value,
  );
}
async function executeJq() {
  if (!props.response || !jqQuery.value.trim()) return;
  jqError.value = "";
  try {
    jqOutput.value = await runJq(props.response.body, jqQuery.value);
    view.value.responseScroll = 0;
  } catch (error) {
    jqError.value = error instanceof Error ? error.message : "jq query failed.";
  }
}
function toggleInspector() {
  inspectorVisible.value = !inspectorVisible.value;
  if (inspectorVisible.value) void nextTick(() => searchInput.value?.focus());
  else search.value = "";
}
function onKey(event: KeyboardEvent) {
  if (
    props.active === false ||
    event.defaultPrevented ||
    event.isComposing ||
    event.repeat ||
    document.querySelector('[role="dialog"], [data-surface="context-menu"]')
  )
    return;
  if (
    (event.metaKey || event.ctrlKey) &&
    !event.shiftKey &&
    !event.altKey &&
    event.key.toLowerCase() === "f" &&
    tab.value === "body" &&
    !props.response?.binary
  ) {
    event.preventDefault();
    if (!inspectorVisible.value) toggleInspector();
    else void nextTick(() => searchInput.value?.focus());
  } else if (event.key === "Escape" && inspectorVisible.value) {
    event.preventDefault();
    toggleInspector();
  }
}
onMounted(() => window.addEventListener("keydown", onKey));
onUnmounted(() => window.removeEventListener("keydown", onKey));
/** Open find on the body tab. */
function find() {
  if (!props.response || props.response.binary) return;
  tab.value = "body";
  if (!inspectorVisible.value) toggleInspector();
  else void nextTick(() => searchInput.value?.focus());
}
defineExpose({
  find,
  copyResult,
  saveBody,
  toggleWrap: () => (wrap.value = !wrap.value),
  toggleHistory: () => (historyOpen.value = !historyOpen.value),
  togglePretty: () => (pretty.value = !pretty.value),
});

// ── Header row context menu ────────────────────────────────────────────────
const contextHeader = ref<Header | null>(null);

function openHeaderCtx(header: Header) {
  contextHeader.value = header;
}

function copyHeaderName() {
  if (!contextHeader.value) return;
  void copy(contextHeader.value.key);
}

function copyHeaderValue() {
  if (!contextHeader.value) return;
  void copy(contextHeader.value.value);
}

function copyHeaderPair() {
  if (!contextHeader.value) return;
  void copy(`${contextHeader.value.key}: ${contextHeader.value.value}`);
}
</script>

<template>
  <section
    class="min-w-0 min-h-0 flex flex-col bg-background"
    :aria-labelledby="headingId"
    :aria-busy="busy"
  >
    <header
      class="h-9 shrink-0 px-4 flex items-center border-b border-border bg-muted font-mono text-[0.625rem] tracking-[0.12em]"
    >
      <h2 :id="headingId" class="font-semibold text-[0.6875rem] uppercase">
        Response
      </h2>
      <span class="ml-auto text-muted-foreground" role="status">{{
        busy
          ? "RECEIVING"
          : error
            ? "FAILED"
            : response
              ? "RECEIVED"
              : "STANDBY"
      }}</span>
      <Button
        variant="ghost"
        class="-mr-2 ml-2 h-6 gap-1 px-1.5 font-mono text-[0.625rem] tracking-normal"
        data-history-toggle
        :aria-pressed="historyOpen"
        aria-label="History"
        title="Request history"
        @click="historyOpen = !historyOpen"
      >
        <History :size="13" aria-hidden="true" />
        <span v-if="history?.length" class="tabular-nums">{{
          history.length
        }}</span>
      </Button>
    </header>
    <HistoryView
      v-if="historyOpen"
      :history="history ?? []"
      @clear="emit('clearHistory')"
    />
    <template v-else-if="response && !busy && !error">
      <p
        v-if="stale"
        class="py-1.5 px-3.5 border-b border-border text-warning font-mono text-[0.625rem]"
        role="status"
      >
        Previous response · request edited since send
      </p>
      <div
        class="flex items-center flex-wrap min-h-11 gap-4 px-4 py-2 border-b border-border font-mono text-[0.6875rem] tabular-nums"
      >
        <span
          data-response-status
          :data-tone="tone"
          class="inline-flex items-center gap-2 data-[tone=success]:text-success data-[tone=redirect]:text-warning data-[tone=error]:text-destructive"
        >
          <span class="block h-1.25 w-1.25 bg-current" />{{ response.status }}
          {{ response.statusText }}
        </span>
        <span class="border-l border-border pl-4">
          <TimingCard
            :timing="response.timing"
            :duration-ms="response.durationMs"
          >
            {{ response.durationMs }}
            <small class="text-muted-foreground"> ms</small>
          </TimingCard>
        </span>
        <span class="border-l border-border pl-4">
          {{ formatBytes(response.sizeBytes) }}
        </span>
        <span
          v-if="response.finalUrl"
          data-response-redirect
          class="basis-full min-w-0 truncate text-muted-foreground"
          :title="response.finalUrl"
          >→ {{ response.finalUrl }} · {{ redirectLabel }}</span
        >
      </div>
      <TabsRoot v-model="tab" class="flex-1 min-h-0 flex flex-col">
        <ContextMenu>
          <ContextMenuTrigger as-child>
            <div
              class="response-toolbar flex justify-between items-center border-b border-border px-2 gap-1 shrink-0"
            >
              <TabsList class="flex items-center" aria-label="Response view">
                <TabsTrigger
                  value="body"
                  class="h-9.5 px-2.5 border-b border-transparent text-xs text-muted-foreground whitespace-nowrap data-[state=active]:text-foreground data-[state=active]:border-b-primary hover:bg-muted hover:text-foreground pointer-coarse:min-h-11"
                >
                  Body
                </TabsTrigger>
                <TabsTrigger
                  value="headers"
                  class="h-9.5 px-2.5 border-b border-transparent text-xs text-muted-foreground whitespace-nowrap data-[state=active]:text-foreground data-[state=active]:border-b-primary hover:bg-muted hover:text-foreground pointer-coarse:min-h-11"
                  data-response-headers
                >
                  Headers
                  <span class="ml-1 font-mono text-[0.625rem]">
                    {{ response.headers.length }}
                  </span>
                </TabsTrigger>
                <TabsTrigger
                  v-if="eventStream"
                  value="events"
                  class="h-9.5 px-2.5 border-b border-transparent text-xs text-muted-foreground whitespace-nowrap data-[state=active]:text-foreground data-[state=active]:border-b-primary hover:bg-muted hover:text-foreground pointer-coarse:min-h-11"
                  data-response-events
                >
                  Events
                  <span class="ml-1 font-mono text-[0.625rem]">{{
                    events.length
                  }}</span>
                </TabsTrigger>
                <TabsTrigger
                  v-if="showTests"
                  value="tests"
                  class="h-9.5 px-2.5 border-b border-transparent text-xs text-muted-foreground whitespace-nowrap data-[state=active]:text-foreground data-[state=active]:border-b-primary hover:bg-muted hover:text-foreground pointer-coarse:min-h-11"
                  data-response-tests
                >
                  Tests
                  <span
                    v-if="testResults?.length"
                    class="ml-1 font-mono text-[0.625rem]"
                    :class="
                      passed === testResults.length
                        ? 'text-success'
                        : 'text-destructive'
                    "
                    data-tests-summary
                  >
                    {{ passed }}/{{ testResults.length }}
                  </span>
                </TabsTrigger>
              </TabsList>
              <div class="flex items-center">
                <Button
                  v-if="
                    tab === 'body' &&
                    (parsed || response.truncated) &&
                    !response.binary
                  "
                  variant="ghost"
                  :aria-pressed="pretty && !response.truncated"
                  :disabled="response.truncated"
                  :title="response.truncated ? unavailable : undefined"
                  @click="pretty = !pretty"
                >
                  {{ pretty && !response.truncated ? "Pretty" : "Raw" }}
                </Button>
                <Button
                  v-if="tab === 'body' && !response.binary"
                  variant="ghost"
                  class="size-7 shrink-0 p-0"
                  :aria-pressed="wrap"
                  aria-label="Wrap lines"
                  title="Wrap lines"
                  @click="wrap = !wrap"
                >
                  <WrapText :size="14" aria-hidden="true" />
                </Button>
                <Button
                  v-if="tab === 'body' && !response.binary"
                  variant="ghost"
                  class="size-7 shrink-0 p-0"
                  :aria-pressed="inspectorVisible"
                  aria-label="Find response"
                  title="Find and filter response · Cmd/Ctrl+F"
                  @click="toggleInspector"
                >
                  <Search :size="14" aria-hidden="true" />
                </Button>
                <Button
                  variant="ghost"
                  class="size-7 shrink-0 p-0"
                  data-save-response
                  aria-label="Save response body"
                  :title="
                    savable
                      ? 'Save response body…'
                      : 'Body is no longer available. Send the request again.'
                  "
                  :disabled="!savable || saving"
                  @click="saveBody"
                >
                  <Download :size="14" aria-hidden="true" />
                </Button>
                <Button
                  variant="ghost"
                  class="size-7 shrink-0 p-0"
                  :aria-label="copied ? 'Copied response' : 'Copy response'"
                  title="Copy response"
                  @click="copyResult"
                >
                  <Check v-if="copied" :size="14" aria-hidden="true" />
                  <Copy v-else :size="14" aria-hidden="true" />
                </Button>
              </div>
            </div>
          </ContextMenuTrigger>
          <ContextMenuContent>
            <ContextMenuItem
              data-testid="ctx-copy-response"
              @select="copyResult"
            >
              Copy response
            </ContextMenuItem>
            <ContextMenuItem
              data-testid="ctx-save-response"
              :disabled="!savable"
              @select="saveBody"
            >
              Save response body…
            </ContextMenuItem>
            <template v-if="tab === 'body' && !response.binary">
              <ContextMenuSeparator />
              <ContextMenuCheckboxItem
                v-if="parsed"
                v-model="pretty"
                data-testid="ctx-toolbar-pretty"
              >
                Pretty
              </ContextMenuCheckboxItem>
              <ContextMenuCheckboxItem
                v-model="wrap"
                data-testid="ctx-toolbar-wrap"
              >
                Wrap lines
              </ContextMenuCheckboxItem>
              <ContextMenuItem
                data-testid="ctx-toolbar-find"
                @select="toggleInspector"
              >
                Find
                <ContextMenuShortcut>{{
                  shortcutLabel(["mod", "f"])
                }}</ContextMenuShortcut>
              </ContextMenuItem>
            </template>
          </ContextMenuContent>
        </ContextMenu>
        <div
          v-if="tab === 'body' && inspectorVisible"
          class="flex min-h-9.5 gap-2 px-2 py-1.25 border-b border-border bg-muted max-[680px]:flex-col"
        >
          <label
            class="flex flex-[1_1_180px] min-w-0 items-center gap-1.5 rounded border border-input bg-background pl-2 text-muted-foreground focus-within:border-primary max-[680px]:basis-8.5"
          >
            <Search :size="13" aria-hidden="true" />
            <span class="sr-only">{{
              filterLines ? "Filter response" : "Find in response"
            }}</span>
            <input
              ref="searchInput"
              v-model="search"
              data-response-search
              type="search"
              :placeholder="filterLines ? 'Filter lines' : 'Find'"
              autocomplete="off"
              class="w-full min-w-0 h-6.5 border-0 rounded-none px-1.75 bg-transparent font-mono text-[0.6875rem]"
              @keydown="onSearchKey"
            />
            <span
              v-if="findStatus"
              data-find-status
              role="status"
              class="shrink-0 whitespace-nowrap font-mono text-[0.625rem]"
              :class="{ 'text-destructive': findStatus === 'No results' }"
              >{{ findStatus }}</span
            >
            <template v-if="!filterLines">
              <button
                type="button"
                class="inline-flex size-5.5 shrink-0 items-center justify-center rounded hover:bg-accent hover:text-foreground disabled:opacity-40"
                aria-label="Previous match"
                title="Previous match · Shift+Enter"
                data-find-previous
                :disabled="!findCount"
                @click="findStep(-1)"
              >
                <ChevronUp :size="13" aria-hidden="true" />
              </button>
              <button
                type="button"
                class="inline-flex size-5.5 shrink-0 items-center justify-center rounded hover:bg-accent hover:text-foreground disabled:opacity-40"
                aria-label="Next match"
                title="Next match · Enter"
                data-find-next
                :disabled="!findCount"
                @click="findStep(1)"
              >
                <ChevronDown :size="13" aria-hidden="true" />
              </button>
            </template>
            <button
              type="button"
              class="mr-0.5 inline-flex size-5.5 shrink-0 items-center justify-center rounded hover:bg-accent hover:text-foreground aria-pressed:bg-accent aria-pressed:text-foreground"
              aria-label="Filter lines"
              title="Show only matching lines"
              data-filter-lines
              :aria-pressed="filterLines"
              @click="filterLines = !filterLines"
            >
              <ListFilter :size="13" aria-hidden="true" />
            </button>
          </label>
          <form
            v-if="(sourceParsed || response.truncated) && !response.binary"
            class="flex flex-[1_1_260px] min-w-0 items-center gap-1.5 rounded border border-input bg-background focus-within:border-primary max-[680px]:basis-8.5"
            aria-label="jq query"
            @submit.prevent="executeJq"
          >
            <input
              v-model="jqQuery"
              data-response-jq
              placeholder="jq query, e.g. .items[]"
              spellcheck="false"
              autocomplete="off"
              :disabled="response.truncated"
              :title="response.truncated ? unavailable : undefined"
              class="w-full min-w-0 h-6.5 border-0 rounded-none px-1.75 bg-transparent font-mono text-[0.6875rem]"
            />
            <Button
              type="submit"
              variant="ghost"
              data-run-jq
              class="flex-none whitespace-nowrap"
              :disabled="!jqQuery.trim() || response.truncated"
            >
              Run jq
            </Button>
            <Button
              v-if="jqOutput !== null"
              type="button"
              variant="ghost"
              class="size-7 shrink-0 p-0"
              aria-label="Clear jq result"
              title="Clear jq result"
              @click="jqOutput = null"
            >
              <X :size="14" aria-hidden="true" />
            </Button>
          </form>
        </div>
        <p v-if="copyError" role="alert" class="p-4 text-destructive text-xs">
          {{ copyError }}
        </p>
        <p v-if="jqError" role="alert" class="p-4 text-destructive text-xs">
          {{ jqError }}
        </p>
        <p
          v-if="saveError"
          data-save-error
          role="alert"
          class="p-4 text-destructive text-xs"
        >
          {{ saveError }}
        </p>
        <p
          v-if="tab === 'body' && response.truncated && !response.binary"
          data-response-truncated
          role="status"
          class="flex flex-wrap items-center gap-1 py-1.5 px-3.5 border-b border-border text-muted-foreground font-mono text-[0.625rem]"
        >
          <template v-if="restoredTruncated">
            Preview is not kept after a restart. Send the request again to
            inspect it.
          </template>
          <!-- prettier-ignore -->
          <template v-else>
            Preview shows the first {{ formatBytes(previewSize) }} of {{ formatBytes(response.sizeBytes) }}.
            <button
              type="button"
              class="underline decoration-dotted underline-offset-3 hover:text-foreground disabled:no-underline"
              :disabled="!savable || saving"
              @click="saveBody"
            >
              Save…
            </button>
            to get the full body.
          </template>
        </p>
        <TabsContent
          value="body"
          class="flex-1 min-h-0 overflow-auto data-[state=active]:flex data-[state=active]:flex-col"
        >
          <div
            v-if="response.binary"
            data-response-binary
            class="flex flex-1 flex-col items-center justify-center gap-3 p-8 text-muted-foreground text-xs"
          >
            <!-- prettier-ignore -->
            <p class="font-mono">
              Binary response · {{ formatBytes(response.sizeBytes) }} · {{ contentType }}
            </p>
            <Button
              variant="secondary"
              :disabled="!savable || saving"
              @click="saveBody"
            >
              Save…
            </Button>
          </div>
          <JsonTreeView
            v-else-if="response.body && showJsonTree"
            ref="bodyView"
            v-model:scroll="view.responseScroll"
            :active="active !== false && tab === 'body'"
            :text="text"
            :filter="filterLines ? search : ''"
            :find="filterLines ? '' : search"
            :wrap="wrap"
          />
          <CodeView
            v-else-if="response.body"
            ref="bodyView"
            v-model:scroll="view.responseScroll"
            :active="active !== false && tab === 'body'"
            :text="text"
            :language="language"
            :filter="filterLines ? search : ''"
            :find="filterLines ? '' : search"
            :wrap="wrap"
          />
          <p
            v-else-if="!restoredTruncated"
            class="p-4 text-muted-foreground text-xs"
          >
            Empty response body.
          </p>
        </TabsContent>
        <TabsContent
          value="events"
          class="flex-1 min-h-0 data-[state=active]:flex data-[state=active]:flex-col"
        >
          <EventList :events="events" />
        </TabsContent>
        <TabsContent value="tests" class="flex-1 min-h-0 overflow-auto">
          <div
            class="flex h-8 items-center justify-between border-b border-border px-4 font-mono text-[0.625rem] tracking-[0.1em] text-muted-foreground"
          >
            <span v-if="testResults?.length"
              >{{ passed }} OF {{ testResults.length }} PASSED</span
            >
            <span v-else>NO RESULTS</span>
            <Button
              variant="ghost"
              class="h-6 px-2 font-sans tracking-normal"
              data-recheck
              @click="emit('recheck')"
            >
              Run again
            </Button>
          </div>
          <ul role="list" data-test-results>
            <li
              v-for="result in testResults"
              :key="result.id"
              class="flex items-start gap-2.5 border-b border-border px-4 py-2 font-mono text-[0.6875rem]"
              :data-test-result="result.pass ? 'pass' : 'fail'"
            >
              <span
                class="w-8 shrink-0 font-semibold"
                :class="result.pass ? 'text-success' : 'text-destructive'"
                >{{ result.pass ? "PASS" : "FAIL" }}</span
              >
              <span class="min-w-0 flex-1">
                <span class="block wrap-anywhere">{{
                  result.description
                }}</span>
                <span
                  v-if="!result.pass"
                  class="block text-muted-foreground wrap-anywhere"
                  >Actual: {{ result.actual }}</span
                >
              </span>
            </li>
          </ul>
          <p
            v-if="!testResults?.length"
            class="p-4 text-xs text-muted-foreground"
          >
            Add assertions in the request Tests tab, then send or run again.
          </p>
          <div
            v-if="captureErrors?.length"
            role="alert"
            class="border-t border-border px-4 py-2 font-mono text-[0.6875rem] text-warning"
          >
            <p v-for="message in captureErrors" :key="message">
              Capture {{ message }}
            </p>
          </div>
        </TabsContent>
        <TabsContent value="headers" class="flex-1 min-h-0 overflow-auto">
          <ContextMenu>
            <ContextMenuTrigger as-child>
              <table
                aria-label="Response headers"
                class="w-full border-collapse font-mono text-[0.6875rem] leading-[1.7] table-fixed"
              >
                <thead>
                  <tr>
                    <th
                      scope="col"
                      class="text-left px-4 py-2 border-b border-border wrap-anywhere align-top text-muted-foreground text-[0.625rem] uppercase font-normal w-[38%]"
                    >
                      Name
                    </th>
                    <th
                      scope="col"
                      class="text-left px-4 py-2 border-b border-border wrap-anywhere align-top text-muted-foreground text-[0.625rem] uppercase font-normal"
                    >
                      Value
                    </th>
                  </tr>
                </thead>
                <tbody>
                  <tr
                    v-for="(header, index) in filteredHeaders"
                    :key="index"
                    @contextmenu="openHeaderCtx(header)"
                  >
                    <td
                      class="text-left px-4 py-2 border-b border-border wrap-anywhere align-top text-info"
                    >
                      {{ header.key }}
                    </td>
                    <td
                      class="text-left px-4 py-2 border-b border-border wrap-anywhere align-top"
                    >
                      {{ header.value }}
                    </td>
                  </tr>
                </tbody>
              </table>
            </ContextMenuTrigger>
            <ContextMenuContent>
              <ContextMenuItem
                data-testid="ctx-header-copy-name"
                @select="copyHeaderName"
              >
                Copy name
              </ContextMenuItem>
              <ContextMenuItem
                data-testid="ctx-header-copy-value"
                @select="copyHeaderValue"
              >
                Copy value
              </ContextMenuItem>
              <ContextMenuItem
                data-testid="ctx-header-copy-pair"
                @select="copyHeaderPair"
              >
                Copy name: value
              </ContextMenuItem>
            </ContextMenuContent>
          </ContextMenu>
          <p
            v-if="!filteredHeaders.length"
            class="p-4 text-muted-foreground text-xs"
          >
            No response headers match this filter.
          </p>
        </TabsContent>
      </TabsRoot>
      <footer
        class="min-h-7 px-4 border-t border-border flex items-center justify-between gap-3 font-mono text-[0.5625rem] text-muted-foreground"
      >
        <span class="truncate">{{ contentType }}</span>
      </footer>
    </template>
    <div
      v-else-if="error"
      class="flex-1 flex flex-col items-start justify-start p-8 min-h-55 text-destructive text-left gap-4"
      role="alert"
    >
      <AlertTriangle :size="22" aria-hidden="true" />
      <h3 class="font-mono text-[0.6875rem] tracking-[0.14em] text-foreground">
        REQUEST FAILED
      </h3>
      <p class="font-mono text-xs leading-[1.8] wrap-anywhere">
        {{ error }}
      </p>
    </div>
    <div
      v-else-if="busy && stream"
      class="flex min-h-0 flex-1 flex-col"
      data-live-stream
    >
      <div
        class="flex min-h-11 items-center gap-4 border-b border-border px-4 py-2 font-mono text-[0.6875rem] tabular-nums"
      >
        <span
          class="inline-flex items-center gap-2"
          :class="stream.status >= 400 ? 'text-destructive' : 'text-success'"
        >
          <span
            class="block h-1.25 w-1.25 bg-current animate-[receive_1s_ease-in-out_infinite_alternate]"
          />{{ stream.status }} {{ stream.statusText }}
        </span>
        <span class="border-l border-border pl-4"
          >{{ stream.events.length }}
          {{ stream.events.length === 1 ? "event" : "events" }}</span
        >
        <span class="border-l border-border pl-4">{{
          formatBytes(stream.bytes)
        }}</span>
        <span class="ml-auto text-muted-foreground"
          >STREAMING · {{ (elapsed / 1000).toFixed(1) }} s</span
        >
      </div>
      <EventList :events="stream.events" live />
      <footer
        class="min-h-7 px-4 border-t border-border flex items-center font-mono text-[0.5625rem] text-muted-foreground"
      >
        Cancel stops the stream and keeps the events.
      </footer>
    </div>
    <div
      v-else-if="busy"
      class="flex-1 flex flex-col items-center justify-center p-8 min-h-55 text-muted-foreground text-center"
    >
      <div class="flex gap-1 mb-6" aria-hidden="true">
        <i
          class="w-1.25 h-3.5 bg-primary animate-[receive_1s_ease-in-out_infinite_alternate]"
        />
        <i
          class="w-1.25 h-3.5 bg-primary animate-[receive_1s_ease-in-out_infinite_alternate]"
        />
        <i
          class="w-1.25 h-3.5 bg-primary animate-[receive_1s_ease-in-out_infinite_alternate]"
        />
        <i
          class="w-1.25 h-3.5 bg-primary animate-[receive_1s_ease-in-out_infinite_alternate]"
        />
        <i
          class="w-1.25 h-3.5 bg-primary animate-[receive_1s_ease-in-out_infinite_alternate]"
        />
      </div>
      <h3 class="font-mono text-[0.6875rem] tracking-[0.14em] text-foreground">
        AWAITING RESPONSE
      </h3>
      <p class="mt-2.5 text-xs">
        {{ (elapsed / 1000).toFixed(1) }} s elapsed · {{ timeoutSeconds }} s
        timeout
      </p>
    </div>
    <div
      v-else
      class="flex-1 flex flex-col items-center justify-center p-8 min-h-55 text-muted-foreground text-center"
    >
      <h3 class="font-mono text-[0.6875rem] tracking-[0.14em] text-foreground">
        AWAITING REQUEST
      </h3>
      <HelpTooltip
        text="Browser transport can limit inspection because of browser security rules. Use the desktop native transport for unrestricted inspection."
      >
        <button
          type="button"
          class="mt-2 text-xs underline decoration-dotted underline-offset-3"
        >
          Browser transport help
        </button>
      </HelpTooltip>
    </div>
  </section>
</template>

<style scoped>
/* Animation for receiving-bars */
@keyframes receive {
  from {
    opacity: 0.2;
  }
  to {
    opacity: 1;
  }
}
/* nth-child delays cannot be expressed as Tailwind utilities */
i:nth-child(2n) {
  animation-delay: 0.2s;
}
i:nth-child(3n) {
  animation-delay: 0.4s;
}
</style>
