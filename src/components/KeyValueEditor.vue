<script setup lang="ts">
import { Plus, X } from "lucide-vue-next";
import { Button } from "@/components/ui/button";
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
} from "@/components/ui/context-menu";
import { pair, type Pair } from "@/lib/request";
const rows = defineModel<Pair[]>({ required: true });
defineProps<{ label: string; disabled?: boolean }>();
function update(id: number, field: "key" | "value", event: Event) {
  rows.value = rows.value.map((row) =>
    row.id === id
      ? { ...row, [field]: (event.target as HTMLInputElement).value }
      : row,
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
    r.id === id ? { ...r, enabled: !r.enabled } : r,
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
        <table class="pair-table" :aria-label="label">
          <thead>
            <tr>
              <th class="check-cell"><span class="sr-only">Enabled</span></th>
              <th scope="col">Name</th>
              <th scope="col">Value</th>
              <th class="action-cell"><span class="sr-only">Remove</span></th>
            </tr>
          </thead>
          <tbody>
            <ContextMenu v-for="(row, index) in rows" :key="row.id">
              <ContextMenuTrigger
                as-child
                :data-testid="`kv-row-ctx-trigger-${row.id}`"
              >
                <tr :class="{ muted: !row.enabled }">
                  <td class="check-cell">
                    <input
                      type="checkbox"
                      :aria-label="`Enable ${label} row ${index + 1}`"
                      :checked="row.enabled"
                      :disabled="disabled"
                      @change="
                        rows = rows.map((r) =>
                          r.id === row.id ? { ...r, enabled: !r.enabled } : r,
                        )
                      "
                    />
                  </td>
                  <td>
                    <input
                      :aria-label="`${label} name ${index + 1}`"
                      :value="row.key"
                      placeholder="Name"
                      :disabled="disabled"
                      spellcheck="false"
                      autocomplete="off"
                      @input="update(row.id, 'key', $event)"
                    />
                  </td>
                  <td>
                    <input
                      :aria-label="`${label} value ${index + 1}`"
                      :value="row.value"
                      placeholder="Value"
                      :disabled="disabled"
                      spellcheck="false"
                      autocomplete="off"
                      @input="update(row.id, 'value', $event)"
                    />
                  </td>
                  <td class="action-cell">
                    <Button
                      variant="ghost"
                      class="size-7 shrink-0 p-0"
                      :aria-label="`Remove ${label} row ${index + 1}`"
                      :disabled="disabled"
                      @click="rows = rows.filter((r) => r.id !== row.id)"
                      ><X :size="13" aria-hidden="true"
                    /></Button>
                  </td>
                </tr>
              </ContextMenuTrigger>
              <ContextMenuContent>
                <ContextMenuItem
                  data-testid="kv-row-ctx-toggle"
                  @select="toggleRow(row.id)"
                  >{{ row.enabled ? "Disable" : "Enable" }}</ContextMenuItem
                >
                <ContextMenuItem
                  data-testid="kv-row-ctx-duplicate"
                  @select="duplicateRow(row.id)"
                  >Duplicate</ContextMenuItem
                >
                <ContextMenuSeparator />
                <ContextMenuItem
                  data-testid="kv-row-ctx-remove"
                  @select="removeRow(row.id)"
                  >Remove</ContextMenuItem
                >
              </ContextMenuContent>
            </ContextMenu>
          </tbody>
        </table>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem data-testid="kv-ctx-add-row" @select="addRow"
          >Add row</ContextMenuItem
        >
        <ContextMenuSeparator />
        <ContextMenuItem data-testid="kv-ctx-enable-all" @select="enableAll"
          >Enable all</ContextMenuItem
        >
        <ContextMenuItem data-testid="kv-ctx-disable-all" @select="disableAll"
          >Disable all</ContextMenuItem
        >
      </ContextMenuContent>
    </ContextMenu>
    <Button
      variant="ghost"
      class="m-2"
      :disabled="disabled"
      @click="rows = [...rows, pair()]"
      ><Plus :size="13" aria-hidden="true" />Add row</Button
    >
  </div>
</template>

<style scoped>
.pair-table {
  width: 100%;
  table-layout: fixed;
  border-collapse: collapse;
}
th {
  color: var(--muted-foreground);
  text-align: left;
  font-size: 0.625rem;
  text-transform: uppercase;
  letter-spacing: 0.1em;
  font-weight: 500;
  height: 32px;
  padding: 0 10px;
  background: var(--muted);
}
td {
  height: 34px;
  border-bottom: 1px solid var(--border);
}
.check-cell {
  width: 34px;
  text-align: center;
  padding: 0;
}
.action-cell {
  width: 34px;
  padding: 0;
}
td input:not([type="checkbox"]) {
  width: 100%;
  height: 33px;
  background: transparent;
  border: 0;
  padding: 0 10px;
  font-family: var(--font-mono);
  font-size: 0.75rem;
  border-radius: 0;
}
td + td {
  border-left: 1px solid var(--border);
}
input[type="checkbox"] {
  accent-color: var(--primary);
  width: 12px;
  height: 12px;
}
.muted input:not([type="checkbox"]) {
  color: var(--muted-foreground);
}
@media (pointer: coarse) {
  td,
  td input:not([type="checkbox"]) {
    height: 44px;
  }
  td input:not([type="checkbox"]) {
    font-size: 1rem;
  }
}
</style>
