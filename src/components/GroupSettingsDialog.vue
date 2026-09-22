<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { RequestGroup } from "../lib/groups";
import type { RequestSession } from "../lib/session";
import type { AuthorizationConfig } from "../lib/authorization";

const props = defineProps<{
  group: RequestGroup | null;
  groups: RequestGroup[];
  sessions: RequestSession[];
  open: boolean;
}>();

const emit = defineEmits<{
  (e: "update:open", value: boolean): void;
  (
    e: "save",
    groupId: number,
    changes: {
      name?: string;
      localAuth?: AuthorizationConfig | undefined;
      localDefinitions?: Record<string, string>;
    },
  ): void;
}>();

// ---------- local form state ----------
type AuthMode = "inherit" | "none" | "bearer" | "basic";

const localName = ref("");
const authMode = ref<AuthMode>("inherit");
const bearerToken = ref("");
const basicUsername = ref("");
const basicPassword = ref("");

const localTokenJson = ref("{}");
const tokenError = ref("");

function initFromProps() {
  if (!props.group) return;
  localName.value = props.group.name;

  const auth = props.group.localAuth;
  if (auth === undefined) {
    authMode.value = "inherit";
    bearerToken.value = "";
    basicUsername.value = "";
    basicPassword.value = "";
  } else if (auth.type === "none") {
    authMode.value = "none";
    bearerToken.value = "";
    basicUsername.value = "";
    basicPassword.value = "";
  } else if (auth.type === "bearer") {
    authMode.value = "bearer";
    bearerToken.value = auth.token;
    basicUsername.value = "";
    basicPassword.value = "";
  } else if (auth.type === "basic") {
    authMode.value = "basic";
    bearerToken.value = "";
    basicUsername.value = auth.username;
    basicPassword.value = auth.password;
  }

  localTokenJson.value = JSON.stringify(
    props.group.localDefinitions ?? {},
    null,
    2,
  );
  tokenError.value = "";
}

watch(
  () => [props.open, props.group] as const,
  ([open]) => {
    if (open) initFromProps();
  },
  { immediate: true },
);

// ---------- computed helpers ----------
const parentBreadcrumb = computed(() => {
  if (!props.group) return "";
  const parentId = props.group.parentId;
  if (parentId === null) return "(root)";
  const byId = new Map(props.groups.map((g) => [g.id, g]));
  const chain: string[] = [];
  let cursor: number | null = parentId;
  const seen = new Set<number>();
  while (cursor !== null) {
    if (seen.has(cursor)) break;
    seen.add(cursor);
    const g = byId.get(cursor);
    if (!g) break;
    chain.unshift(g.name);
    cursor = g.parentId;
  }
  return chain.join(" > ");
});

const effectiveAuth = computed(
  (): { label: string; source: "local" | "inherited"; from?: string } => {
    if (!props.group) return { label: "No auth", source: "local" };

    const local = props.group.localAuth;
    if (local !== undefined) {
      const typeLabel =
        local.type === "none"
          ? "No auth"
          : local.type === "bearer"
            ? "Bearer"
            : "Basic";
      return { label: `Local · ${typeLabel}`, source: "local" };
    }

    const byId = new Map(props.groups.map((g) => [g.id, g]));
    let cursor: number | null = props.group.parentId;
    const seen = new Set<number>();
    while (cursor !== null) {
      if (seen.has(cursor)) break;
      seen.add(cursor);
      const g = byId.get(cursor);
      if (!g) break;
      if (g.localAuth !== undefined) {
        const typeLabel =
          g.localAuth.type === "none"
            ? "No auth"
            : g.localAuth.type === "bearer"
              ? "Bearer"
              : "Basic";
        return {
          label: `Inherited from ${g.name} · ${typeLabel}`,
          source: "inherited",
          from: g.name,
        };
      }
      cursor = g.parentId;
    }
    return { label: "Local · No auth", source: "local" };
  },
);

function getDescendantGroupIds(
  groupId: number,
  groups: RequestGroup[],
): number[] {
  const result: number[] = [];
  const queue = [groupId];
  const byParent = new Map<number | null, RequestGroup[]>();
  for (const g of groups) {
    if (!byParent.has(g.parentId)) byParent.set(g.parentId, []);
    byParent.get(g.parentId)!.push(g);
  }
  while (queue.length) {
    const cur = queue.shift()!;
    const children = byParent.get(cur) ?? [];
    for (const c of children) {
      result.push(c.id);
      queue.push(c.id);
    }
  }
  return result;
}

const descendantGroupCount = computed(() => {
  if (!props.group) return 0;
  return getDescendantGroupIds(props.group.id, props.groups).length;
});

const descendantRequestCount = computed(() => {
  if (!props.group) return 0;
  const descIds = new Set([
    props.group.id,
    ...getDescendantGroupIds(props.group.id, props.groups),
  ]);
  return props.sessions.filter(
    (s) => s.groupId !== null && descIds.has(s.groupId),
  ).length;
});

// ---------- actions ----------
function parseLocalDefinitions(): Record<string, string> | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(localTokenJson.value);
  } catch {
    tokenError.value = "Enter a valid JSON object.";
    return null;
  }
  if (
    !parsed ||
    Array.isArray(parsed) ||
    typeof parsed !== "object" ||
    Object.values(parsed).some((value) => typeof value !== "string")
  ) {
    tokenError.value =
      "Token definitions must be a JSON object with string values.";
    return null;
  }
  if (Object.keys(parsed).some((name) => name.startsWith("_"))) {
    tokenError.value = "Token names must not start with _.";
    return null;
  }
  return parsed as Record<string, string>;
}

function buildLocalAuth(): AuthorizationConfig | undefined {
  if (authMode.value === "inherit") return undefined;
  if (authMode.value === "none") return { type: "none" };
  if (authMode.value === "bearer")
    return { type: "bearer", token: bearerToken.value };
  return {
    type: "basic",
    username: basicUsername.value,
    password: basicPassword.value,
  };
}

function handleSave() {
  if (!props.group) return;
  const localDefinitions = parseLocalDefinitions();
  if (!localDefinitions) return;
  const changes: {
    name?: string;
    localAuth?: AuthorizationConfig | undefined;
    localDefinitions?: Record<string, string>;
  } = {};
  changes.name = localName.value;
  changes.localAuth = buildLocalAuth();
  changes.localDefinitions = localDefinitions;
  emit("save", props.group.id, changes);
}

function handleCancel() {
  emit("update:open", false);
}
</script>

<template>
  <div
    v-if="open"
    class="dialog-backdrop"
    data-testid="group-settings-dialog"
    role="dialog"
    aria-modal="true"
    @keydown.esc="handleCancel"
  >
    <div class="dialog-content">
      <h2>Group Settings</h2>

      <!-- General Section -->
      <section>
        <h3>General</h3>
        <div class="field-row">
          <label for="group-name">Name</label>
          <input
            id="group-name"
            v-model="localName"
            data-testid="group-name-input"
            type="text"
          />
        </div>
        <div data-testid="parent-breadcrumb" class="info-row">
          <span>{{ parentBreadcrumb }}</span>
        </div>
        <div data-testid="effective-auth-label" class="info-row">
          {{ effectiveAuth.label }}
        </div>
        <div data-testid="descendant-counts" class="info-row">
          {{ descendantRequestCount }} requests · {{ descendantGroupCount }}
          groups
        </div>
      </section>

      <!-- Authorization Section -->
      <section>
        <h3>Authorization</h3>
        <div class="radio-group">
          <label>
            <input
              v-model="authMode"
              data-testid="auth-option-inherit"
              type="radio"
              name="auth-mode"
              value="inherit"
            />
            Inherit
          </label>
          <label>
            <input
              v-model="authMode"
              data-testid="auth-option-none"
              type="radio"
              name="auth-mode"
              value="none"
            />
            No auth
          </label>
          <label>
            <input
              v-model="authMode"
              data-testid="auth-option-bearer"
              type="radio"
              name="auth-mode"
              value="bearer"
            />
            Bearer
          </label>
          <label>
            <input
              v-model="authMode"
              data-testid="auth-option-basic"
              type="radio"
              name="auth-mode"
              value="basic"
            />
            Basic
          </label>
        </div>

        <div v-if="authMode === 'bearer'" class="field-row">
          <label>Token</label>
          <input
            v-model="bearerToken"
            data-testid="bearer-token-input"
            type="password"
            autocomplete="off"
          />
        </div>

        <div v-if="authMode === 'basic'" class="field-row-group">
          <div class="field-row">
            <label>Username</label>
            <input
              v-model="basicUsername"
              data-testid="basic-username-input"
              type="text"
              autocomplete="off"
            />
          </div>
          <div class="field-row">
            <label>Password</label>
            <input
              v-model="basicPassword"
              data-testid="basic-password-input"
              type="password"
              autocomplete="off"
            />
          </div>
        </div>

        <div
          v-if="authMode === 'inherit' && effectiveAuth.source === 'inherited'"
          class="info-row"
        >
          <em>{{ effectiveAuth.label }}</em>
        </div>
      </section>

      <!-- Tokens Section -->
      <section>
        <h3>Tokens</h3>
        <p class="help-text">
          Local tokens inherit through parent groups. Use
          <code v-text="'{{name}}'" /> to interpolate them. Token values can use
          other tokens or <code>&lt;&lt;NAME&gt;&gt;</code> to read NAME from
          Blink's process environment when a request is sent.
        </p>
        <label class="sr-only" for="local-token-json">Local tokens JSON</label>
        <textarea
          id="local-token-json"
          v-model="localTokenJson"
          data-local-token-json
          spellcheck="false"
          autocomplete="off"
        />
        <p v-if="tokenError" data-token-json-error class="error" role="alert">
          {{ tokenError }}
        </p>
      </section>

      <!-- Actions -->
      <div class="dialog-actions">
        <button data-testid="cancel-button" type="button" @click="handleCancel">
          Cancel
        </button>
        <button data-testid="save-button" type="button" @click="handleSave">
          Save
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.dialog-backdrop {
  position: fixed;
  inset: 0;
  z-index: 50;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgb(0 0 0 / 0.5);
}

.dialog-content {
  background: var(--background);
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 24px;
  min-width: 420px;
  max-width: 600px;
  max-height: 90dvh;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

h2 {
  font-size: 0.875rem;
  font-weight: 700;
  letter-spacing: 0.08em;
  margin-bottom: 4px;
}

h3 {
  font-size: 0.75rem;
  font-weight: 600;
  letter-spacing: 0.07em;
  color: var(--muted-foreground);
  margin-bottom: 8px;
  text-transform: uppercase;
}

h4 {
  font-size: 0.6875rem;
  font-weight: 600;
  color: var(--muted-foreground);
  margin: 10px 0 6px;
}

section {
  display: flex;
  flex-direction: column;
  gap: 6px;
  border-bottom: 1px solid var(--border);
  padding-bottom: 16px;
}

.field-row {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 0.75rem;
}

.field-row label {
  width: 80px;
  flex-shrink: 0;
  color: var(--muted-foreground);
}

.field-row input {
  flex: 1;
  height: 28px;
  padding: 0 8px;
  border: 1px solid var(--input);
  border-radius: 2px;
  background: var(--background);
  color: var(--foreground);
  font: 0.75rem var(--font-mono);
}

.field-row-group {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.radio-group {
  display: flex;
  gap: 14px;
  font-size: 0.75rem;
  flex-wrap: wrap;
}

.radio-group label {
  display: flex;
  align-items: center;
  gap: 5px;
  cursor: pointer;
}

.info-row {
  font-size: 0.6875rem;
  color: var(--muted-foreground);
  font-family: var(--font-mono);
}

.token-row {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 4px;
}

.token-row input {
  height: 26px;
  padding: 0 6px;
  border: 1px solid var(--input);
  border-radius: 2px;
  background: var(--background);
  color: var(--foreground);
  font: 0.6875rem var(--font-mono);
  min-width: 0;
  flex: 1;
}

.token-row button {
  flex-shrink: 0;
  color: var(--muted-foreground);
  font: 0.5625rem var(--font-mono);
}

.token-row button:hover {
  color: var(--destructive);
}

.add-btn {
  font: 0.5625rem var(--font-mono);
  color: var(--muted-foreground);
  margin-top: 4px;
}

.add-btn:hover {
  color: var(--primary);
}

.help-text {
  font-size: 0.625rem;
  color: var(--muted-foreground);
  line-height: 1.5;
}

.help-text code {
  font-family: var(--font-mono);
  background: var(--muted);
  padding: 0 3px;
}

[data-local-token-json] {
  min-height: 180px;
  resize: vertical;
  padding: 8px;
  border: 1px solid var(--input);
  border-radius: 2px;
  background: var(--background);
  color: var(--foreground);
  font: 0.75rem/1.5 var(--font-mono);
}

.error {
  font-size: 0.625rem;
  color: var(--destructive);
}

.dialog-actions {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  padding-top: 4px;
}

.dialog-actions button {
  height: 30px;
  padding: 0 14px;
  border: 1px solid var(--border);
  border-radius: 2px;
  font: 0.75rem var(--font-mono);
  color: var(--foreground);
  background: var(--background);
}

.dialog-actions button:hover {
  background: var(--accent);
}

.dialog-actions button:last-child {
  background: var(--primary);
  color: var(--primary-foreground);
  border-color: var(--primary);
}

.dialog-actions button:last-child:hover {
  opacity: 0.9;
}
</style>
