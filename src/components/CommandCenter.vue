<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { Search } from "lucide-vue-next";
import type { RequestGroup } from "@/lib/groups";
import type { RequestSession } from "@/lib/session";
import { matchRequests } from "@/lib/command-center";

const props = defineProps<{
  sessions: RequestSession[];
  groups: RequestGroup[];
}>();
const emit = defineEmits<{ select: [id: number] }>();

const open = ref(false);
const query = ref("");
const index = ref(0);
const input = ref<HTMLInputElement>();
const trigger = ref<HTMLButtonElement>();
let opener: HTMLElement | null = null;
const matches = computed(() =>
  matchRequests(props.sessions, props.groups, query.value),
);
watch(query, () => (index.value = 0));
watch(matches, (list) => {
  if (index.value >= list.length) index.value = Math.max(0, list.length - 1);
});

async function show() {
  opener = document.activeElement as HTMLElement | null;
  query.value = "";
  index.value = 0;
  open.value = true;
  await nextTick();
  input.value?.focus();
}
async function hide() {
  open.value = false;
  await nextTick();
  const target =
    opener && opener !== document.body && opener.isConnected
      ? opener
      : trigger.value;
  target?.focus();
}
function choose(id: number) {
  open.value = false;
  emit("select", id);
}
function onKey(event: KeyboardEvent) {
  const count = matches.value.length;
  if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    void hide();
  } else if (event.key === "ArrowDown" && count) {
    event.preventDefault();
    index.value = (index.value + 1) % count;
  } else if (event.key === "ArrowUp" && count) {
    event.preventDefault();
    index.value = (index.value - 1 + count) % count;
  } else if (event.key === "Enter" && count) {
    const match = matches.value[index.value];
    if (match) {
      event.preventDefault();
      choose(match.id);
    }
  }
}
defineExpose({ show });
</script>

<template>
  <div class="relative w-[min(420px,50vw)]" data-command-center>
    <button
      v-show="!open"
      ref="trigger"
      type="button"
      class="flex h-6.5 w-full items-center gap-2 rounded border border-border bg-muted px-2.5 text-xs text-muted-foreground hover:bg-accent hover:text-foreground"
      data-command-center-trigger
      aria-label="Search requests"
      title="Search requests · Cmd/Ctrl+P"
      @click="show"
    >
      <Search :size="13" aria-hidden="true" />
      <span class="flex-1 text-left">Search requests</span>
      <kbd class="text-[10px]">⌘P</kbd>
    </button>
    <template v-if="open">
      <button
        type="button"
        tabindex="-1"
        class="fixed inset-0 z-40 cursor-default"
        aria-label="Close request search"
        @click="hide"
      />
      <div
        class="absolute inset-x-0 top-0 z-50 overflow-hidden rounded-lg border border-border bg-secondary shadow-[0_8px_24px_oklch(0_0_0/0.4)]"
        data-surface="command-center"
      >
        <input
          ref="input"
          v-model="query"
          role="combobox"
          aria-label="Search requests"
          aria-autocomplete="list"
          aria-expanded="true"
          aria-controls="command-center-list"
          :aria-activedescendant="
            matches[index]
              ? `command-center-option-${matches[index].id}`
              : undefined
          "
          class="h-7 w-full rounded-none border-0 border-b border-border bg-transparent px-2.5 text-xs outline-none focus-visible:outline-none"
          placeholder="Search requests by name, URL, or group"
          spellcheck="false"
          autocomplete="off"
          @keydown="onKey"
        />
        <ul
          id="command-center-list"
          role="listbox"
          aria-label="Requests"
          class="max-h-72 overflow-auto py-1"
        >
          <li
            v-for="(match, i) in matches"
            :id="`command-center-option-${match.id}`"
            :key="match.id"
            role="option"
            :aria-selected="i === index"
            class="flex cursor-pointer items-center gap-2 px-2.5 py-1 text-xs aria-selected:bg-accent"
            @mousedown.prevent="choose(match.id)"
            @mousemove="index = i"
          >
            <span
              class="method w-14 shrink-0 font-mono text-[10px]"
              :data-method="match.method"
              >{{ match.method }}</span
            >
            <span class="min-w-0 flex-1 truncate">{{ match.label }}</span>
            <span
              v-if="match.groupPath"
              class="max-w-[40%] truncate text-muted-foreground"
              >{{ match.groupPath }}</span
            >
          </li>
        </ul>
        <p
          v-if="!matches.length"
          class="px-2.5 pb-2 text-xs text-muted-foreground"
          role="status"
        >
          No matching requests
        </p>
      </div>
    </template>
  </div>
</template>
