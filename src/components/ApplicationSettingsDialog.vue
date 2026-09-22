<script setup lang="ts">
import { ref, watch } from 'vue';

const props = defineProps<{
  definitions: Record<string, string>;
  open: boolean;
}>();

const emit = defineEmits<{
  (event: 'update:open', value: boolean): void;
  (event: 'save', definitions: Record<string, string>): void;
}>();

const source = ref('{}');
const error = ref('');

function formatDefinitions(definitions: Record<string, string>) {
  return JSON.stringify(definitions, null, 2);
}

watch(
  () => [props.open, props.definitions] as const,
  ([open]) => {
    if (!open) return;
    source.value = formatDefinitions(props.definitions);
    error.value = '';
  },
  { immediate: true }
);

function parseDefinitions(): Record<string, string> | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(source.value);
  } catch {
    error.value = 'Enter a valid JSON object.';
    return null;
  }
  if (
    !parsed ||
    Array.isArray(parsed) ||
    typeof parsed !== 'object' ||
    Object.values(parsed).some((value) => typeof value !== 'string')
  ) {
    error.value = 'Token definitions must be a JSON object with string values.';
    return null;
  }
  if (Object.keys(parsed).some((name) => name.startsWith('_'))) {
    error.value = 'Token names must not start with _.';
    return null;
  }
  return parsed as Record<string, string>;
}

function save() {
  const definitions = parseDefinitions();
  if (!definitions) return;
  emit('save', definitions);
}
</script>

<template>
  <div
    v-if="open"
    class="fixed inset-0 z-50 grid place-items-center bg-black/50"
    role="dialog"
    aria-modal="true"
    aria-label="Application settings"
    @keydown.esc="emit('update:open', false)"
  >
    <form
      class="grid gap-3 w-[min(640px,calc(100vw-32px))] p-5 border border-border rounded bg-background"
      @submit.prevent="save"
    >
      <header>
        <div class="flex items-center gap-1.5">
          <h2 class="text-sm font-bold tracking-[0.08em]">
            Application Settings
          </h2>
          <button
            class="grid place-items-center w-4 h-4 p-0 border border-input rounded-full text-muted-foreground bg-muted font-mono text-[10px] font-semibold leading-none hover:text-foreground hover:bg-accent focus-visible:text-foreground focus-visible:bg-accent"
            data-token-help="global"
            type="button"
            aria-label="Token syntax help"
            title="Workspace-global tokens are available as {{_.name}}. Use <<NAME>> in a token value to read NAME from Blink's process environment when a request is sent."
          >
            ?
          </button>
        </div>
      </header>
      <label class="font-mono text-xs" for="global-token-json">
        Global tokens
      </label>
      <textarea
        id="global-token-json"
        v-model="source"
        class="min-h-70 resize-y p-2.5 border border-input rounded-sm text-foreground bg-background font-mono text-xs leading-relaxed"
        data-global-token-json
        spellcheck="false"
        autocomplete="off"
      />
      <p
        v-if="error"
        class="text-destructive font-mono text-xs"
        data-token-json-error
        role="alert"
      >
        {{ error }}
      </p>
      <footer class="flex justify-end gap-2">
        <button
          class="h-7.5 px-3.5 border border-border rounded-sm bg-background font-mono text-xs"
          type="button"
          @click="emit('update:open', false)"
        >
          Cancel
        </button>
        <button
          class="h-7.5 px-3.5 border border-primary rounded-sm bg-primary text-primary-foreground font-mono text-xs"
          data-save-application-settings
          type="submit"
        >
          Save
        </button>
      </footer>
    </form>
  </div>
</template>
