<script setup lang="ts">
import { computed, ref } from "vue";
import { ArrowLeft, GitCompare, Trash2 } from "lucide-vue-next";
import { Button } from "@/components/ui/button";
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
} from "@/components/ui/context-menu";
import {
  diffText,
  headerLines,
  relativeTime,
  type HistoryEntry,
} from "@/lib/history";
import { diffLines, foldDiff } from "@/lib/diff";
import { formatBytes } from "@/lib/request";
import { useClipboard } from "@/composables/useClipboard";

const props = defineProps<{ history: HistoryEntry[] }>();
const emit = defineEmits<{ clear: [] }>();
const { copy } = useClipboard();

/** Ids of the older and newer entry in the open comparison. */
const comparing = ref<[number, number] | null>(null);
const pair = computed(() => {
  if (!comparing.value) return null;
  const [before, after] = comparing.value.map((id) =>
    props.history.find((entry) => entry.id === id),
  );
  return before && after ? { before, after } : null;
});
const older = (entry: HistoryEntry) =>
  props.history[props.history.indexOf(entry) + 1];

function compare(before: HistoryEntry | undefined, after: HistoryEntry) {
  if (before) comparing.value = [before.id, after.id];
}
/** Compare with the send before it; the oldest compares with the latest. */
function open(entry: HistoryEntry) {
  const previous = older(entry);
  if (previous) compare(previous, entry);
  else if (entry !== props.history[0]) compare(entry, props.history[0]);
}
const sections = computed(() => {
  const value = pair.value;
  if (!value) return [];
  return [
    {
      title: "Headers",
      lines: diffLines(headerLines(value.before), headerLines(value.after)),
    },
    {
      title: "Body",
      lines: diffLines(diffText(value.before), diffText(value.after)),
    },
  ].map((section) => ({
    ...section,
    changed: section.lines?.some((line) => line.kind !== "same") ?? true,
    hunks: section.lines ? foldDiff(section.lines) : null,
  }));
});

const tone = (entry: HistoryEntry) =>
  entry.error || (entry.status ?? 0) >= 400
    ? "text-destructive"
    : (entry.status ?? 0) >= 300
      ? "text-warning"
      : "text-success";
const statusLabel = (entry: HistoryEntry) =>
  entry.error ? "ERR" : String(entry.status);
const clock = (entry: HistoryEntry) =>
  new Date(entry.sentAt).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
const path = (url: string) => {
  try {
    const parsed = new URL(url);
    return parsed.pathname + parsed.search;
  } catch {
    return url;
  }
};
</script>

<template>
  <div class="flex min-h-0 flex-1 flex-col" data-history>
    <template v-if="pair">
      <div
        class="flex min-h-9.5 shrink-0 items-center gap-2 border-b border-border px-2 font-mono text-[0.6875rem]"
      >
        <Button
          variant="ghost"
          class="size-7 shrink-0 p-0"
          aria-label="Back to history"
          title="Back to history"
          data-history-back
          @click="comparing = null"
        >
          <ArrowLeft :size="14" aria-hidden="true" />
        </Button>
        <span class="truncate" data-history-comparison>
          <span :class="tone(pair.before)">{{ statusLabel(pair.before) }}</span>
          {{ clock(pair.before) }}
          <span class="mx-1 text-muted-foreground">→</span>
          <span :class="tone(pair.after)">{{ statusLabel(pair.after) }}</span>
          {{ clock(pair.after) }}
        </span>
        <span class="ml-auto shrink-0 text-muted-foreground tabular-nums">
          {{ pair.before.durationMs }} → {{ pair.after.durationMs }} ms
        </span>
      </div>
      <div class="min-h-0 flex-1 overflow-auto pb-4" data-diff>
        <section v-for="section in sections" :key="section.title">
          <h3
            class="sticky top-0 z-1 flex items-center gap-2 border-b border-border bg-muted px-4 py-1.5 font-mono text-[0.625rem] tracking-[0.1em] text-muted-foreground uppercase"
          >
            {{ section.title }}
            <span v-if="!section.changed" class="normal-case tracking-normal"
              >· No changes</span
            >
          </h3>
          <p
            v-if="!section.hunks"
            class="px-4 py-2 text-xs text-muted-foreground"
          >
            Too many changes to compare.
          </p>
          <pre
            v-else-if="section.changed"
            class="m-0 font-mono text-[0.75rem] leading-[1.7]"
          ><template v-for="(hunk, h) in section.hunks" :key="h"><div
              v-if="'hidden' in hunk"
              class="border-y border-border bg-muted/50 px-4 text-[0.625rem] text-muted-foreground"
            >⋯ {{ hunk.hidden }} unchanged {{ hunk.hidden === 1 ? "line" : "lines" }}</div><div
              v-for="(line, l) in hunk.lines"
              v-else
              :key="l"
              class="diff-line flex whitespace-pre-wrap wrap-anywhere"
              :data-kind="line.kind"
            ><span class="w-10 shrink-0 pr-2 text-right text-muted-foreground select-none">{{ line.before ?? "" }}</span><span class="w-10 shrink-0 pr-2 text-right text-muted-foreground select-none">{{ line.after ?? "" }}</span><span class="w-4 shrink-0 select-none">{{ line.kind === "add" ? "+" : line.kind === "remove" ? "−" : "" }}</span><span class="min-w-0 flex-1 pr-4">{{ line.text }}</span></div></template></pre>
        </section>
      </div>
    </template>
    <template v-else>
      <div
        class="flex min-h-9.5 shrink-0 items-center justify-between border-b border-border px-4 font-mono text-[0.625rem] tracking-[0.1em] text-muted-foreground"
      >
        <span
          >{{ history.length }}
          {{ history.length === 1 ? "SEND" : "SENDS" }}</span
        >
        <Button
          variant="ghost"
          class="size-7 p-0"
          aria-label="Clear history"
          title="Clear history"
          data-history-clear
          :disabled="!history.length"
          @click="emit('clear')"
        >
          <Trash2 :size="13" aria-hidden="true" />
        </Button>
      </div>
      <p v-if="!history.length" class="p-4 text-xs text-muted-foreground">
        No sends yet. Each send of this request is kept here.
      </p>
      <ul class="min-h-0 flex-1 overflow-auto" role="list">
        <ContextMenu v-for="entry in history" :key="entry.id">
          <ContextMenuTrigger as-child>
            <li
              class="group flex min-h-9 cursor-pointer items-center gap-3 border-b border-border px-4 font-mono text-[0.6875rem] hover:bg-muted"
              :data-history-entry="entry.id"
              :title="`${entry.method} ${entry.url}${entry.error ? ` · ${entry.error}` : ''}`"
              @click="open(entry)"
            >
              <span class="w-8 shrink-0 font-semibold" :class="tone(entry)">{{
                statusLabel(entry)
              }}</span>
              <span class="flex min-w-0 flex-1 flex-col leading-[1.3]">
                <span class="truncate"
                  ><span class="method" :data-method="entry.method">{{
                    entry.method
                  }}</span>
                  {{ path(entry.url) }}</span
                >
                <span class="truncate text-[0.5625rem] text-muted-foreground"
                  >{{ clock(entry) }} · {{ relativeTime(entry.sentAt) }}</span
                >
              </span>
              <span class="shrink-0 text-muted-foreground tabular-nums"
                >{{ entry.durationMs }} ms</span
              >
              <span
                class="w-16 shrink-0 text-right text-muted-foreground tabular-nums"
                >{{ entry.error ? "" : formatBytes(entry.sizeBytes) }}</span
              >
              <button
                v-if="entry !== history[0]"
                type="button"
                class="invisible inline-flex size-6 shrink-0 items-center justify-center rounded text-muted-foreground group-hover:visible hover:bg-accent hover:text-foreground focus-visible:visible"
                aria-label="Compare with latest"
                title="Compare with latest"
                data-compare-latest
                @click.stop="compare(entry, history[0])"
              >
                <GitCompare :size="13" aria-hidden="true" />
              </button>
              <span v-else class="w-6 shrink-0" />
            </li>
          </ContextMenuTrigger>
          <ContextMenuContent>
            <ContextMenuItem
              :disabled="!older(entry)"
              data-testid="history-compare-previous"
              @select="compare(older(entry), entry)"
            >
              Compare with previous
            </ContextMenuItem>
            <ContextMenuItem
              :disabled="entry === history[0]"
              data-testid="history-compare-latest"
              @select="compare(entry, history[0])"
            >
              Compare with latest
            </ContextMenuItem>
            <ContextMenuSeparator />
            <ContextMenuItem @select="copy(entry.url)"
              >Copy URL</ContextMenuItem
            >
            <ContextMenuItem :disabled="!entry.body" @select="copy(entry.body)">
              Copy body
            </ContextMenuItem>
          </ContextMenuContent>
        </ContextMenu>
      </ul>
    </template>
  </div>
</template>

<style scoped>
/* Diff line colors from the theme's semantic tokens. */
.diff-line[data-kind="add"] {
  background: color-mix(in oklch, var(--success) 14%, transparent);
}
.diff-line[data-kind="add"] > :nth-child(3) {
  color: var(--success);
}
.diff-line[data-kind="remove"] {
  background: color-mix(in oklch, var(--destructive) 14%, transparent);
}
.diff-line[data-kind="remove"] > :nth-child(3) {
  color: var(--destructive);
}
</style>
