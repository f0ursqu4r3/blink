<script setup lang="ts">
import { computed, watch, useId } from "vue";
import { createView, type RequestView } from "@/lib/session";
import { TabsRoot, TabsList, TabsTrigger, TabsContent } from "reka-ui";
import {
  Check,
  Copy,
  Crosshair,
  AlertTriangle,
  WrapText,
} from "lucide-vue-next";
import { Button } from "@/components/ui/button";
import { formatBytes, type ApiResponse } from "@/lib/request";
import { useClipboard } from "@/composables/useClipboard";
import { formatJson, JSON_HIGHLIGHT_LIMIT } from "@/lib/json";
import CodeView from "./CodeView.vue";
const props = defineProps<{
  response: ApiResponse | null;
  busy: boolean;
  error: string;
  elapsed: number;
  stale?: boolean;
  active?: boolean;
}>();
const headingId = useId();
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
const parsed = computed(() => {
  if (!props.response) return null;
  try {
    return { text: formatJson(props.response.body) };
  } catch {
    return null;
  }
});
const text = computed(() =>
  pretty.value && parsed.value
    ? parsed.value.text
    : (props.response?.body ?? ""),
);
const contentType = computed(
  () =>
    props.response?.headers
      .find((header) => header.key.toLowerCase() === "content-type")
      ?.value.split(";")[0] ?? "No Content-Type",
);
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
  () => props.response,
  () => {
    tab.value = "body";
    view.value.responseScroll = 0;
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
</script>

<template>
  <section
    class="response-panel"
    :aria-labelledby="headingId"
    :aria-busy="busy"
  >
    <header class="panel-heading">
      <h2 :id="headingId"><span>02</span> Response</h2>
      <span class="state-label" role="status">{{
        busy
          ? "RECEIVING"
          : error
            ? "FAILED"
            : response
              ? "RECEIVED"
              : "STANDBY"
      }}</span>
    </header>
    <template v-if="response && !busy && !error">
      <p v-if="stale" class="stale-response" role="status">
        Previous response · request edited since send
      </p>
      <div class="response-metrics">
        <span data-response-status :data-tone="tone" class="status"
          ><span class="status-dot" />{{ response.status }}
          {{ response.statusText }}</span
        >
        <span>{{ response.durationMs }}<small> ms</small></span>
        <span>{{ formatBytes(response.sizeBytes) }}</span>
      </div>
      <TabsRoot v-model="tab" class="response-tabs">
        <div class="response-toolbar">
          <TabsList class="tab-list" aria-label="Response view">
            <TabsTrigger value="body" class="tab-trigger">Body</TabsTrigger>
            <TabsTrigger
              value="headers"
              class="tab-trigger"
              data-response-headers
              >Headers <span>{{ response.headers.length }}</span></TabsTrigger
            >
          </TabsList>
          <div class="response-actions">
            <Button
              v-if="tab === 'body' && parsed"
              variant="ghost"
              :aria-pressed="pretty"
              @click="pretty = !pretty"
              >{{ pretty ? "Pretty" : "Raw" }}</Button
            >
            <Button
              v-if="tab === 'body'"
              variant="ghost"
              class="size-7 shrink-0 p-0"
              :aria-pressed="wrap"
              aria-label="Wrap lines"
              title="Wrap lines"
              @click="wrap = !wrap"
              ><WrapText :size="14" aria-hidden="true"
            /></Button>
            <Button
              variant="ghost"
              class="size-7 shrink-0 p-0"
              :aria-label="copied ? 'Copied response' : 'Copy response'"
              title="Copy response"
              @click="copyResult"
              ><Check v-if="copied" :size="14" aria-hidden="true" /><Copy
                v-else
                :size="14"
                aria-hidden="true"
            /></Button>
          </div>
        </div>
        <p v-if="copyError" role="alert" class="copy-error">{{ copyError }}</p>
        <TabsContent value="body" class="body-content">
          <CodeView
            v-model:scroll="view.responseScroll"
            :active="active !== false && tab === 'body'"
            v-if="response.body"
            :text="text"
            :json="Boolean(parsed) && pretty"
            :wrap="wrap"
          />
          <p v-else class="empty-body">Empty response body.</p>
        </TabsContent>
        <TabsContent value="headers" class="headers-content">
          <table aria-label="Response headers">
            <thead>
              <tr>
                <th scope="col">Name</th>
                <th scope="col">Value</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="(header, index) in response.headers" :key="index">
                <td>{{ header.key }}</td>
                <td>{{ header.value }}</td>
              </tr>
            </tbody>
          </table>
          <p v-if="!response.headers.length" class="empty-body">
            No visible response headers.
          </p>
        </TabsContent>
      </TabsRoot>
      <footer class="panel-footer">
        <span class="truncate">{{ contentType }}</span
        ><span
          v-if="
            tab === 'body' &&
            parsed &&
            pretty &&
            text.length > JSON_HIGHLIGHT_LIMIT
          "
          title="Syntax highlighting is disabled above 64,000 characters. Pretty formatting remains active."
          >HIGHLIGHT OFF · LARGE BODY</span
        ><span v-else>UTF-8 VIEW</span>
      </footer>
    </template>
    <div v-else-if="error" class="error-state" role="alert">
      <AlertTriangle :size="22" aria-hidden="true" />
      <h3>REQUEST FAILED</h3>
      <p>{{ error }}</p>
      <span>Edit the request, then send again.</span>
    </div>
    <div v-else-if="busy" class="waiting-state">
      <div class="receiving-bars" aria-hidden="true">
        <i /><i /><i /><i /><i />
      </div>
      <h3>AWAITING RESPONSE</h3>
      <p>{{ (elapsed / 1000).toFixed(1) }} s elapsed · 30 s timeout</p>
    </div>
    <div v-else class="waiting-state">
      <Crosshair :size="32" :stroke-width="1" aria-hidden="true" />
      <h3>AWAITING REQUEST</h3>
      <p>Enter an endpoint. Send. Inspect.</p>
      <span class="empty-protocol">HTTP / HTTPS</span>
    </div>
  </section>
</template>

<style scoped>
.response-panel {
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  background: var(--background);
}
.stale-response {
  padding: 6px 14px;
  border-bottom: 1px solid var(--border);
  color: var(--primary);
  font: 0.625rem var(--font-mono);
}
.panel-heading {
  height: 36px;
  flex-shrink: 0;
  padding: 0 16px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  border-bottom: 1px solid var(--border);
  background: var(--muted);
  font: 0.625rem var(--font-mono);
  letter-spacing: 0.12em;
}
h2 {
  font-weight: 600;
  font-size: 0.6875rem;
  text-transform: uppercase;
}
h2 span {
  color: var(--primary);
  margin-right: 10px;
}
.state-label {
  color: var(--muted-foreground);
}
.response-metrics {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  min-height: 44px;
  gap: 16px;
  padding: 8px 16px;
  border-bottom: 1px solid var(--border);
  font: 0.6875rem var(--font-mono);
  font-variant-numeric: tabular-nums;
}
.response-metrics > span + span {
  border-left: 1px solid var(--border);
  padding-left: 16px;
}
.response-metrics small {
  color: var(--muted-foreground);
  font-size: inherit;
}
.status {
  display: inline-flex;
  align-items: center;
  gap: 8px;
}
.status-dot {
  height: 5px;
  width: 5px;
  background: currentColor;
}
[data-tone="success"] {
  color: var(--success);
}
[data-tone="redirect"] {
  color: var(--primary);
}
[data-tone="error"] {
  color: var(--destructive);
}
.response-tabs {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.response-toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  border-bottom: 1px solid var(--border);
  padding: 0 8px;
  gap: 4px;
  flex-shrink: 0;
}
.tab-list,
.response-actions {
  display: flex;
  align-items: center;
}
.tab-trigger {
  height: 38px;
  padding: 0 10px;
  border-bottom: 1px solid transparent;
  font-size: 0.75rem;
  color: var(--muted-foreground);
  white-space: nowrap;
}
.tab-trigger span {
  margin-left: 4px;
  font: 0.625rem var(--font-mono);
}
.tab-trigger[data-state="active"] {
  color: var(--primary);
  border-bottom-color: var(--primary);
}
.tab-trigger:hover {
  background: var(--muted);
  color: var(--foreground);
}
.body-content[data-state="active"] {
  display: flex;
  flex-direction: column;
}
.body-content,
.headers-content {
  flex: 1;
  min-height: 0;
  overflow: auto;
}
.headers-content table {
  width: 100%;
  border-collapse: collapse;
  font: 0.6875rem/1.7 var(--font-mono);
  table-layout: fixed;
}
th,
td {
  text-align: left;
  padding: 8px 16px;
  border-bottom: 1px solid var(--border);
  overflow-wrap: anywhere;
  vertical-align: top;
}
th {
  color: var(--muted-foreground);
  font-size: 0.625rem;
  text-transform: uppercase;
  font-weight: 400;
}
th:first-child {
  width: 38%;
}
td:first-child {
  color: var(--primary);
}
.panel-footer {
  min-height: 28px;
  padding: 0 16px;
  border-top: 1px solid var(--border);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  font: 0.5625rem var(--font-mono);
  color: var(--muted-foreground);
}
.panel-footer > span:last-child {
  flex-shrink: 0;
  letter-spacing: 0.12em;
}
.waiting-state,
.error-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 32px;
  min-height: 220px;
  color: var(--muted-foreground);
  text-align: center;
}
.waiting-state > svg {
  color: var(--input);
  margin-bottom: 24px;
}
h3 {
  font: 0.6875rem var(--font-mono);
  letter-spacing: 0.14em;
  color: var(--foreground);
}
.waiting-state p {
  margin-top: 10px;
  font-size: 0.75rem;
}
.empty-protocol {
  margin-top: 28px;
  font: 0.5625rem var(--font-mono);
  letter-spacing: 0.15em;
}
.error-state {
  align-items: flex-start;
  justify-content: flex-start;
  text-align: left;
  color: var(--destructive);
  gap: 16px;
}
.error-state p {
  font: 0.75rem/1.8 var(--font-mono);
  overflow-wrap: anywhere;
}
.error-state span {
  color: var(--muted-foreground);
  font-size: 0.75rem;
}
.empty-body,
.copy-error {
  padding: 16px;
  color: var(--muted-foreground);
  font-size: 0.75rem;
}
.copy-error {
  color: var(--destructive);
}
.receiving-bars {
  display: flex;
  gap: 4px;
  margin-bottom: 24px;
}
.receiving-bars i {
  width: 5px;
  height: 14px;
  background: var(--primary);
  animation: receive 1s ease-in-out infinite alternate;
}
.receiving-bars i:nth-child(2n) {
  animation-delay: 0.2s;
}
.receiving-bars i:nth-child(3n) {
  animation-delay: 0.4s;
}
@keyframes receive {
  from {
    opacity: 0.2;
  }
  to {
    opacity: 1;
  }
}
@media (pointer: coarse) {
  .tab-trigger {
    min-height: 44px;
  }
}
</style>
