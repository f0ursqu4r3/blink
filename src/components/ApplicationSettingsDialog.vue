<script setup lang="ts">
import { ref, watch } from "vue";

const props = defineProps<{
  definitions: Record<string, string>;
  open: boolean;
}>();

const emit = defineEmits<{
  (event: "update:open", value: boolean): void;
  (event: "save", definitions: Record<string, string>): void;
}>();

const source = ref("{}");
const error = ref("");

function formatDefinitions(definitions: Record<string, string>) {
  return JSON.stringify(definitions, null, 2);
}

watch(
  () => [props.open, props.definitions] as const,
  ([open]) => {
    if (!open) return;
    source.value = formatDefinitions(props.definitions);
    error.value = "";
  },
  { immediate: true },
);

function parseDefinitions(): Record<string, string> | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(source.value);
  } catch {
    error.value = "Enter a valid JSON object.";
    return null;
  }
  if (
    !parsed ||
    Array.isArray(parsed) ||
    typeof parsed !== "object" ||
    Object.values(parsed).some((value) => typeof value !== "string")
  ) {
    error.value = "Token definitions must be a JSON object with string values.";
    return null;
  }
  if (Object.keys(parsed).some((name) => name.startsWith("_"))) {
    error.value = "Token names must not start with _.";
    return null;
  }
  return parsed as Record<string, string>;
}

function save() {
  const definitions = parseDefinitions();
  if (!definitions) return;
  emit("save", definitions);
}
</script>

<template>
  <div
    v-if="open"
    class="dialog-backdrop"
    role="dialog"
    aria-modal="true"
    aria-label="Application settings"
    @keydown.esc="emit('update:open', false)"
  >
    <form class="dialog-content" @submit.prevent="save">
      <header>
        <h2>Application Settings</h2>
        <p>
          Workspace-global tokens are available as
          <code v-text="'{{_.name}}'" />.
        </p>
      </header>
      <label for="global-token-json">Global tokens</label>
      <textarea
        id="global-token-json"
        v-model="source"
        data-global-token-json
        spellcheck="false"
        autocomplete="off"
      />
      <p class="help-text">
        Use <code>&lt;&lt;NAME&gt;&gt;</code> in a token value to read NAME from
        Blink's process environment when a request is sent.
      </p>
      <p v-if="error" data-token-json-error role="alert">{{ error }}</p>
      <footer>
        <button type="button" @click="emit('update:open', false)">
          Cancel
        </button>
        <button data-save-application-settings type="submit">Save</button>
      </footer>
    </form>
  </div>
</template>

<style scoped>
.dialog-backdrop {
  position: fixed;
  inset: 0;
  z-index: 50;
  display: grid;
  place-items: center;
  background: rgb(0 0 0 / 0.5);
}
.dialog-content {
  display: grid;
  gap: 12px;
  width: min(640px, calc(100vw - 32px));
  padding: 20px;
  border: 1px solid var(--border);
  border-radius: 4px;
  background: var(--background);
}
h2 {
  font-size: 0.875rem;
  font-weight: 700;
  letter-spacing: 0.08em;
}
p,
label,
textarea,
button {
  font: 0.75rem var(--font-mono);
}
p {
  color: var(--muted-foreground);
}
textarea {
  min-height: 280px;
  resize: vertical;
  padding: 10px;
  border: 1px solid var(--input);
  border-radius: 2px;
  color: var(--foreground);
  background: var(--background);
  line-height: 1.5;
}
.help-text {
  font-size: 0.6875rem;
}
[role="alert"] {
  color: var(--destructive);
}
footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
button {
  height: 30px;
  padding: 0 14px;
  border: 1px solid var(--border);
  border-radius: 2px;
  background: var(--background);
}
button:last-child {
  border-color: var(--primary);
  color: var(--primary-foreground);
  background: var(--primary);
}
</style>
