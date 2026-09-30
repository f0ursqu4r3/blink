<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  ref,
  useAttrs,
  useId,
  watch,
} from "vue";
import { menuContent } from "@/components/ui/menu-classes";
import type { InterpolationContext } from "@/lib/interpolation";
import {
  applyTokenOption,
  matchTokenOptions,
  tokenHint,
  tokenOptions,
  tokenQueryAt,
  type TokenOption,
} from "@/lib/token-hints";
import {
  applyDisplayEdit,
  deleteIntoUnit,
  displayOffset,
  rawOffset,
  rawSlice,
  snapCaret,
  tokenDisplay,
  widenRange,
  type RawEdit,
} from "@/lib/token-display";

/**
 * A text input that shows each defined token as its value, colors token
 * references, and suggests tokens after `{{`. Backspace at the end of a
 * shown value turns it back into its `{{name}}` text. Attributes and classes
 * go to the native input. The input holds the shown text; a copy sits over
 * it and carries the colors, and the input keeps the caret and selection.
 */
defineOptions({ inheritAttrs: false });
const model = defineModel<string>({ default: "" });
const props = defineProps<{ tokens?: InterpolationContext }>();
const attrs = useAttrs();
const listId = useId();
const input = ref<HTMLInputElement>();
const backdrop = ref<HTMLElement>();

const display = computed(() => tokenDisplay(model.value, props.tokens));
const segments = computed(() => display.value.segments);
const hasTokens = computed(() => segments.value.some((s) => s.span.token));
const options = computed(() => tokenOptions(props.tokens));

const query = ref<{ from: number; query: string } | null>(null);
const active = ref(0);
const position = ref({ left: 0, top: 0 });
const matches = computed<TokenOption[]>(() =>
  query.value ? matchTokenOptions(options.value, query.value.query) : [],
);
const open = computed(() => matches.value.length > 0);

// The base layer gives some inputs a border that the backdrop lacks.
const backdropBorder = ref("0px");
watch(
  backdrop,
  () => {
    if (input.value)
      backdropBorder.value = getComputedStyle(input.value).borderLeftWidth;
    syncScroll();
  },
  { flush: "post" },
);

let measure: CanvasRenderingContext2D | null | undefined;
function textWidth(el: HTMLInputElement, text: string) {
  measure ??= document.createElement("canvas").getContext("2d");
  if (!measure) return 0;
  measure.font = getComputedStyle(el).font;
  return measure.measureText(text).width;
}

function syncScroll() {
  if (input.value && backdrop.value)
    backdrop.value.scrollLeft = input.value.scrollLeft;
}

/** Store a raw edit, then put the caret where it lands in the shown text. */
function commit(edit: RawEdit) {
  hint.value = null;
  model.value = edit.raw;
  void nextTick(() => {
    const el = input.value;
    if (!el) return;
    if (el.value !== display.value.text) el.value = display.value.text;
    const pos = displayOffset(segments.value, edit.caret);
    el.setSelectionRange(pos, pos);
    lastCaret = pos;
    refresh();
  });
}

function refresh() {
  syncScroll();
  const el = input.value;
  if (!el || !options.value.length || el.selectionStart !== el.selectionEnd) {
    query.value = null;
    return;
  }
  const next = tokenQueryAt(el.value, el.selectionStart ?? 0);
  if (next?.from !== query.value?.from) active.value = 0;
  query.value = next;
  if (!next) return;
  const rect = el.getBoundingClientRect();
  const padding = parseFloat(getComputedStyle(el).paddingLeft) || 0;
  const x = textWidth(el, el.value.slice(0, next.from - 2)) - el.scrollLeft;
  position.value = {
    left: Math.min(rect.left + padding + Math.max(0, x), rect.right),
    top: rect.bottom + 2,
  };
}

function close() {
  query.value = null;
}

// Keep the caret out of shown values. It jumps to the edge in the direction
// it moved.
let lastCaret = 0;
function onSelectionChange() {
  const el = input.value;
  if (!el || document.activeElement !== el) return;
  const pos = el.selectionStart ?? 0;
  if (pos === el.selectionEnd) {
    const snapped = snapCaret(segments.value, pos, lastCaret);
    if (snapped !== pos) el.setSelectionRange(snapped, snapped);
    lastCaret = snapped;
  }
  refresh();
}
function onFocus() {
  document.addEventListener("selectionchange", onSelectionChange);
}
function onBlur() {
  document.removeEventListener("selectionchange", onSelectionChange);
  close();
}
onBeforeUnmount(onBlur);

// Hint for the token under the pointer. The backdrop ignores the pointer, so
// the input finds the token from the pointer position.
const hint = ref<{ text: string; left: number; top: number } | null>(null);
function onPointerMove(event: PointerEvent) {
  const el = input.value;
  if (!el || !hasTokens.value || open.value) {
    hint.value = null;
    return;
  }
  const style = getComputedStyle(el);
  const rect = el.getBoundingClientRect();
  const start =
    rect.left +
    (parseFloat(style.borderLeftWidth) || 0) +
    (parseFloat(style.paddingLeft) || 0) -
    el.scrollLeft;
  for (const seg of segments.value) {
    const left = start + textWidth(el, el.value.slice(0, seg.from));
    const right = start + textWidth(el, el.value.slice(0, seg.to));
    if (event.clientX < left || event.clientX >= right) continue;
    hint.value = seg.span.token
      ? {
          text: tokenHint(seg.span, props.tokens),
          left: Math.max(rect.left, left),
          top: rect.bottom + 2,
        }
      : null;
    return;
  }
  hint.value = null;
}

function accept(option: TokenOption) {
  const el = input.value;
  if (!el || !query.value) return;
  const next = applyTokenOption(
    model.value,
    rawOffset(segments.value, query.value.from),
    rawOffset(segments.value, el.selectionStart ?? el.value.length),
    option.name,
  );
  close();
  commit({ raw: next.text, caret: next.cursor });
}

function onInput(event: Event) {
  const el = event.target as HTMLInputElement;
  commit(
    applyDisplayEdit(
      model.value,
      display.value,
      el.value,
      el.selectionEnd ?? el.value.length,
    ),
  );
}

function selection(el: HTMLInputElement) {
  return [el.selectionStart ?? 0, el.selectionEnd ?? 0] as const;
}

// Copy and cut take the raw text, so a pasted value keeps its references.
function onCopy(event: ClipboardEvent) {
  const el = input.value;
  if (!el || !event.clipboardData) return;
  const [start, end] = selection(el);
  if (start === end) return;
  event.clipboardData.setData(
    "text/plain",
    rawSlice(model.value, segments.value, start, end),
  );
  event.preventDefault();
}
function onCut(event: ClipboardEvent) {
  const el = input.value;
  onCopy(event);
  if (!el || !event.defaultPrevented) return;
  const [from, to] = widenRange(segments.value, ...selection(el));
  const rawFrom = rawOffset(segments.value, from);
  commit({
    raw:
      model.value.slice(0, rawFrom) +
      model.value.slice(rawOffset(segments.value, to)),
    caret: rawFrom,
  });
}

function onKeydown(event: KeyboardEvent) {
  if (!open.value) {
    const el = input.value;
    if (
      !el ||
      (event.key !== "Backspace" && event.key !== "Delete") ||
      event.altKey ||
      event.metaKey ||
      event.ctrlKey
    )
      return;
    const [start, end] = selection(el);
    if (start !== end) return;
    const edit = deleteIntoUnit(
      model.value,
      segments.value,
      start,
      event.key === "Backspace" ? "backward" : "forward",
    );
    if (!edit) return;
    event.preventDefault();
    commit(edit);
    return;
  }
  const count = matches.value.length;
  if (event.key === "ArrowDown") active.value = (active.value + 1) % count;
  else if (event.key === "ArrowUp")
    active.value = (active.value - 1 + count) % count;
  else if (event.key === "Enter" || event.key === "Tab")
    accept(matches.value[active.value]);
  else if (event.key === "Escape") close();
  else return;
  // Keep Enter from sending and Escape from closing a dialog.
  event.preventDefault();
  event.stopPropagation();
}

defineExpose({
  focus: () => input.value?.focus(),
  select: () => input.value?.select(),
});
</script>

<template>
  <div class="relative grid min-w-0 flex-1">
    <input
      ref="input"
      v-bind="attrs"
      class="col-start-1 row-start-1"
      :class="{ 'token-input-overlay': hasTokens }"
      :value="display.text"
      :role="options.length ? 'combobox' : undefined"
      :aria-autocomplete="options.length ? 'list' : undefined"
      :aria-expanded="options.length ? open : undefined"
      :aria-controls="open ? listId : undefined"
      :aria-activedescendant="open ? `${listId}-${active}` : undefined"
      @input="onInput"
      @keydown="onKeydown"
      @click="onSelectionChange"
      @scroll="syncScroll"
      @focus="onFocus"
      @blur="onBlur"
      @copy="onCopy"
      @cut="onCut"
      @pointermove="onPointerMove"
      @pointerleave="hint = null"
    />
    <div
      v-if="hasTokens"
      ref="backdrop"
      aria-hidden="true"
      class="pointer-events-none col-start-1 row-start-1 flex items-center overflow-hidden whitespace-pre"
      :class="attrs.class"
      :style="{
        borderStyle: 'solid',
        borderColor: 'transparent',
        borderWidth: backdropBorder,
      }"
      data-token-backdrop
    >
      <span
        v-for="(seg, index) in segments"
        :key="index"
        :data-token="seg.span.token"
        :data-token-value="seg.unit ? '' : undefined"
        :class="{
          'text-keyword': seg.span.token === 'resolved',
          'rounded-xs bg-keyword/15': seg.unit,
          'text-warning': seg.span.token === 'env',
          'text-destructive underline decoration-wavy decoration-destructive/60 underline-offset-3':
            seg.span.token === 'unresolved',
        }"
        >{{ seg.text }}</span
      >
    </div>
    <Teleport to="body">
      <ul
        v-if="open"
        :id="listId"
        role="listbox"
        aria-label="Tokens"
        class="fixed grid max-h-60 min-w-45 max-w-120 grid-cols-[auto_minmax(0,1fr)_auto]"
        :class="menuContent"
        :style="{ left: `${position.left}px`, top: `${position.top}px` }"
        data-token-suggestions
      >
        <li
          v-for="(option, index) in matches"
          :id="`${listId}-${index}`"
          :key="option.name"
          role="option"
          :aria-selected="index === active"
          class="col-span-3 grid h-6 cursor-default select-none grid-cols-subgrid items-center gap-x-4 rounded-sm px-2 font-mono aria-selected:bg-accent aria-selected:text-foreground"
          @mousedown.prevent="accept(option)"
          @mousemove="active = index"
        >
          <span class="shrink-0 text-keyword">{{ option.name }}</span>
          <span class="min-w-0 truncate text-muted-foreground">{{
            option.value
          }}</span>
          <span class="shrink-0 font-sans text-muted-foreground">
            {{ option.scope === "local" ? "Group" : "Global" }}
          </span>
        </li>
      </ul>
      <div
        v-if="hint"
        role="tooltip"
        class="pointer-events-none fixed z-50 max-w-120 truncate rounded-sm border border-border bg-popover px-2 py-1 font-mono text-xs text-popover-foreground shadow-menu"
        :style="{ left: `${hint.left}px`, top: `${hint.top}px` }"
        data-token-hint
      >
        {{ hint.text }}
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
/* The backdrop draws the text. */
.token-input-overlay {
  color: transparent;
  caret-color: var(--foreground);
}
</style>
