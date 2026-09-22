<script setup lang="ts">
import { Plus, X } from 'lucide-vue-next';
import { Button } from '@/components/ui/button';
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
} from '@/components/ui/context-menu';
import { pair, type Pair } from '@/lib/request';
const rows = defineModel<Pair[]>({ required: true });
defineProps<{ label: string; disabled?: boolean }>();
function update(id: number, field: 'key' | 'value', event: Event) {
  rows.value = rows.value.map((row) =>
    row.id === id
      ? { ...row, [field]: (event.target as HTMLInputElement).value }
      : row
  );
}
function duplicateRow(id: number) {
  const idx = rows.value.findIndex((r) => r.id === id);
  if (idx < 0) return;
  const src = rows.value[idx];
  const copy = pair(src.key, src.value);
  copy.enabled = src.enabled;
  const next = [...rows.value];
  next.splice(idx + 1, 0, copy);
  rows.value = next;
}
function removeRow(id: number) {
  rows.value = rows.value.filter((r) => r.id !== id);
}
function toggleRow(id: number) {
  rows.value = rows.value.map((r) =>
    r.id === id ? { ...r, enabled: !r.enabled } : r
  );
}
function enableAll() {
  rows.value = rows.value.map((r) => ({ ...r, enabled: true }));
}
function disableAll() {
  rows.value = rows.value.map((r) => ({ ...r, enabled: false }));
}
function addRow() {
  rows.value = [...rows.value, pair()];
}
</script>

<template>
  <div>
    <ContextMenu>
      <ContextMenuTrigger as-child data-testid="kv-table-ctx-trigger">
        <table class="w-full table-fixed border-collapse" :aria-label="label">
          <thead>
            <tr>
              <th
                class="w-8.5 text-center p-0 text-muted-foreground text-[0.625rem] uppercase tracking-widest font-medium h-8 bg-muted"
              >
                <span class="sr-only">Enabled</span>
              </th>
              <th
                scope="col"
                class="text-muted-foreground text-left text-[0.625rem] uppercase tracking-widest font-medium h-8 px-2.5 bg-muted border-l border-border"
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
                    class="w-8.5 text-center p-0 h-8.5 border-b border-border pointer-coarse:h-11"
                  >
                    <input
                      type="checkbox"
                      class="accent-primary w-3 h-3"
                      :aria-label="`Enable ${label} row ${index + 1}`"
                      :checked="row.enabled"
                      :disabled="disabled"
                      @change="
                        rows = rows.map((r) =>
                          r.id === row.id ? { ...r, enabled: !r.enabled } : r
                        )
                      "
                    />
                  </td>
                  <td
                    class="h-8.5 border-b border-border border-l pointer-coarse:h-11"
                  >
                    <input
                      :aria-label="`${label} name ${index + 1}`"
                      :value="row.key"
                      placeholder="Name"
                      :disabled="disabled"
                      spellcheck="false"
                      autocomplete="off"
                      class="w-full h-8.25 bg-transparent border-0 px-2.5 font-mono text-xs rounded-none pointer-coarse:h-11 pointer-coarse:text-base"
                      :class="{ 'text-muted-foreground': !row.enabled }"
                      @input="update(row.id, 'key', $event)"
                    />
                  </td>
                  <td
                    class="h-8.5 border-b border-l border-border pointer-coarse:h-11"
                  >
                    <input
                      :aria-label="`${label} value ${index + 1}`"
                      :value="row.value"
                      placeholder="Value"
                      :disabled="disabled"
                      spellcheck="false"
                      autocomplete="off"
                      class="w-full h-8.25 bg-transparent border-0 px-2.5 font-mono text-xs rounded-none pointer-coarse:h-11 pointer-coarse:text-base"
                      :class="{ 'text-muted-foreground': !row.enabled }"
                      @input="update(row.id, 'value', $event)"
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
                      @click="rows = rows.filter((r) => r.id !== row.id)"
                    >
                      <X :size="13" aria-hidden="true" />
                    </Button>
                  </td>
                </tr>
              </ContextMenuTrigger>
              <ContextMenuContent>
                <ContextMenuItem
                  data-testid="kv-row-ctx-toggle"
                  @select="toggleRow(row.id)"
                >
                  {{ row.enabled ? 'Disable' : 'Enable' }}
                </ContextMenuItem>
                <ContextMenuItem
                  data-testid="kv-row-ctx-duplicate"
                  @select="duplicateRow(row.id)"
                >
                  Duplicate
                </ContextMenuItem>
                <ContextMenuSeparator />
                <ContextMenuItem
                  data-testid="kv-row-ctx-remove"
                  @select="removeRow(row.id)"
                >
                  Remove
                </ContextMenuItem>
              </ContextMenuContent>
            </ContextMenu>
          </tbody>
        </table>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem data-testid="kv-ctx-add-row" @select="addRow">
          Add row
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem data-testid="kv-ctx-enable-all" @select="enableAll">
          Enable all
        </ContextMenuItem>
        <ContextMenuItem data-testid="kv-ctx-disable-all" @select="disableAll">
          Disable all
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
    <Button
      variant="ghost"
      class="m-2"
      :disabled="disabled"
      @click="rows = [...rows, pair()]"
    >
      <Plus :size="13" aria-hidden="true" />Add row
    </Button>
  </div>
</template>
