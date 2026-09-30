<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { Search } from "lucide-vue-next";
import type { RequestGroup } from "@/lib/groups";
import type { RequestSession } from "@/lib/session";
import {
  COMMAND_PREFIX,
  highlightRuns,
  isCommandQuery,
  matchCommands,
  matchRequests,
  type Command,
} from "@/lib/command-center";
import { shortcutLabel } from "@/lib/shortcut";

const props = defineProps<{
  sessions: RequestSession[];
  groups: RequestGroup[];
  globalDefinitions?: Record<string, string>;
  commands?: Command[];
}>();
const emit = defineEmits<{ select: [id: number]; command: [id: string] }>();

const open = ref(false);
const query = ref("");
const index = ref(0);
const input = ref<HTMLInputElement>();
const trigger = ref<HTMLButtonElement>();
let opener: HTMLElement | null = null;
const commandMode = computed(() => isCommandQuery(query.value));
const commandMatches = computed(() =>
  commandMode.value ? matchCommands(props.commands ?? [], query.value) : [],
);
const requestMatches = computed(() =>
  commandMode.value
    ? []
    : matchRequests(
        props.sessions,
        props.groups,
        query.value,
        props.globalDefinitions,
      ),
);
/** The active list, as option ids, so keys work the same in both modes. */
const matches = computed(() =>
  commandMode.value
    ? commandMatches.value.map((command) => ({
        key: `command-${command.id}`,
        choose: () => command.disabled || runCommand(command.id),
      }))
    : requestMatches.value.map((match) => ({
        key: `request-${match.id}`,
        choose: () => choose(match.id),
      })),
);
watch(query, () => (index.value = 0));
watch(matches, (list) => {
  if (index.value >= list.length) index.value = Math.max(0, list.length - 1);
});

/** Open the search. `show(">")` opens the command list. */
async function show(prefix = "") {
  opener = document.activeElement as HTMLElement | null;
  query.value = prefix;
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
async function runCommand(id: string) {
  // Return focus first, so a command that moves focus keeps it.
  await hide();
  emit("command", id);
}
function onFocusOut(event: FocusEvent) {
  const next = event.relatedTarget as Node | null;
  const container = event.currentTarget as HTMLElement;
  if (!next || !container.contains(next)) {
    // Focus already moved outside; do not steal it back.
    open.value = false;
  }
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
      void match.choose();
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
      :title="`Search requests · ${shortcutLabel(['mod', 'p'])}. Type ${COMMAND_PREFIX} for commands · ${shortcutLabel(['mod', 'shift', 'p'])}`"
      @click="show()"
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
        @focusout="onFocusOut"
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
              ? `command-center-option-${matches[index].key}`
              : undefined
          "
          class="h-7 w-full rounded-none border-0 border-b border-border bg-transparent px-2.5 text-xs outline-none focus-visible:outline-none"
          :placeholder="
            commandMode
              ? 'Run a command'
              : `Search requests by name, URL, or group. Type ${COMMAND_PREFIX} for commands`
          "
          spellcheck="false"
          autocomplete="off"
          @keydown="onKey"
        />
        <ul
          id="command-center-list"
          role="listbox"
          :aria-label="commandMode ? 'Commands' : 'Requests'"
          class="max-h-72 overflow-auto py-1"
        >
          <li
            v-for="(command, i) in commandMatches"
            :id="`command-center-option-command-${command.id}`"
            :key="command.id"
            role="option"
            :aria-selected="i === index"
            :aria-disabled="command.disabled || undefined"
            :data-command="command.id"
            class="flex cursor-pointer items-center gap-2 px-2.5 py-1 text-xs aria-selected:bg-accent aria-disabled:cursor-default aria-disabled:text-muted-foreground"
            @mousedown.prevent="command.disabled || runCommand(command.id)"
            @mousemove="index = i"
          >
            <span class="min-w-0 flex-1 truncate"
              ><template
                v-for="(run, r) in highlightRuns(
                  command.label,
                  command.indices,
                )"
                :key="r"
                ><mark
                  v-if="run.match"
                  class="bg-transparent font-semibold text-primary"
                  >{{ run.text }}</mark
                ><template v-else>{{ run.text }}</template></template
              ></span
            >
            <kbd
              v-if="command.shortcut"
              class="shrink-0 text-[10px] text-muted-foreground"
              >{{ shortcutLabel(command.shortcut) }}</kbd
            >
          </li>
          <li
            v-for="(match, i) in requestMatches"
            :id="`command-center-option-request-${match.id}`"
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
            <span class="min-w-0 flex-1 truncate"
              ><template
                v-for="(run, r) in highlightRuns(
                  match.label,
                  match.labelIndices,
                )"
                :key="r"
                ><mark
                  v-if="run.match"
                  class="bg-transparent font-semibold text-primary"
                  >{{ run.text }}</mark
                ><template v-else>{{ run.text }}</template></template
              ></span
            >
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
          {{ commandMode ? "No matching commands" : "No matching requests" }}
        </p>
      </div>
    </template>
  </div>
</template>
