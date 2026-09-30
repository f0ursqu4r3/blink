<script setup lang="ts">
import { nextTick, ref } from "vue";
import { Plus, X } from "lucide-vue-next";
import { Button } from "@/components/ui/button";
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuCheckboxItem,
} from "@/components/ui/context-menu";
import { useClipboard } from "@/composables/useClipboard";
import TokenInput from "./TokenInput.vue";
import type { InterpolationContext } from "@/lib/interpolation";
import { pair, type Pair } from "@/lib/request";
const rows = defineModel<Pair[]>({ required: true });
const props = defineProps<{
  label: string;
  disabled?: boolean;
  /** Hide the Enabled column and menu items, for rows that are always on. */
  hideEnabled?: boolean;
  /** Tokens to color and suggest in values. */
  tokens?: InterpolationContext;
}>();
const editor = ref<HTMLElement | null>(null);
const { copyError, copy } = useClipboard();
function focusRow(index: number) {
  void nextTick(() => {
    const inputs =
      editor.value?.querySelectorAll<HTMLInputElement>("[data-row-name]");
    const target = inputs?.[Math.min(index, inputs.length - 1)];
    (
      target ?? editor.value?.querySelector<HTMLButtonElement>("[data-add-row]")
    )?.focus();
  });
}
function canEdit() {
  return !props.disabled;
}
function update(id: number, field: "key" | "value", value: string) {
  if (!canEdit()) return;
  rows.value = rows.value.map((row) =>
    row.id === id ? { ...row, [field]: value } : row,
  );
}
function duplicateRow(id: number) {
  if (!canEdit()) return;
  const idx = rows.value.findIndex((r) => r.id === id);
  if (idx < 0) return;
  const src = rows.value[idx];
  const copy = pair(src.key, src.value);
  copy.enabled = src.enabled;
  const next = [...rows.value];
  next.splice(idx + 1, 0, copy);
  rows.value = next;
  focusRow(idx + 1);
}
function removeRow(id: number) {
  if (!canEdit()) return;
  const index = rows.value.findIndex((row) => row.id === id);
  rows.value = rows.value.filter((r) => r.id !== id);
  focusRow(index);
}
function toggleRow(id: number) {
  if (!canEdit()) return;
  rows.value = rows.value.map((r) =>
    r.id === id ? { ...r, enabled: !r.enabled } : r,
  );
}
function toggleFromInput(id: number) {
  toggleRow(id);
}
function enableAll() {
  if (!canEdit()) return;
  rows.value = rows.value.map((r) => ({ ...r, enabled: true }));
}
function disableAll() {
  if (!canEdit()) return;
  rows.value = rows.value.map((r) => ({ ...r, enabled: false }));
}
function addRow() {
  if (!canEdit()) return;
  const next = [...rows.value, pair()];
  rows.value = next;
  focusRow(next.length - 1);
}
</script>

<template>
  <div ref="editor">
    <ContextMenu>
      <ContextMenuTrigger as-child data-testid="kv-table-ctx-trigger">
        <table class="w-full table-fixed border-collapse" :aria-label="label">
          <thead>
            <tr>
              <th
                v-if="!hideEnabled"
                class="w-8.5 text-center p-0 text-muted-foreground text-[0.625rem] uppercase tracking-widest font-medium h-8 bg-muted"
              >
                <span class="sr-only">Enabled</span>
              </th>
              <th
                scope="col"
                class="text-muted-foreground text-left text-[0.625rem] uppercase tracking-widest font-medium h-8 px-2.5 bg-muted border-border"
                :class="{ 'border-l': !hideEnabled }"
              >
                Name
              </th>
              <th
                scope="col"
                class="text-muted-foreground text-left text-[0.625rem] uppercase tracking-widest font-medium h-8 px-2.5 bg-muted border-l border-border"
              >
                Value
              </th>
              <th
                class="w-8.5 p-0 text-muted-foreground text-left text-[0.625rem] uppercase tracking-widest font-medium h-8 bg-muted border-l border-border"
              >
                <span class="sr-only">Remove</span>
              </th>
            </tr>
          </thead>
          <tbody>
            <ContextMenu v-for="(row, index) in rows" :key="row.id">
              <ContextMenuTrigger
                as-child
                :data-testid="`kv-row-ctx-trigger-${row.id}`"
              >
                <tr>
                  <td
                    v-if="!hideEnabled"
                    class="w-8.5 text-center p-0 h-8.5 border-b border-border pointer-coarse:h-11"
                  >
                    <input
                      type="checkbox"
                      class="accent-primary w-3 h-3"
                      :aria-label="`Enable ${label} row ${index + 1}`"
                      :checked="row.enabled"
                      :disabled="disabled"
                      @change="toggleFromInput(row.id)"
                    />
                  </td>
                  <td
                    class="h-8.5 border-b border-border pointer-coarse:h-11"
                    :class="{ 'border-l': !hideEnabled }"
                  >
                    <input
                      :aria-label="`${label} name ${index + 1}`"
                      data-row-name
                      @contextmenu.stop
                      :value="row.key"
                      placeholder="Name"
                      :disabled="disabled"
                      spellcheck="false"
                      autocomplete="off"
                      class="w-full h-8.25 bg-transparent border-0 px-2.5 font-mono text-xs rounded-none pointer-coarse:h-11 pointer-coarse:text-base"
                      :class="{ 'text-muted-foreground': !row.enabled }"
                      @input="
                        update(
                          row.id,
                          'key',
                          ($event.target as HTMLInputElement).value,
                        )
                      "
                    />
                  </td>
                  <td
                    class="h-8.5 border-b border-l border-border pointer-coarse:h-11"
                  >
                    <TokenInput
                      :aria-label="`${label} value ${index + 1}`"
                      @contextmenu.stop
                      :model-value="row.value"
                      :tokens="tokens"
                      placeholder="Value"
                      :disabled="disabled"
                      spellcheck="false"
                      autocomplete="off"
                      class="w-full h-8.25 bg-transparent border-0 px-2.5 font-mono text-xs rounded-none pointer-coarse:h-11 pointer-coarse:text-base"
                      :class="{ 'text-muted-foreground': !row.enabled }"
                      @update:model-value="update(row.id, 'value', $event)"
                    />
                  </td>
                  <td
                    class="w-8.5 p-0 h-8.5 border-b border-border border-l pointer-coarse:h-11"
                  >
                    <Button
                      variant="ghost"
                      class="size-7 shrink-0 p-0"
                      :aria-label="`Remove ${label} row ${index + 1}`"
                      :disabled="disabled"
                      @click="removeRow(row.id)"
                    >
                      <X :size="13" aria-hidden="true" />
                    </Button>
                  </td>
                </tr>
              </ContextMenuTrigger>
              <ContextMenuContent>
                <ContextMenuCheckboxItem
                  v-if="!hideEnabled"
                  data-testid="kv-row-ctx-toggle"
                  :model-value="row.enabled"
                  :disabled="disabled"
                  @select="toggleRow(row.id)"
                >
                  Enabled
                </ContextMenuCheckboxItem>
                <ContextMenuItem
                  data-testid="kv-row-ctx-duplicate"
                  :disabled="disabled"
                  @select="duplicateRow(row.id)"
                >
                  Duplicate row
                </ContextMenuItem>
                <ContextMenuSeparator />
                <ContextMenuItem
                  data-testid="kv-row-ctx-copy-name"
                  :disabled="!row.key"
                  @select="copy(row.key)"
                >
                  Copy name
                </ContextMenuItem>
                <ContextMenuItem
                  data-testid="kv-row-ctx-copy-value"
                  :disabled="!row.value"
                  @select="copy(row.value)"
                >
                  Copy value
                </ContextMenuItem>
                <ContextMenuSeparator />
                <ContextMenuItem
                  data-testid="kv-row-ctx-remove"
                  variant="destructive"
                  :disabled="disabled"
                  @select="removeRow(row.id)"
                >
                  Delete row
                </ContextMenuItem>
              </ContextMenuContent>
            </ContextMenu>
          </tbody>
        </table>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem
          data-testid="kv-ctx-add-row"
          :disabled="disabled"
          @select="addRow"
        >
          Add row
        </ContextMenuItem>
        <template v-if="!hideEnabled">
          <ContextMenuSeparator />
          <ContextMenuItem
            data-testid="kv-ctx-enable-all"
            :disabled="disabled"
            @select="enableAll"
          >
            Enable all
          </ContextMenuItem>
          <ContextMenuItem
            data-testid="kv-ctx-disable-all"
            :disabled="disabled"
            @select="disableAll"
          >
            Disable all
          </ContextMenuItem>
        </template>
      </ContextMenuContent>
    </ContextMenu>
    <p v-if="copyError" class="px-3 py-1 text-xs text-destructive" role="alert">
      {{ copyError }}
    </p>
    <Button
      variant="ghost"
      class="m-2"
      data-add-row
      :disabled="disabled"
      @click="addRow"
    >
      <Plus :size="13" aria-hidden="true" />Add row
    </Button>
  </div>
</template>
