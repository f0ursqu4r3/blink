<script setup lang="ts">
import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import {
  Braces,
  Check,
  ChevronDown,
  CircleDot,
  Copy,
  FileJson,
  History,
  LockKeyhole,
  MoreHorizontal,
  Plus,
  Send,
  Settings,
  Trash2,
  Zap,
} from "lucide-vue-next";
import { Button } from "@/components/ui/button";

type Header = {
  key: string;
  value: string;
  enabled: boolean;
};

type ApiResponse = {
  status: number;
  statusText: string;
  durationMs: number;
  headers: { key: string; value: string }[];
  body: string;
};

const methods = ["GET", "POST", "PUT", "PATCH", "DELETE"];
const tabs = ["Query", "Headers", "Body", "Auth"] as const;

const method = ref("GET");
const url = ref("https://jsonplaceholder.typicode.com/todos/1");
const activeTab = ref<(typeof tabs)[number]>("Body");
const requestBody = ref("");
const headers = ref<Header[]>([
  { key: "Accept", value: "application/json", enabled: true },
  { key: "Content-Type", value: "application/json", enabled: false },
]);
const response = ref<ApiResponse | null>(null);
const errorMessage = ref("");
const isSending = ref(false);
const copied = ref(false);

const canSend = computed(() => {
  try {
    const target = new URL(url.value);
    return target.protocol === "http:" || target.protocol === "https:";
  } catch {
    return false;
  }
});

const visibleResponse = computed(() => {
  if (!response.value) {
    return "";
  }

  try {
    return JSON.stringify(JSON.parse(response.value.body), null, 2);
  } catch {
    return response.value.body;
  }
});

const enabledHeaders = computed(() =>
  headers.value
    .filter((header) => header.enabled && header.key.trim())
    .map(({ key, value }) => ({ key: key.trim(), value: value.trim() })),
);

function addHeader() {
  headers.value.push({ key: "", value: "", enabled: true });
}

function removeHeader(index: number) {
  headers.value.splice(index, 1);
}

function resetRequest() {
  method.value = "GET";
  url.value = "";
  requestBody.value = "";
  headers.value = [{ key: "Accept", value: "application/json", enabled: true }];
  response.value = null;
  errorMessage.value = "";
  activeTab.value = "Body";
}

async function sendRequest() {
  if (!canSend.value || isSending.value) {
    return;
  }

  isSending.value = true;
  response.value = null;
  errorMessage.value = "";

  try {
    const payload = {
      method: method.value,
      url: url.value,
      headers: enabledHeaders.value,
      body: requestBody.value.trim() || null,
    };

    if ("__TAURI_INTERNALS__" in window) {
      response.value = await invoke<ApiResponse>("send_request", { request: payload });
      return;
    }

    const startedAt = performance.now();
    const browserResponse = await fetch(payload.url, {
      method: payload.method,
      headers: Object.fromEntries(payload.headers.map(({ key, value }) => [key, value])),
      body: payload.body,
    });
    const body = await browserResponse.text();
    response.value = {
      status: browserResponse.status,
      statusText: browserResponse.statusText,
      durationMs: Math.round(performance.now() - startedAt),
      headers: [...browserResponse.headers.entries()].map(([key, value]) => ({ key, value })),
      body,
    };
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : "Blink could not send this request.";
  } finally {
    isSending.value = false;
  }
}

async function copyResponse() {
  if (!visibleResponse.value) {
    return;
  }

  await navigator.clipboard.writeText(visibleResponse.value);
  copied.value = true;
  window.setTimeout(() => {
    copied.value = false;
  }, 1600);
}
</script>

<template>
  <main class="min-h-screen bg-slate-950 text-slate-100">
    <a
      class="absolute -top-12 left-4 z-50 rounded bg-cyan-300 px-3 py-2 text-sm font-semibold text-slate-950 focus:top-4"
      href="#workspace"
    >
      Skip to workspace
    </a>

    <div class="grid min-h-screen grid-cols-[220px_minmax(0,1fr)] lg:grid-cols-[248px_minmax(0,1fr)] max-[820px]:block">
      <aside class="flex min-h-screen flex-col border-r border-slate-800 bg-slate-950 px-3 py-4 max-[820px]:min-h-0 max-[820px]:border-b max-[820px]:border-r-0">
        <div class="flex items-center gap-2 px-2">
          <div class="flex size-8 items-center justify-center rounded-md bg-cyan-400 text-slate-950">
            <Zap :size="17" :stroke-width="2.7" aria-hidden="true" />
          </div>
          <span class="text-sm font-semibold tracking-tight text-slate-50">Blink</span>
          <span class="ml-auto rounded border border-slate-800 px-1.5 py-0.5 font-mono text-[10px] text-slate-500">LOCAL</span>
        </div>

        <Button class="mt-7 w-full justify-start" variant="default" @click="resetRequest">
          <Plus :size="16" aria-hidden="true" />
          New request
        </Button>

        <nav class="mt-5 space-y-1" aria-label="Primary navigation">
          <button class="flex h-9 w-full items-center gap-2 rounded-md bg-slate-800 px-2.5 text-left text-sm font-medium text-slate-100">
            <CircleDot :size="16" class="text-cyan-300" aria-hidden="true" />
            Untitled request
          </button>
          <button class="flex h-9 w-full items-center gap-2 rounded-md px-2.5 text-left text-sm text-slate-400 transition-colors hover:bg-slate-900 hover:text-slate-200">
            <History :size="16" aria-hidden="true" />
            Session activity
          </button>
        </nav>

        <div class="mt-8 px-2">
          <div class="flex items-center justify-between text-[11px] font-semibold uppercase tracking-[0.13em] text-slate-600">
            <span>Workspace</span>
            <MoreHorizontal :size="15" aria-hidden="true" />
          </div>
          <p class="mt-3 text-xs leading-5 text-slate-500">No account. No cloud sync. This session stays on this device.</p>
        </div>

        <div class="mt-auto border-t border-slate-800 px-2 pt-4 max-[820px]:hidden">
          <div class="flex items-center gap-2 text-xs text-slate-500">
            <LockKeyhole :size="14" class="text-slate-400" aria-hidden="true" />
            Private by default
          </div>
          <button class="mt-3 flex h-8 w-full items-center gap-2 rounded-md px-1 text-left text-xs text-slate-500 transition-colors hover:text-slate-200">
            <Settings :size="14" aria-hidden="true" />
            Settings
          </button>
        </div>
      </aside>

      <section id="workspace" class="min-w-0 bg-slate-950">
        <header class="flex min-h-16 items-center gap-3 border-b border-slate-800 px-5 lg:px-7">
          <div class="min-w-0">
            <div class="flex items-center gap-2">
              <h1 class="truncate text-sm font-semibold text-slate-100">Untitled request</h1>
              <span class="size-1.5 rounded-full bg-amber-300" title="Unsaved request" />
            </div>
            <p class="mt-0.5 text-xs text-slate-500">One-off request, not saved</p>
          </div>
          <div class="ml-auto flex items-center gap-2">
            <span class="hidden font-mono text-[11px] text-slate-600 sm:inline">⌘ ↵</span>
            <Button :disabled="!canSend || isSending" @click="sendRequest">
              <Send :size="15" aria-hidden="true" />
              {{ isSending ? "Sending" : "Send" }}
            </Button>
          </div>
        </header>

        <div class="grid min-h-[calc(100vh-64px)] grid-rows-[minmax(340px,0.92fr)_minmax(320px,1.08fr)] max-[820px]:min-h-0 max-[820px]:grid-rows-none">
          <section class="min-h-0 border-b border-slate-800 px-5 py-5 lg:px-7 lg:py-6">
            <div class="flex items-center gap-3">
              <label class="sr-only" for="method">HTTP method</label>
              <div class="relative shrink-0">
                <select id="method" v-model="method" class="h-11 appearance-none rounded-md border border-slate-700 bg-slate-900 py-0 pl-3 pr-9 font-mono text-sm font-semibold text-cyan-300 outline-none transition-colors focus:border-cyan-400 focus:ring-2 focus:ring-cyan-400/20">
                  <option v-for="item in methods" :key="item" :value="item">{{ item }}</option>
                </select>
                <ChevronDown :size="15" class="pointer-events-none absolute right-3 top-3.5 text-slate-500" aria-hidden="true" />
              </div>
              <label class="sr-only" for="request-url">Request URL</label>
              <input
                id="request-url"
                v-model="url"
                class="h-11 min-w-0 flex-1 rounded-md border border-slate-700 bg-slate-900 px-3 font-mono text-sm text-slate-100 outline-none placeholder:text-slate-600 focus:border-cyan-400 focus:ring-2 focus:ring-cyan-400/20"
                placeholder="https://api.example.com/resource"
                spellcheck="false"
                type="url"
                @keydown.meta.enter.prevent="sendRequest"
                @keydown.ctrl.enter.prevent="sendRequest"
              />
            </div>
            <p v-if="!canSend && url" class="mt-2 text-xs text-rose-300">Enter a complete http:// or https:// URL.</p>

            <div class="mt-6 flex items-end justify-between border-b border-slate-800">
              <div class="flex gap-5" role="tablist" aria-label="Request options">
                <button
                  v-for="tab in tabs"
                  :key="tab"
                  class="border-b-2 px-0 pb-3 text-sm transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan-400"
                  :class="activeTab === tab ? 'border-cyan-400 text-slate-100' : 'border-transparent text-slate-500 hover:text-slate-300'"
                  :aria-selected="activeTab === tab"
                  role="tab"
                  type="button"
                  @click="activeTab = tab"
                >
                  {{ tab }}
                  <span v-if="tab === 'Headers'" class="ml-1 font-mono text-[10px] text-slate-600">{{ enabledHeaders.length }}</span>
                </button>
              </div>
              <span class="mb-3 hidden font-mono text-[11px] text-slate-600 sm:inline">Request editor</span>
            </div>

            <div v-if="activeTab === 'Body'" class="pt-4">
              <div class="mb-2 flex items-center justify-between">
                <label for="request-body" class="text-xs font-medium text-slate-400">JSON body</label>
                <span class="font-mono text-[10px] uppercase tracking-[0.12em] text-slate-600">application/json</span>
              </div>
              <textarea
                id="request-body"
                v-model="requestBody"
                class="request-textarea min-h-32 w-full resize-y rounded-md border border-slate-800 bg-slate-900/70 p-3 font-mono text-[13px] leading-6 text-slate-200 outline-none placeholder:text-slate-600 focus:border-cyan-400 focus:ring-2 focus:ring-cyan-400/20"
                placeholder="{ }"
                spellcheck="false"
              />
            </div>

            <div v-else-if="activeTab === 'Headers'" class="pt-4">
              <div class="space-y-2">
                <div v-for="(header, index) in headers" :key="index" class="grid grid-cols-[auto_minmax(110px,1fr)_minmax(120px,1.8fr)_auto] items-center gap-2">
                  <label class="flex size-8 cursor-pointer items-center justify-center" :aria-label="`Enable ${header.key || 'header'}`">
                    <input v-model="header.enabled" class="size-3.5 accent-cyan-400" type="checkbox" />
                  </label>
                  <input v-model="header.key" class="h-9 min-w-0 rounded border border-slate-800 bg-slate-900 px-2 font-mono text-xs text-slate-200 outline-none focus:border-cyan-400" placeholder="Header" />
                  <input v-model="header.value" class="h-9 min-w-0 rounded border border-slate-800 bg-slate-900 px-2 font-mono text-xs text-slate-200 outline-none focus:border-cyan-400" placeholder="Value" />
                  <button class="flex size-8 items-center justify-center rounded text-slate-600 hover:bg-slate-800 hover:text-rose-300" :aria-label="`Remove header ${header.key}`" type="button" @click="removeHeader(index)">
                    <Trash2 :size="14" aria-hidden="true" />
                  </button>
                </div>
              </div>
              <Button class="mt-3" variant="ghost" @click="addHeader"><Plus :size="14" aria-hidden="true" /> Add header</Button>
            </div>

            <div v-else class="flex min-h-32 items-center rounded-md border border-dashed border-slate-800 bg-slate-900/35 px-4 text-sm text-slate-500">
              {{ activeTab }} settings are not needed for this request yet.
            </div>
          </section>

          <section class="min-h-0 px-5 py-5 lg:px-7 lg:py-6" aria-live="polite">
            <div class="mb-4 flex items-center justify-between">
              <div class="flex items-center gap-3">
                <h2 class="text-sm font-semibold text-slate-200">Response</h2>
                <template v-if="response">
                  <span class="rounded border border-emerald-400/25 bg-emerald-400/10 px-2 py-0.5 font-mono text-xs font-semibold text-emerald-300">{{ response.status }} {{ response.statusText }}</span>
                  <span class="font-mono text-xs text-slate-500">{{ response.durationMs }} ms</span>
                </template>
              </div>
              <Button v-if="response" variant="secondary" @click="copyResponse">
                <Check v-if="copied" :size="14" aria-hidden="true" />
                <Copy v-else :size="14" aria-hidden="true" />
                {{ copied ? "Copied" : "Copy" }}
              </Button>
            </div>

            <div v-if="isSending" class="animate-pulse rounded-md border border-slate-800 bg-slate-900/55 p-4">
              <div class="h-3 w-24 rounded bg-slate-700" />
              <div class="mt-4 h-3 w-3/4 rounded bg-slate-800" />
              <div class="mt-2 h-3 w-1/2 rounded bg-slate-800" />
              <div class="mt-2 h-3 w-2/3 rounded bg-slate-800" />
            </div>

            <div v-else-if="errorMessage" class="rounded-md border border-rose-400/25 bg-rose-400/10 p-4">
              <p class="text-sm font-medium text-rose-200">Request failed</p>
              <p class="mt-1 font-mono text-xs leading-5 text-rose-200/75">{{ errorMessage }}</p>
            </div>

            <div v-else-if="response" class="grid min-h-0 grid-cols-[minmax(0,1fr)_200px] divide-x divide-slate-800 overflow-hidden rounded-md border border-slate-800 max-[980px]:grid-cols-1 max-[980px]:divide-x-0 max-[980px]:divide-y">
              <pre class="min-h-56 overflow-auto bg-slate-900/65 p-4 font-mono text-[13px] leading-6 text-slate-200"><code>{{ visibleResponse }}</code></pre>
              <dl class="bg-slate-900/35 p-4 text-xs">
                <div class="border-b border-slate-800 pb-3">
                  <dt class="uppercase tracking-[0.12em] text-slate-600">Status</dt>
                  <dd class="mt-1 font-mono text-slate-200">{{ response.status }}</dd>
                </div>
                <div class="border-b border-slate-800 py-3">
                  <dt class="uppercase tracking-[0.12em] text-slate-600">Time</dt>
                  <dd class="mt-1 font-mono text-slate-200">{{ response.durationMs }} ms</dd>
                </div>
                <div class="pt-3">
                  <dt class="uppercase tracking-[0.12em] text-slate-600">Headers</dt>
                  <dd class="mt-1 font-mono text-slate-200">{{ response.headers.length }} received</dd>
                </div>
              </dl>
            </div>

            <div v-else class="flex min-h-56 flex-col items-center justify-center rounded-md border border-dashed border-slate-800 bg-slate-900/25 p-6 text-center">
              <div class="flex size-9 items-center justify-center rounded-md border border-slate-800 bg-slate-900 text-slate-500">
                <FileJson :size="18" aria-hidden="true" />
              </div>
              <p class="mt-3 text-sm font-medium text-slate-400">Ready to inspect a response</p>
              <p class="mt-1 max-w-sm text-xs leading-5 text-slate-600">Send a request to view status, timing, headers, and the response body here.</p>
              <div class="mt-4 flex items-center gap-2 font-mono text-[11px] text-slate-600"><Braces :size="13" aria-hidden="true" /> JSON formatted when possible</div>
            </div>
          </section>
        </div>
      </section>
    </div>
  </main>
</template>
