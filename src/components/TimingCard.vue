<script setup lang="ts">
import { computed } from "vue";
import {
  HoverCardContent,
  HoverCardPortal,
  HoverCardRoot,
  HoverCardTrigger,
} from "reka-ui";
import type { ResponseTiming } from "@/lib/request";
import { formatMs, timingPhases } from "@/lib/timing";

const props = defineProps<{ timing?: ResponseTiming; durationMs: number }>();
const phases = computed(() => (props.timing ? timingPhases(props.timing) : []));
const total = computed(() =>
  Math.max(
    props.durationMs,
    phases.value.reduce((sum, phase) => sum + phase.ms, 0),
    1,
  ),
);
const percent = (ms: number) => `${(ms / total.value) * 100}%`;
</script>

<template>
  <HoverCardRoot :open-delay="150" :close-delay="100">
    <HoverCardTrigger as-child>
      <button
        type="button"
        data-response-duration
        class="cursor-default tabular-nums underline decoration-dotted decoration-muted-foreground underline-offset-3"
        :aria-label="`${durationMs} milliseconds. Show timing`"
      >
        <slot />
      </button>
    </HoverCardTrigger>
    <HoverCardPortal>
      <HoverCardContent
        data-timing-card
        side="bottom"
        align="start"
        :side-offset="6"
        class="z-[70] w-80 rounded border border-border bg-secondary p-3 font-mono text-[0.6875rem] text-foreground shadow-[0_8px_24px_oklch(0_0_0/0.4)]"
      >
        <p class="mb-2 text-[0.5625rem] tracking-[0.1em] text-muted-foreground">
          TIMING
        </p>
        <p v-if="!phases.length" class="text-muted-foreground">
          This response has no phase timing. Send the request again.
        </p>
        <ul class="flex flex-col gap-1.5">
          <li
            v-for="phase in phases"
            :key="phase.id"
            class="grid grid-cols-[7.5rem_1fr_4rem] items-center gap-2"
            :data-phase="phase.id"
          >
            <span class="truncate text-muted-foreground">{{
              phase.label
            }}</span>
            <span class="relative h-2 rounded-sm bg-muted">
              <span
                class="absolute inset-y-0 min-w-px rounded-sm"
                :class="
                  phase.id === 'wait'
                    ? 'bg-primary'
                    : phase.id === 'download'
                      ? 'bg-success'
                      : 'bg-info'
                "
                :style="{
                  left: percent(phase.offsetMs),
                  width: percent(phase.ms),
                }"
              />
            </span>
            <span class="text-right tabular-nums">{{
              formatMs(phase.ms)
            }}</span>
          </li>
        </ul>
        <p
          class="mt-2 flex justify-between border-t border-border pt-2 tabular-nums"
        >
          <span class="text-muted-foreground">Total</span>
          <span>{{ formatMs(durationMs) }}</span>
        </p>
        <p
          v-if="timing && timing.dnsMs === undefined"
          class="mt-2 text-[0.625rem] leading-normal text-muted-foreground"
        >
          The browser hides connection phases for this origin. The desktop app
          shows all phases.
        </p>
      </HoverCardContent>
    </HoverCardPortal>
  </HoverCardRoot>
</template>
