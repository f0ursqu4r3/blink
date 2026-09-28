<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useTheme } from "@/composables/useTheme";
import { accentSlots, DEFAULT_ACCENT, type AccentSlot } from "@/lib/theme";
import {
  canReadGhosttyThemes,
  listGhosttyThemes,
  readGhosttyTheme,
} from "@/lib/ghostty-themes";

const { draft, error, saveError, preview } = useTheme();
const names = ref<string[]>([]);
const readError = ref("");
const accent = computed(() => draft.value?.accent ?? DEFAULT_ACCENT);
const message = computed(() => error.value || saveError.value);
// Guards against a slow readGhosttyTheme() overwriting a newer edit, reset,
// theme choice, or accent change made while the read was in flight.
let request = 0;

onMounted(async () => {
  if (!canReadGhosttyThemes) return;
  try {
    names.value = await listGhosttyThemes();
  } catch {
    readError.value = "Could not list Ghostty themes.";
  }
});

async function choose(name: string) {
  readError.value = "";
  const current = ++request;
  if (!name) return preview(null);
  try {
    const text = await readGhosttyTheme(name);
    if (current !== request) return;
    preview({ name, text, accent: accent.value });
  } catch (reason) {
    if (current !== request) return;
    readError.value = String(reason);
  }
}
function edit(text: string) {
  request++;
  if (!text.trim()) return preview(null);
  preview({ name: "Custom", text, accent: accent.value });
}
function setAccent(value: AccentSlot) {
  request++;
  if (draft.value) preview({ ...draft.value, accent: value });
}
function reset() {
  request++;
  preview(null);
}
</script>

<template>
  <section class="grid gap-2 border-t border-border pt-3 text-xs">
    <h3 class="font-semibold uppercase tracking-[0.07em] text-muted-foreground">
      Theme
    </h3>
    <div v-if="canReadGhosttyThemes" class="grid gap-1.5 text-muted-foreground">
      <label for="app-theme">Ghostty theme</label>
      <select
        id="app-theme"
        :value="draft?.name ?? ''"
        class="h-8 w-full min-w-0 border border-input rounded-sm px-2 bg-background text-foreground font-mono"
        @change="choose(($event.target as HTMLSelectElement).value)"
      >
        <option value="">Blink (default)</option>
        <option v-if="draft && !names.includes(draft.name)" :value="draft.name">
          {{ draft.name }}
        </option>
        <option v-for="name in names" :key="name" :value="name">
          {{ name }}
        </option>
      </select>
      <p
        v-if="readError"
        class="text-destructive font-mono"
        role="alert"
        data-theme-read-error
      >
        {{ readError }}
      </p>
    </div>
    <div class="grid gap-1.5 text-muted-foreground">
      <label for="app-theme-colors">Ghostty colors</label>
      <textarea
        id="app-theme-colors"
        :value="draft?.text ?? ''"
        rows="6"
        class="min-h-28 w-full resize-y p-2 border border-input rounded-sm text-foreground bg-background font-mono text-xs leading-relaxed"
        placeholder="background = #1e1e1e&#10;foreground = #d4d4d4&#10;palette = 3=#dcdcaa"
        data-theme-colors
        spellcheck="false"
        autocomplete="off"
        @input="edit(($event.target as HTMLTextAreaElement).value)"
      />
      <p
        v-if="message"
        class="text-destructive font-mono"
        role="alert"
        data-theme-error
      >
        {{ message }}
      </p>
    </div>
    <div class="flex items-end gap-3">
      <div class="grid flex-1 gap-1.5 text-muted-foreground">
        <label for="app-theme-accent">Accent</label>
        <select
          id="app-theme-accent"
          :value="accent"
          :disabled="!draft"
          class="h-8 w-full min-w-0 border border-input rounded-sm px-2 bg-background text-foreground font-mono disabled:opacity-50"
          @change="
            setAccent(
              Number(($event.target as HTMLSelectElement).value) as AccentSlot,
            )
          "
        >
          <option
            v-for="slot in accentSlots"
            :key="slot.value"
            :value="slot.value"
          >
            {{ slot.label }} ({{ slot.value }})
          </option>
        </select>
      </div>
      <button
        type="button"
        class="h-8 px-3 border border-border rounded-sm bg-background font-mono text-xs disabled:opacity-50"
        data-theme-reset
        :disabled="!draft"
        @click="reset"
      >
        Reset to default
      </button>
    </div>
  </section>
</template>
