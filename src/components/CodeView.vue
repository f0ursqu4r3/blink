<script setup lang="ts">
import { computed } from "vue";
import { JSON_HIGHLIGHT_LIMIT } from "@/lib/json";
const props = defineProps<{ text: string; json?: boolean; wrap?: boolean }>();
const lines = computed(() =>
  Array.from({ length: props.text.split("\n").length }, (_, i) => i + 1).join(
    "\n",
  ),
);
const tokens = computed(() => {
  if (!props.json || props.text.length > JSON_HIGHLIGHT_LIMIT)
    return [{ text: props.text, kind: "" }];
  const expression =
    /("(?:\\.|[^"\\])*"\s*:)|("(?:\\.|[^"\\])*")|(-?\b\d+(?:\.\d+)?(?:[eE][+-]?\d+)?\b)|(\b(?:true|false|null)\b)/g;
  const result: { text: string; kind: string }[] = [];
  let last = 0;
  for (const match of props.text.matchAll(expression)) {
    const index = match.index!;
    if (index > last)
      result.push({ text: props.text.slice(last, index), kind: "" });
    result.push({
      text: match[0],
      kind: match[1]
        ? "key"
        : match[2]
          ? "string"
          : match[3]
            ? "number"
            : "literal",
    });
    last = index + match[0].length;
  }
  result.push({ text: props.text.slice(last), kind: "" });
  return result;
});
</script>

<template>
  <div
    class="code-view"
    :class="{ wrapped: wrap }"
    tabindex="0"
    aria-label="Response body"
  >
    <pre v-if="!wrap" class="line-numbers" aria-hidden="true">{{ lines }}</pre>
    <pre
      class="source"
      data-response-body
    ><code><span v-for="(token, index) in tokens" :key="index" :class="token.kind">{{ token.text }}</span></code></pre>
  </div>
</template>

<style scoped>
.code-view {
  display: flex;
  align-items: flex-start;
  min-height: 0;
  flex: 1;
  overflow: auto;
  padding: 16px 0;
}
pre {
  margin: 0;
  font: 0.8125rem/1.75 var(--font-mono);
  tab-size: 2;
}
.line-numbers {
  color: var(--muted-foreground);
  opacity: 0.7;
  user-select: none;
  text-align: right;
  min-width: 42px;
  padding: 0 12px;
  border-right: 1px solid var(--border);
}
.source {
  padding: 0 16px;
  min-width: 0;
}
.wrapped .source {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  width: 100%;
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
</style>
