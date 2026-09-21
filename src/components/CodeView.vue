<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useVirtualizer } from "@tanstack/vue-virtual";
import {
  highlightResponseLine,
  type ResponseLanguage,
} from "@/lib/response-content";

const props = withDefaults(
  defineProps<{
    text: string;
    language?: ResponseLanguage;
    filter?: string;
    wrap?: boolean;
    active?: boolean;
  }>(),
  { language: "plaintext" },
);
const scroll = defineModel<number>("scroll", { default: 0 });
const element = ref<HTMLElement>();
const filter = computed(() => props.filter?.trim().toLocaleLowerCase() ?? "");
const lines = computed(() =>
  props.text.split("\n").flatMap((text, index) =>
    !filter.value || text.toLocaleLowerCase().includes(filter.value)
      ? [
          {
            html: highlightResponseLine(text, props.language),
            number: index + 1,
          },
        ]
      : [],
  ),
);
const virtualizer = useVirtualizer<HTMLElement, HTMLElement>(
  computed(() => ({
    count: lines.value.length,
    getScrollElement: () => element.value ?? null,
    estimateSize: () => 23,
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
watch(filter, () => {
  if (element.value) element.value.scrollTop = 0;
});

function saveScroll() {
  if (element.value && props.active !== false)
    scroll.value = element.value.scrollTop;
}
function measureRow(node: unknown) {
  if (node instanceof HTMLElement) virtualizer.value.measureElement(node);
}
</script>

<template>
  <div
    ref="element"
    class="code-view"
    data-virtual-scroller
    :class="{ wrapped: wrap }"
    tabindex="0"
    aria-label="Response body"
    @scroll.passive="saveScroll"
  >
    <pre class="source" :data-language="language"><code
      class="virtual-canvas"
      data-response-body
      :data-language="language"
      :style="{ height: `${virtualizer.getTotalSize()}px` }"
      ><span
        v-for="virtualRow in virtualRows"
        :key="String(virtualRow.key)"
        :ref="measureRow"
        class="code-line"
        :data-index="virtualRow.index"
        :data-line="lines[virtualRow.index].number"
        :style="{ transform: `translateY(${virtualRow.start}px)` }"
        ><span
          class="line-source"
          v-html="lines[virtualRow.index].html"
        /></span></code
    ></pre>
    <p v-if="!lines.length" class="empty-code">
      No response lines match this filter.
    </p>
  </div>
</template>

<style scoped>
.code-view {
  min-height: 0;
  flex: 1;
  overflow: auto;
  padding: 16px 0;
  scrollbar-gutter: stable;
}
.source {
  margin: 0;
  min-width: max-content;
  font: 0.8125rem/1.75 var(--font-mono);
  tab-size: 2;
}
.virtual-canvas {
  position: relative;
  display: block;
}
.code-line {
  position: absolute;
  display: flex;
  width: max-content;
  min-width: 100%;
  height: 23px;
  white-space: pre;
}
.code-view:not(.wrapped) .code-line::before {
  width: 54px;
  flex: none;
  padding-right: 12px;
  border-right: 1px solid var(--border);
  color: var(--muted-foreground);
  opacity: 0.7;
  text-align: right;
  user-select: none;
  content: attr(data-line);
}
.line-source {
  padding: 0 16px;
}
.wrapped .source,
.wrapped .code-line {
  width: 100%;
  min-width: 0;
}
.wrapped .code-line {
  height: auto;
  min-height: 23px;
  white-space: pre-wrap;
}
.wrapped .line-source {
  overflow-wrap: anywhere;
}
:deep(.hljs-attr),
:deep(.hljs-attribute),
:deep(.hljs-property) {
  color: var(--foreground);
}
:deep(.hljs-string),
:deep(.hljs-regexp) {
  color: var(--success);
}
:deep(.hljs-number),
:deep(.hljs-literal),
:deep(.hljs-symbol) {
  color: var(--primary);
}
:deep(.hljs-keyword),
:deep(.hljs-tag),
:deep(.hljs-selector-tag),
:deep(.hljs-name) {
  color: oklch(0.8 0.07 240);
}
:deep(.hljs-comment),
:deep(.hljs-meta) {
  color: var(--muted-foreground);
}
.empty-code {
  padding: 0 16px;
  color: var(--muted-foreground);
  font-size: 0.75rem;
}
</style>
