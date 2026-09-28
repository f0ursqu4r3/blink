<script setup lang="ts">
import { ref, watch } from "vue";
import {
  DialogContent,
  DialogOverlay,
  DialogPortal,
  DialogRoot,
  DialogTitle,
} from "reka-ui";
import HelpTooltip from "./HelpTooltip.vue";
import ThemeSettings from "./ThemeSettings.vue";
import { useTheme } from "../composables/useTheme";
import {
  defaultPreferences,
  type WorkspacePreferences,
} from "../lib/preferences";

const props = defineProps<{
  definitions: Record<string, string>;
  preferences?: WorkspacePreferences;
  open: boolean;
}>();

const emit = defineEmits<{
  (event: "update:open", value: boolean): void;
  (
    event: "save",
    definitions: Record<string, string>,
    preferences: WorkspacePreferences,
  ): void;
}>();

const methodInput = ref<HTMLSelectElement | null>(null);
const source = ref("{}");
const error = ref("");
const preferences = ref(defaultPreferences());
const theme = useTheme();
// Theme edits preview live. Closing without a successful save restores the
// saved theme.
let committed = false;
watch(
  () => props.open,
  (open, wasOpen) => {
    if (open && !wasOpen) {
      committed = false;
      theme.revert();
    } else if (!open && wasOpen && !committed) theme.revert();
  },
);

function formatDefinitions(definitions: Record<string, string>) {
  return JSON.stringify(definitions, null, 2);
}

watch(
  () => [props.open, props.definitions] as const,
  ([open]) => {
    if (!open) return;
    source.value = formatDefinitions(props.definitions);
    preferences.value = { ...(props.preferences ?? defaultPreferences()) };
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
  if (!definitions || theme.error.value) return;
  emit("save", definitions, { ...preferences.value });
  if (theme.commit()) return;
  committed = true;
  emit("update:open", false);
}
</script>

<template>
  <DialogRoot :open="open" @update:open="emit('update:open', $event)">
    <DialogPortal>
      <DialogOverlay class="fixed inset-0 z-50 bg-black/50" />
      <DialogContent
        class="fixed z-[60] left-1/2 top-1/2 flex max-h-[90dvh] w-[min(560px,calc(100vw-24px))] -translate-x-1/2 -translate-y-1/2 flex-col rounded-lg overflow-hidden border border-border bg-background"
        :aria-describedby="undefined"
        @open-auto-focus.prevent="methodInput?.focus()"
        @pointer-down-outside.prevent
      >
        <form class="flex min-h-0 flex-col" @submit.prevent="save">
          <header class="shrink-0 border-b border-border px-4 py-3">
            <DialogTitle class="text-sm font-bold tracking-[0.08em]"
              >Application Settings</DialogTitle
            >
          </header>
          <div class="min-h-0 overflow-y-auto p-4 grid gap-4">
            <fieldset
              class="grid grid-cols-2 gap-3 text-xs max-[440px]:grid-cols-1"
            >
              <div
                class="col-span-full flex items-center gap-1.5 text-muted-foreground"
              >
                <span class="font-semibold uppercase tracking-[0.07em]"
                  >New request defaults</span
                >
                <HelpTooltip
                  text="These settings apply only when you create a new request. They do not change existing requests or duplicates."
                >
                  <button
                    type="button"
                    aria-label="New request defaults help"
                    class="help-trigger"
                  >
                    ?
                  </button>
                </HelpTooltip>
              </div>
              <div class="grid gap-1.5 text-muted-foreground">
                <label for="app-default-method">Method</label>
                <select
                  id="app-default-method"
                  ref="methodInput"
                  v-model="preferences.defaultMethod"
                  class="h-8 w-full min-w-0 border border-input rounded-sm px-2 bg-background text-foreground font-mono"
                >
                  <option
                    v-for="method in [
                      'GET',
                      'POST',
                      'PUT',
                      'PATCH',
                      'DELETE',
                      'HEAD',
                      'OPTIONS',
                    ]"
                    :key="method"
                  >
                    {{ method }}
                  </option>
                </select>
              </div>
              <div class="grid gap-1.5 text-muted-foreground">
                <label for="app-default-body">Body mode</label>
                <select
                  id="app-default-body"
                  v-model="preferences.defaultBodyMode"
                  class="h-8 w-full min-w-0 border border-input rounded-sm px-2 bg-background text-foreground font-mono"
                >
                  <option value="none">None</option>
                  <option value="json">JSON</option>
                  <option value="text">Text</option>
                  <option value="graphql">GraphQL</option>
                </select>
              </div>
              <label class="flex items-center gap-2"
                ><input
                  v-model="preferences.pretty"
                  type="checkbox"
                  class="accent-primary"
                />
                Format responses</label
              >
              <label class="flex items-center gap-2"
                ><input
                  v-model="preferences.wrap"
                  type="checkbox"
                  class="accent-primary"
                />
                Wrap response lines</label
              >
            </fieldset>
            <section class="grid gap-2 border-t border-border pt-3 text-xs">
              <h3
                class="font-semibold uppercase tracking-[0.07em] text-muted-foreground"
              >
                Workspace
              </h3>
              <label class="flex items-center gap-2"
                ><input
                  v-model="preferences.confirmCloseDrafts"
                  type="checkbox"
                  class="accent-primary"
                />
                Confirm before closing drafts</label
              >
            </section>
            <ThemeSettings />
            <section class="grid gap-2 border-t border-border pt-3">
              <div class="flex items-center gap-1.5">
                <label
                  class="text-xs font-semibold uppercase tracking-[0.07em] text-muted-foreground"
                  for="global-token-json"
                  >Global tokens</label
                >
                <HelpTooltip
                  text="Workspace-global tokens use {{_.name}}. Use <<NAME>> in a value to read NAME from Blink's process environment when sending."
                >
                  <button
                    class="help-trigger"
                    data-token-help="global"
                    type="button"
                    aria-label="Token syntax help"
                  >
                    ?
                  </button>
                </HelpTooltip>
              </div>
              <textarea
                id="global-token-json"
                v-model="source"
                rows="5"
                class="min-h-28 w-full resize-y p-2 border border-input rounded-sm text-foreground bg-background font-mono text-xs leading-relaxed"
                data-global-token-json
                spellcheck="false"
                autocomplete="off"
              />
            </section>
            <p
              v-if="error"
              class="text-destructive font-mono text-xs"
              data-token-json-error
              role="alert"
            >
              {{ error }}
            </p>
          </div>
          <footer
            class="flex shrink-0 justify-end gap-2 border-t border-border px-4 py-3"
          >
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
      </DialogContent>
    </DialogPortal>
  </DialogRoot>
</template>

<style scoped>
@reference "../style.css";
.help-trigger {
  @apply inline-flex size-5 shrink-0 items-center justify-center rounded-full border border-input text-[10px] text-muted-foreground hover:bg-accent hover:text-foreground;
}
</style>
