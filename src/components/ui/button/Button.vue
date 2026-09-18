<script setup lang="ts">
import { computed } from "vue";
import { Primitive } from "reka-ui";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const buttonVariants = cva(
  "inline-flex h-7 shrink-0 items-center justify-center gap-1.5 rounded-xs px-2.5 font-mono text-xs font-medium transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring disabled:pointer-events-none disabled:opacity-40 [@media(pointer:coarse)]:min-h-11",
  {
    variants: {
      variant: {
        default:
          "border border-primary bg-primary text-primary-foreground hover:bg-primary/90 active:bg-primary/80",
        secondary:
          "border border-input bg-secondary text-foreground hover:bg-accent active:bg-muted",
        ghost:
          "border border-transparent text-muted-foreground hover:bg-accent hover:text-foreground active:bg-muted",
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

const buttonClass = computed(() =>
  cn(buttonVariants({ variant: props.variant }), props.class),
);
</script>

<template>
  <Primitive
    as="button"
    :class="buttonClass"
    :disabled="disabled"
    :type="type ?? 'button'"
    :data-variant="variant ?? 'default'"
  >
    <slot />
  </Primitive>
</template>
