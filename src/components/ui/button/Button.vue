<script setup lang="ts">
import { computed } from "vue";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const buttonVariants = cva(
  "inline-flex h-9 items-center justify-center gap-2 rounded-md px-3 text-sm font-medium transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan-400 disabled:pointer-events-none disabled:opacity-45",
  {
    variants: {
      variant: {
        default: "bg-cyan-400 text-slate-950 hover:bg-cyan-300 active:bg-cyan-500",
        secondary:
          "border border-slate-700 bg-slate-900 text-slate-200 hover:border-slate-600 hover:bg-slate-800 active:bg-slate-700",
        ghost: "text-slate-400 hover:bg-slate-800 hover:text-slate-100 active:bg-slate-700",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  },
);

type ButtonVariants = VariantProps<typeof buttonVariants>;

const props = defineProps<{
  class?: string;
  disabled?: boolean;
  type?: "button" | "submit" | "reset";
  variant?: ButtonVariants["variant"];
}>();

const buttonClass = computed(() => cn(buttonVariants({ variant: props.variant }), props.class));
</script>

<template>
  <button :class="buttonClass" :disabled="disabled" :type="type ?? 'button'">
    <slot />
  </button>
</template>
