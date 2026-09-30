<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { useVirtualizer } from "@tanstack/vue-virtual";
import { useFind } from "@/composables/useFind";
import {
  highlightResponseLine,
  type ResponseLanguage,
} from "@/lib/response-content";

const props = withDefaults(
  defineProps<{
    text: string;
    language?: ResponseLanguage;
    filter?: string;
    /** Highlight matches without hiding lines. */
    find?: string;
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
            text,
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
const finder = useFind({
  element,
  texts: () => lines.value.map((line) => line.text),
  query: () => props.find ?? "",
  active: () => props.active !== false,
  rendered: () => virtualRows.value,
  scrollToIndex: (index) =>
    virtualizer.value.scrollToIndex(index, { align: "center" }),
});
defineExpose({
  findCount: finder.count,
  findCurrent: finder.current,
  findStep: finder.step,
});

watch([element, () => props.text], () => {
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
// Vue can call the ref before the row is in the DOM. A detached row measures
// 0px, and correcting that later scrolls the list down one row at a time.
function measureRow(node: unknown) {
  if (!(node instanceof HTMLElement)) return;
  if (node.isConnected) virtualizer.value.measureElement(node);
  else
    void nextTick(() => {
      if (node.isConnected) virtualizer.value.measureElement(node);
    });
}
</script>

<template>
  <div
    ref="element"
    class="min-h-0 flex-1 overflow-auto py-4 scrollbar-gutter-stable"
    data-virtual-scroller
    :class="{ wrapped: wrap }"
    tabindex="0"
    aria-label="Response body"
    @scroll.passive="saveScroll"
  >
    <pre
      class="m-0 min-w-max font-mono text-[0.8125rem] leading-[1.75] tab-2 in-[.wrapped]:w-full in-[.wrapped]:min-w-0"
      :data-language="language"
    ><code
      class="relative block"
      data-response-body
      :data-language="language"
      :style="{ height: `${virtualizer.getTotalSize()}px` }"
      ><span
        v-for="virtualRow in virtualRows"
        :key="String(virtualRow.key)"
        :ref="measureRow"
        class="code-line absolute flex w-max min-w-full h-5.75 whitespace-pre in-[.wrapped]:w-full in-[.wrapped]:min-w-0 in-[.wrapped]:h-auto in-[.wrapped]:min-h-5.75 in-[.wrapped]:whitespace-pre-wrap"
        :data-index="virtualRow.index"
        :data-line="lines[virtualRow.index].number"
        :style="{ transform: `translateY(${virtualRow.start}px)` }"
        ><span
          class="line-source px-4 in-[.wrapped]:wrap-anywhere"
          v-html="lines[virtualRow.index].html"
        /></span></code
    ></pre>
    <p v-if="!lines.length" class="px-4 text-muted-foreground text-xs">
      No response lines match this filter.
    </p>
  </div>
</template>

<style scoped>
/* Line numbers via pseudo-element — requires content: attr() and cannot be expressed as Tailwind utilities */
.code-line:not(.wrapped *):before {
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
</style>
