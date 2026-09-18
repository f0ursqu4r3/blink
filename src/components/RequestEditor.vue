<script setup lang="ts">
import { computed, ref, useId } from "vue";
import { TabsRoot, TabsList, TabsTrigger, TabsContent } from "reka-ui";
import { Braces, KeyRound } from "lucide-vue-next";
import { Button } from "@/components/ui/button";
import KeyValueEditor from "./KeyValueEditor.vue";
import { activePairs, supportsBody, type Draft } from "@/lib/request";
import { formatJson } from "@/lib/json";
const draft = defineModel<Draft>({ required: true });
defineProps<{ busy: boolean }>();
const id = useId();
const tab = ref("query");
const formatError = ref("");
const bodyPlaceholder = computed(() =>
  draft.value.bodyMode === "json" ? '{\n  "key": "value"\n}' : "Request body",
);
const bodyAllowed = computed(() => supportsBody(draft.value.method));
const tabs = computed(() => [
  { id: "query", label: "Query", count: activePairs(draft.value.query).length },
  {
    id: "headers",
    label: "Headers",
    count: activePairs(draft.value.headers).length,
  },
  {
    id: "body",
    label: "Body",
    count: bodyAllowed.value && draft.value.bodyMode !== "none" ? 1 : 0,
  },
  { id: "auth", label: "Auth", count: draft.value.auth !== "none" ? 1 : 0 },
]);
function formatBody() {
  try {
    draft.value.body = formatJson(draft.value.body);
    formatError.value = "";
  } catch {
    formatError.value = "Invalid JSON. The body was not changed.";
  }
}
</script>

<template>
  <section class="request-panel" :aria-labelledby="`${id}-heading`">
    <header class="panel-heading">
      <h2 :id="`${id}-heading`"><span>01</span> Request</h2>
      <span class="text-muted-foreground">{{
        busy ? "SENDING" : "COMPOSE"
      }}</span>
    </header>
    <TabsRoot v-model="tab" class="editor-tabs">
      <TabsList class="tab-list" aria-label="Request options">
        <TabsTrigger
          v-for="item in tabs"
          :key="item.id"
          :value="item.id"
          class="tab-trigger"
        >
          {{ item.label
          }}<span v-if="item.count" class="tab-count">{{ item.count }}</span>
        </TabsTrigger>
      </TabsList>
      <TabsContent value="query" class="tab-content">
        <KeyValueEditor v-model="draft.query" label="Query" :disabled="busy" />
        <p class="editor-note">
          Enabled rows are appended to the URL. Duplicate keys are preserved.
        </p>
      </TabsContent>
      <TabsContent value="headers" class="tab-content">
        <KeyValueEditor
          v-model="draft.headers"
          label="Header"
          :disabled="busy"
        />
        <p class="editor-note">
          Body mode sets Content-Type unless overridden here.
        </p>
      </TabsContent>
      <TabsContent value="body" class="tab-content body-content">
        <div class="body-toolbar">
          <label :for="`${id}-body-mode`">Body</label>
          <select
            :id="`${id}-body-mode`"
            v-model="draft.bodyMode"
            :disabled="busy"
            class="compact-select"
          >
            <option value="none">None</option>
            <option value="json">JSON</option>
            <option value="text">Text</option>
          </select>
          <Button
            v-if="draft.bodyMode === 'json'"
            variant="ghost"
            class="ml-auto"
            :disabled="busy || !draft.body"
            @click="formatBody"
            ><Braces :size="13" aria-hidden="true" />Format</Button
          >
        </div>
        <p v-if="!bodyAllowed" class="editor-note border-b">
          {{ draft.method }} sends no body. Your draft is retained.
        </p>
        <template v-if="draft.bodyMode !== 'none'">
          <label class="sr-only" :for="`${id}-body`">Request body</label>
          <textarea
            :id="`${id}-body`"
            v-model="draft.body"
            :disabled="busy"
            class="body-editor"
            spellcheck="false"
            autocomplete="off"
            :placeholder="bodyPlaceholder"
            @input="formatError = ''"
          />
        </template>
        <p v-else class="editor-note">No request body.</p>
        <p v-if="formatError" role="alert" class="editor-note text-destructive">
          {{ formatError }}
        </p>
      </TabsContent>
      <TabsContent value="auth" class="tab-content">
        <div class="auth-form">
          <label :for="`${id}-auth-type`">Authorization</label>
          <select
            :id="`${id}-auth-type`"
            v-model="draft.auth"
            class="compact-select"
            :disabled="busy"
          >
            <option value="none">No auth</option>
            <option value="bearer">Bearer token</option>
            <option value="basic">Basic auth</option>
          </select>
          <template v-if="draft.auth === 'bearer'">
            <label :for="`${id}-auth-token`">Token</label
            ><input
              :id="`${id}-auth-token`"
              v-model="draft.token"
              type="password"
              :disabled="busy"
              autocomplete="off"
              spellcheck="false"
              placeholder="Bearer token"
            />
          </template>
          <template v-if="draft.auth === 'basic'">
            <label :for="`${id}-auth-user`">Username</label
            ><input
              :id="`${id}-auth-user`"
              v-model="draft.username"
              :disabled="busy"
              autocomplete="off"
              spellcheck="false"
            />
            <label :for="`${id}-auth-password`">Password</label
            ><input
              :id="`${id}-auth-password`"
              v-model="draft.password"
              type="password"
              :disabled="busy"
              autocomplete="off"
            />
          </template>
        </div>
        <p class="editor-note flex items-center gap-2">
          <KeyRound :size="13" aria-hidden="true" />Credentials stay in memory.
          Copy cURL includes them.
        </p>
      </TabsContent>
    </TabsRoot>
    <footer class="panel-footer">
      {{ activePairs(draft.query).length }} QUERY ·
      {{ activePairs(draft.headers).length }} HEADERS<span
        >AUTH / {{ draft.auth.toUpperCase() }}</span
      >
    </footer>
  </section>
</template>

<style scoped>
.request-panel {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
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
  text-transform: uppercase;
  font-size: 0.6875rem;
}
h2 span {
  color: var(--primary);
  margin-right: 10px;
}
.editor-tabs {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.tab-list {
  display: flex;
  flex-shrink: 0;
  border-bottom: 1px solid var(--border);
  padding: 0 8px;
}
.tab-trigger {
  height: 38px;
  padding: 0 12px;
  border-bottom: 1px solid transparent;
  font-size: 0.75rem;
  color: var(--muted-foreground);
  display: flex;
  gap: 7px;
  align-items: center;
}
.tab-trigger:hover {
  color: var(--foreground);
  background: var(--muted);
}
.tab-trigger[data-state="active"] {
  color: var(--primary);
  border-bottom-color: var(--primary);
}
.tab-count {
  font: 0.625rem var(--font-mono);
  color: var(--muted-foreground);
}
.tab-content {
  flex: 1;
  min-height: 0;
  overflow: auto;
  outline-offset: -2px;
}
.body-content[data-state="active"] {
  display: flex;
  flex-direction: column;
}
.body-toolbar {
  display: flex;
  align-items: center;
  padding: 8px 12px;
  gap: 10px;
  border-bottom: 1px solid var(--border);
  color: var(--muted-foreground);
  font-size: 0.75rem;
}
.compact-select {
  height: 28px;
  padding: 0 8px;
  font: 0.75rem var(--font-mono);
}
.editor-note {
  padding: 12px 16px;
  font-size: 0.6875rem;
  line-height: 1.7;
  color: var(--muted-foreground);
}
.editor-note.text-destructive {
  color: var(--destructive);
}
.body-editor {
  flex: 1;
  min-height: 180px;
  width: 100%;
  resize: none;
  border: 0;
  border-radius: 0;
  padding: 16px;
  font: 0.8125rem/1.75 var(--font-mono);
  background: transparent;
  tab-size: 2;
}
.auth-form {
  display: grid;
  grid-template-columns: 100px minmax(0, 1fr);
  gap: 12px;
  align-items: center;
  padding: 16px;
  font-size: 0.75rem;
}
.auth-form label {
  color: var(--muted-foreground);
}
.auth-form input {
  height: 30px;
  min-width: 0;
  padding: 0 8px;
  font-family: var(--font-mono);
}
.panel-footer {
  height: 28px;
  padding: 0 16px;
  border-top: 1px solid var(--border);
  display: flex;
  align-items: center;
  justify-content: space-between;
  font: 0.5625rem var(--font-mono);
  letter-spacing: 0.12em;
  color: var(--muted-foreground);
}
@media (pointer: coarse) {
  .tab-trigger {
    min-height: 44px;
  }
  .compact-select,
  .auth-form input {
    min-height: 44px;
    font-size: 1rem;
  }
  .body-editor {
    font-size: 1rem;
  }
}
</style>
