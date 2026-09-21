<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useVirtualizer } from "@tanstack/vue-virtual";
import { parse } from "lossless-json";

type JsonContainer = Record<string, unknown> | unknown[];
type JsonRow = {
  id: string;
  key: string | null;
  value: unknown;
  depth: number;
  container: boolean;
  label: string;
};

const props = defineProps<{
  text: string;
  filter?: string;
  active?: boolean;
}>();

const scroll = defineModel<number>("scroll", { default: 0 });
const element = ref<HTMLElement>();
const collapsed = ref(new Set<string>());

const value = computed(() => parse(props.text));
const query = computed(() => props.filter?.trim().toLocaleLowerCase() ?? "");

function isContainer(value: unknown): value is JsonContainer {
  return (
    Boolean(value) &&
    typeof value === "object" &&
    !("isLosslessNumber" in (value as Record<string, unknown>))
  );
}

function childEntries(value: JsonContainer): [string, unknown][] {
  return Array.isArray(value)
    ? value.map((child, index) => [String(index), child])
    : Object.entries(value);
}

function valueLabel(value: unknown) {
  if (value === null) return "null";
  if (typeof value === "string") return JSON.stringify(value);
  if (typeof value === "boolean") return String(value);
  if (typeof value === "object" && "isLosslessNumber" in (value as object))
    return String(value);
  if (Array.isArray(value)) return `[${value.length}]`;
  if (isContainer(value)) return `{${Object.keys(value).length}}`;
  return String(value);
}

function addRows(
  value: unknown,
  key: string | null,
  id: string,
  depth: number,
): { matches: boolean; rows: JsonRow[] } {
  const label = `${key === null ? "root" : key} ${valueLabel(value)}`;
  const container = isContainer(value);
  const children = container
    ? childEntries(value).map(([childKey, child]) =>
        addRows(child, childKey, `${id}/${childKey}`, depth + 1),
      )
    : [];
  const matches =
    !query.value ||
    label.toLocaleLowerCase().includes(query.value) ||
    children.some((child) => child.matches);
  if (!matches) return { matches, rows: [] };

  const row: JsonRow = { id, key, value, depth, container, label };
  const showChildren =
    container && (Boolean(query.value) || !collapsed.value.has(id));
  return {
    matches,
    rows: [
      row,
      ...(showChildren ? children.flatMap((child) => child.rows) : []),
    ],
  };
}

const rows = computed(() => addRows(value.value, null, "$", 0).rows);
const virtualizer = useVirtualizer<HTMLElement, HTMLElement>(
  computed(() => ({
    count: rows.value.length,
    getScrollElement: () => element.value ?? null,
    estimateSize: () => 25,
    initialRect: { width: 0, height: 800 },
    observeElementRect: (_instance, callback) => {
      const target = element.value;
      const report = () =>
        callback({
          width: target?.clientWidth ?? 0,
          height: target?.clientHeight || 800,
        });
      report();
      if (!target || typeof ResizeObserver === "undefined") return;
      const observer = new ResizeObserver(report);
      observer.observe(target);
      return () => observer.disconnect();
    },
    overscan: 12,
  })),
);
const virtualRows = computed(() => virtualizer.value.getVirtualItems());

watch([element, () => props.active, () => props.text], () => {
  if (element.value && props.active !== false)
    element.value.scrollTop = scroll.value;
});
watch(query, () => {
  if (element.value) element.value.scrollTop = 0;
});

function saveScroll() {
  if (element.value && props.active !== false)
    scroll.value = element.value.scrollTop;
}

function toggle(row: JsonRow) {
  const next = new Set(collapsed.value);
  if (next.has(row.id)) next.delete(row.id);
  else next.add(row.id);
  collapsed.value = next;
}

function valueClass(value: unknown) {
  if (value === null) return "literal";
  if (typeof value === "string") return "string";
  if (typeof value === "boolean") return "literal";
  if (typeof value === "object" && value && "isLosslessNumber" in value)
    return "number";
  return "container";
}
</script>

<template>
  <div
    ref="element"
    class="json-tree"
    data-json-tree
    data-response-body
    data-virtual-scroller
    data-language="json"
    tabindex="0"
    aria-label="JSON response body"
    @scroll.passive="saveScroll"
  >
    <div
      class="tree-canvas"
      :style="{ height: `${virtualizer.getTotalSize()}px` }"
    >
      <div
        v-for="virtualRow in virtualRows"
        :key="String(virtualRow.key)"
        class="tree-row"
        :data-index="virtualRow.index"
        :style="{
          transform: `translateY(${virtualRow.start}px)`,
          paddingInlineStart: `${virtualRow.index >= 0 ? rows[virtualRow.index].depth * 18 + 12 : 12}px`,
        }"
      >
        <template v-if="rows[virtualRow.index]">
          <button
            v-if="rows[virtualRow.index].container"
            type="button"
            class="tree-toggle"
            :aria-label="`${collapsed.has(rows[virtualRow.index].id) ? 'Expand' : 'Collapse'} ${rows[virtualRow.index].key ?? 'root'}`"
            :aria-expanded="!collapsed.has(rows[virtualRow.index].id)"
            @click="toggle(rows[virtualRow.index])"
          >
            {{ collapsed.has(rows[virtualRow.index].id) ? "›" : "⌄" }}
          </button>
          <span v-else class="tree-spacer" aria-hidden="true" />
          <span v-if="rows[virtualRow.index].key !== null" class="key"
            >"{{ rows[virtualRow.index].key }}":
          </span>
          <span :class="valueClass(rows[virtualRow.index].value)">
            {{ valueLabel(rows[virtualRow.index].value) }}
          </span>
        </template>
      </div>
    </div>
    <p v-if="!rows.length" class="empty-tree">
      No JSON values match this filter.
    </p>
  </div>
</template>

<style scoped>
.json-tree {
  min-height: 0;
  flex: 1;
  overflow: auto;
  font: 0.8125rem/1.75 var(--font-mono);
  scrollbar-gutter: stable;
}
.tree-canvas {
  position: relative;
  min-width: max-content;
}
.tree-row {
  position: absolute;
  display: flex;
  align-items: center;
  width: max-content;
  min-width: 100%;
  height: 25px;
  gap: 7px;
  white-space: pre;
}
.tree-toggle,
.tree-spacer {
  display: inline-flex;
  width: 14px;
  height: 20px;
  flex: none;
  align-items: center;
  justify-content: center;
  color: var(--muted-foreground);
}
.tree-toggle:hover {
  color: var(--primary);
  background: var(--accent);
}
.key {
  color: var(--foreground);
}
.string {
  color: var(--success);
}
.number {
  color: var(--primary);
}
.literal {
  color: oklch(0.8 0.07 240);
}
.container {
  color: var(--muted-foreground);
}
.empty-tree {
  padding: 16px;
  color: var(--muted-foreground);
  font: 0.75rem var(--font-sans);
}
</style>
