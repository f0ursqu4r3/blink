<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useVirtualizer } from '@tanstack/vue-virtual';
import { parse } from 'lossless-json';
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
} from '@/components/ui/context-menu';
import { useClipboard } from '@/composables/useClipboard';

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
  wrap?: boolean;
}>();

const scroll = defineModel<number>('scroll', { default: 0 });
const element = ref<HTMLElement>();
const collapsed = ref(new Set<string>());

const value = computed(() => parse(props.text));
const query = computed(() => props.filter?.trim().toLocaleLowerCase() ?? '');

function isContainer(value: unknown): value is JsonContainer {
  return (
    Boolean(value) &&
    typeof value === 'object' &&
    !('isLosslessNumber' in (value as Record<string, unknown>))
  );
}

function childEntries(value: JsonContainer): [string, unknown][] {
  return Array.isArray(value)
    ? value.map((child, index) => [String(index), child])
    : Object.entries(value);
}

function valueLabel(value: unknown) {
  if (value === null) return 'null';
  if (typeof value === 'string') return JSON.stringify(value);
  if (typeof value === 'boolean') return String(value);
  if (typeof value === 'object' && 'isLosslessNumber' in (value as object))
    return String(value);
  if (Array.isArray(value)) return `[${value.length}]`;
  if (isContainer(value)) return `{${Object.keys(value).length}}`;
  return String(value);
}

function addRows(
  value: unknown,
  key: string | null,
  id: string,
  depth: number
): { matches: boolean; rows: JsonRow[] } {
  const label = `${key === null ? 'root' : key} ${valueLabel(value)}`;
  const container = isContainer(value);
  const children = container
    ? childEntries(value).map(([childKey, child]) =>
        addRows(child, childKey, `${id}/${childKey}`, depth + 1)
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

const rows = computed(() => addRows(value.value, null, '$', 0).rows);
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
      if (!target || typeof ResizeObserver === 'undefined') return;
      const observer = new ResizeObserver(report);
      observer.observe(target);
      return () => observer.disconnect();
    },
    overscan: 12,
  }))
);
const virtualRows = computed(() => virtualizer.value.getVirtualItems());

watch([element, () => props.text], () => {
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
  if (value === null) return 'text-[oklch(0.8_0.07_240)]';
  if (typeof value === 'string') return 'text-success';
  if (typeof value === 'boolean') return 'text-[oklch(0.8_0.07_240)]';
  if (typeof value === 'object' && value && 'isLosslessNumber' in value)
    return 'text-primary';
  return 'text-muted-foreground';
}
function measureRow(node: unknown) {
  if (node instanceof HTMLElement) virtualizer.value.measureElement(node);
}

const { copy } = useClipboard();
const contextRow = ref<JsonRow | null>(null);

function openContextMenu(row: JsonRow) {
  contextRow.value = row;
}

function copyPath() {
  if (!contextRow.value) return;
  void copy(contextRow.value.id);
}

function copyValue() {
  if (!contextRow.value) return;
  void copy(valueLabel(contextRow.value.value));
}
</script>

<template>
  <div
    ref="element"
    class="min-h-0 flex-1 overflow-auto font-mono text-[0.8125rem] leading-[1.75] scrollbar-gutter-stable"
    :class="{ wrapped: wrap }"
    data-json-tree
    data-response-body
    data-virtual-scroller
    data-language="json"
    tabindex="0"
    aria-label="JSON response body"
    @scroll.passive="saveScroll"
  >
    <ContextMenu>
      <ContextMenuTrigger as-child>
        <div
          class="relative min-w-max in-[.wrapped]:min-w-0"
          :style="{ height: `${virtualizer.getTotalSize()}px` }"
        >
          <div
            v-for="virtualRow in virtualRows"
            :key="String(virtualRow.key)"
            :ref="measureRow"
            class="tree-row absolute flex items-center w-max min-w-full min-h-6.25 gap-1.75 whitespace-pre in-[.wrapped]:items-start in-[.wrapped]:w-full in-[.wrapped]:h-auto in-[.wrapped]:whitespace-pre-wrap"
            :data-index="virtualRow.index"
            :style="{
              transform: `translateY(${virtualRow.start}px)`,
              paddingInlineStart: `${virtualRow.index >= 0 ? rows[virtualRow.index].depth * 18 + 12 : 12}px`,
            }"
            @contextmenu="
              rows[virtualRow.index] && openContextMenu(rows[virtualRow.index])
            "
          >
            <template v-if="rows[virtualRow.index]">
              <button
                v-if="rows[virtualRow.index].container"
                type="button"
                class="inline-flex w-3.5 h-5 flex-none items-center justify-center text-muted-foreground hover:text-primary hover:bg-accent"
                :aria-label="`${collapsed.has(rows[virtualRow.index].id) ? 'Expand' : 'Collapse'} ${rows[virtualRow.index].key ?? 'root'}`"
                :aria-expanded="!collapsed.has(rows[virtualRow.index].id)"
                @click="toggle(rows[virtualRow.index])"
              >
                {{ collapsed.has(rows[virtualRow.index].id) ? '›' : '⌄' }}
              </button>
              <span
                v-else
                class="inline-flex w-3.5 h-5 flex-none items-center justify-center text-muted-foreground"
                aria-hidden="true"
              />
              <span
                v-if="rows[virtualRow.index].key !== null"
                class="text-foreground in-[.wrapped]:wrap-anywhere"
              >
                "{{ rows[virtualRow.index].key }}":
              </span>
              <span
                :class="[
                  valueClass(rows[virtualRow.index].value),
                  'in-[.wrapped]:wrap-anywhere',
                ]"
              >
                {{ valueLabel(rows[virtualRow.index].value) }}
              </span>
            </template>
          </div>
        </div>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem data-testid="ctx-copy-path" @select="copyPath">
          Copy path
        </ContextMenuItem>
        <ContextMenuItem data-testid="ctx-copy-value" @select="copyValue">
          Copy value
        </ContextMenuItem>
        <template v-if="contextRow?.container">
          <ContextMenuSeparator data-testid="ctx-separator" />
          <ContextMenuItem
            data-testid="ctx-toggle"
            @select="contextRow && toggle(contextRow)"
          >
            {{
              contextRow && collapsed.has(contextRow.id) ? 'Expand' : 'Collapse'
            }}
          </ContextMenuItem>
        </template>
      </ContextMenuContent>
    </ContextMenu>
    <p v-if="!rows.length" class="p-4 text-muted-foreground font-sans text-xs">
      No JSON values match this filter.
    </p>
  </div>
</template>
