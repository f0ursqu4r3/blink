<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { DialogContent, DialogOverlay, DialogRoot, DialogTitle } from 'reka-ui'
import HelpTooltip from './HelpTooltip.vue'
import KeyValueEditor from './KeyValueEditor.vue'
import EnvironmentTokensEditor, {
  type EnvironmentColumn,
  type EnvironmentRow,
} from './EnvironmentTokensEditor.vue'
import { createEnvironment, nextEnvironmentColor, type Environment } from '../lib/environments'
import type { RequestGroup } from '../lib/groups'
import type { RequestSession } from '../lib/session'
import type { AuthorizationConfig } from '../lib/authorization'
import { canNestGroup } from '../lib/groups'
import { pair, type Method, type Pair } from '../lib/request'
import { definitionsToRows, rowsToDefinitions } from '../lib/definitions'
import {
  defaultPreferences,
  resolveNewRequestDefaults,
  type WorkspacePreferences,
} from '../lib/preferences'

const props = defineProps<{
  group: RequestGroup | null
  groups: RequestGroup[]
  sessions: RequestSession[]
  open: boolean
  preferences?: WorkspacePreferences
}>()

const emit = defineEmits<{
  (e: 'update:open', value: boolean): void
  (
    e: 'save',
    groupId: number,
    changes: {
      name?: string
      localAuth?: AuthorizationConfig | undefined
      localDefinitions?: Record<string, string>
      parentId?: number | null
      defaultMethod?: Method | undefined
      defaultUrl?: string | undefined
      environments?: Environment[] | undefined
    },
  ): void
}>()

// ---------- local form state ----------
type AuthMode = 'inherit' | 'none' | 'bearer' | 'basic'

const localName = ref('')
const authMode = ref<AuthMode>('inherit')
const bearerToken = ref('')
const basicUsername = ref('')
const basicPassword = ref('')
const parentId = ref<number | null>(null)
const defaultMethod = ref<Method | ''>('')
const defaultUrl = ref('')

const localTokenRows = ref<Pair[]>([])
/** Root groups edit tokens as a table with a column per environment. */
const isRoot = computed(() => parentId.value === null)
const envRows = ref<EnvironmentRow[]>([])
const envColumns = ref<EnvironmentColumn[]>([])
function toEnvRows(base: Record<string, string>, environments: Environment[]) {
  const keys = [
    ...new Set([
      ...Object.keys(base),
      ...environments.flatMap((environment) => Object.keys(environment.values)),
    ]),
  ]
  const rows = keys.map((key) => ({
    id: pair().id,
    key,
    base: base[key] ?? '',
    values: Object.fromEntries(
      environments.map((environment) => [environment.id, environment.values[key] ?? '']),
    ),
  }))
  return rows.length ? rows : [{ id: pair().id, key: '', base: '', values: {} }]
}
function addEnvRow() {
  envRows.value = [...envRows.value, { id: pair().id, key: '', base: '', values: {} }]
}
function addEnvColumn() {
  const environment = createEnvironment(
    envColumns.value.length ? `ENV ${envColumns.value.length + 1}` : 'DEV',
    nextEnvironmentColor(envColumns.value as Environment[]),
  )
  envColumns.value = [...envColumns.value, environment]
}
// Moving a group in or out of the root keeps the tokens typed so far.
watch(isRoot, (root) => {
  if (root)
    envRows.value = toEnvRows(
      Object.fromEntries(
        localTokenRows.value.filter((row) => row.key.trim()).map((row) => [row.key, row.value]),
      ),
      [],
    )
  else
    localTokenRows.value = definitionsToRows(
      Object.fromEntries(
        envRows.value.filter((row) => row.key.trim()).map((row) => [row.key, row.base]),
      ),
    )
})
const tokenError = ref('')
const formError = ref('')

function initFromProps() {
  if (!props.group) return
  localName.value = props.group.name
  parentId.value = props.group.parentId
  defaultMethod.value = props.group.defaultMethod ?? ''
  defaultUrl.value = props.group.defaultUrl ?? ''

  const auth = props.group.localAuth
  if (auth === undefined) {
    authMode.value = 'inherit'
    bearerToken.value = ''
    basicUsername.value = ''
    basicPassword.value = ''
  } else if (auth.type === 'none') {
    authMode.value = 'none'
    bearerToken.value = ''
    basicUsername.value = ''
    basicPassword.value = ''
  } else if (auth.type === 'bearer') {
    authMode.value = 'bearer'
    bearerToken.value = auth.token
    basicUsername.value = ''
    basicPassword.value = ''
  } else if (auth.type === 'basic') {
    authMode.value = 'basic'
    bearerToken.value = ''
    basicUsername.value = auth.username
    basicPassword.value = auth.password
  }

  localTokenRows.value = definitionsToRows(props.group.localDefinitions ?? {})
  envColumns.value = (props.group.environments ?? []).map(({ values: _values, ...column }) => ({
    ...column,
  }))
  envRows.value = toEnvRows(props.group.localDefinitions ?? {}, props.group.environments ?? [])
  tokenError.value = ''
  formError.value = ''
}

watch(
  () => [props.open, props.group] as const,
  ([open]) => {
    if (open) initFromProps()
  },
  { immediate: true },
)

// ---------- computed helpers ----------
const parentBreadcrumb = computed(() => {
  if (!props.group) return ''
  if (parentId.value === null) return '(root)'
  const byId = new Map(props.groups.map((g) => [g.id, g]))
  const chain: string[] = []
  let cursor: number | null = parentId.value
  const seen = new Set<number>()
  while (cursor !== null) {
    if (seen.has(cursor)) break
    seen.add(cursor)
    const g = byId.get(cursor)
    if (!g) break
    chain.unshift(g.name)
    cursor = g.parentId
  }
  return chain.join(' > ')
})

const effectiveAuth = computed(
  (): { label: string; source: 'local' | 'inherited'; from?: string } => {
    if (!props.group) return { label: 'No auth', source: 'local' }

    const local = buildLocalAuth()
    if (local !== undefined) {
      const typeLabel =
        local.type === 'none' ? 'No auth' : local.type === 'bearer' ? 'Bearer' : 'Basic'
      return { label: `Local · ${typeLabel}`, source: 'local' }
    }

    const byId = new Map(props.groups.map((g) => [g.id, g]))
    let cursor: number | null = parentId.value
    const seen = new Set<number>()
    while (cursor !== null) {
      if (seen.has(cursor)) break
      seen.add(cursor)
      const g = byId.get(cursor)
      if (!g) break
      if (g.localAuth !== undefined) {
        const typeLabel =
          g.localAuth.type === 'none'
            ? 'No auth'
            : g.localAuth.type === 'bearer'
              ? 'Bearer'
              : 'Basic'
        return {
          label: `Inherited from ${g.name} · ${typeLabel}`,
          source: 'inherited',
          from: g.name,
        }
      }
      cursor = g.parentId
    }
    return { label: 'Local · No auth', source: 'local' }
  },
)

function getDescendantGroupIds(groupId: number, groups: RequestGroup[]): number[] {
  const result: number[] = []
  const queue = [groupId]
  const byParent = new Map<number | null, RequestGroup[]>()
  for (const g of groups) {
    if (!byParent.has(g.parentId)) byParent.set(g.parentId, [])
    byParent.get(g.parentId)!.push(g)
  }
  while (queue.length) {
    const cur = queue.shift()!
    const children = byParent.get(cur) ?? []
    for (const c of children) {
      result.push(c.id)
      queue.push(c.id)
    }
  }
  return result
}

const descendantGroupCount = computed(() => {
  if (!props.group) return 0
  return getDescendantGroupIds(props.group.id, props.groups).length
})
const parentOptions = computed(() =>
  props.groups.filter(
    (candidate) => props.group && canNestGroup(props.groups, props.group.id, candidate.id),
  ),
)

const inheritedDefaults = computed(() =>
  resolveNewRequestDefaults(
    props.groups,
    parentId.value,
    props.preferences ?? defaultPreferences(),
  ),
)

const descendantRequestCount = computed(() => {
  if (!props.group) return 0
  const descIds = new Set([props.group.id, ...getDescendantGroupIds(props.group.id, props.groups)])
  return props.sessions.filter((s) => s.groupId !== null && descIds.has(s.groupId)).length
})

// ---------- actions ----------
/** Base tokens and environments from the table, or null with an error. */
function parseEnvironmentTable(): {
  localDefinitions: Record<string, string>
  environments: Environment[]
} | null {
  const names = new Set<string>()
  for (const column of envColumns.value) {
    const name = column.name.trim().toUpperCase()
    if (!name) {
      tokenError.value = 'Enter a name for each environment.'
      return null
    }
    if (names.has(name)) {
      tokenError.value = `Environment "${name}" is defined more than once.`
      return null
    }
    names.add(name)
  }
  const used = envRows.value.filter(
    (row) => row.key.trim() || row.base || Object.values(row.values).some(Boolean),
  )
  const result = rowsToDefinitions(
    used.map((row) => ({ ...pair(row.key, row.base || ' '), enabled: true })),
  )
  if ('error' in result) {
    tokenError.value = result.error
    return null
  }
  tokenError.value = ''
  const localDefinitions: Record<string, string> = {}
  for (const row of used) {
    const key = row.key.trim()
    // A token set only in environments has no base value.
    const inEnvironment = envColumns.value.some((c) => row.values[c.id])
    if (row.base || !inEnvironment) localDefinitions[key] = row.base
  }
  const environments = envColumns.value.map((column) => ({
    id: column.id,
    name: column.name.trim().toUpperCase(),
    color: column.color,
    ...(column.protected ? { protected: true } : {}),
    values: Object.fromEntries(
      used
        .filter((row) => row.values[column.id])
        .map((row) => [row.key.trim(), row.values[column.id]]),
    ),
  }))
  return { localDefinitions, environments }
}
function parseLocalDefinitions(): Record<string, string> | null {
  const result = rowsToDefinitions(localTokenRows.value)
  if ('error' in result) {
    tokenError.value = result.error
    return null
  }
  tokenError.value = ''
  return result.definitions
}

function buildLocalAuth(): AuthorizationConfig | undefined {
  if (authMode.value === 'inherit') return undefined
  if (authMode.value === 'none') return { type: 'none' }
  if (authMode.value === 'bearer') return { type: 'bearer', token: bearerToken.value }
  return {
    type: 'basic',
    username: basicUsername.value,
    password: basicPassword.value,
  }
}

function handleSave() {
  if (!props.group) return
  const name = localName.value.trim()
  if (!name) {
    formError.value = 'Enter a group name.'
    return
  }
  if (name.length > 80) {
    formError.value = 'Group name must be 80 characters or fewer.'
    return
  }
  if (defaultUrl.value.length > 65536) {
    formError.value = 'Initial URL must be 65536 characters or fewer.'
    return
  }
  let localDefinitions: Record<string, string> | null
  let environments: Environment[] | undefined
  if (isRoot.value) {
    const table = parseEnvironmentTable()
    if (!table) return
    localDefinitions = table.localDefinitions
    environments = table.environments
  } else {
    localDefinitions = parseLocalDefinitions()
    if (!localDefinitions) return
    // Environments stay stored but apply only to a root group.
    environments = props.group.environments
  }
  const changes: {
    name?: string
    localAuth?: AuthorizationConfig | undefined
    localDefinitions?: Record<string, string>
    parentId?: number | null
    defaultMethod?: Method | undefined
    defaultUrl?: string | undefined
    environments?: Environment[] | undefined
  } = {}
  changes.environments = environments
  changes.name = name
  changes.localAuth = buildLocalAuth()
  changes.localDefinitions = localDefinitions
  changes.parentId = parentId.value
  changes.defaultMethod = defaultMethod.value || undefined
  changes.defaultUrl = defaultUrl.value || undefined
  emit('save', props.group.id, changes)
  emit('update:open', false)
}

function handleCancel() {
  emit('update:open', false)
}
</script>

<template>
  <DialogRoot :open="open" @update:open="emit('update:open', $event)">
    <DialogOverlay class="fixed inset-0 z-50 bg-black/50" />
    <DialogContent
      class="fixed left-1/2 top-1/2 z-[60] flex max-h-[90dvh] w-[min(760px,calc(100vw-24px))] -translate-x-1/2 -translate-y-1/2 flex-col rounded-lg overflow-hidden border border-border bg-background"
      data-testid="group-settings-dialog"
      :aria-describedby="undefined"
      @pointer-down-outside.prevent>
      <form class="flex min-h-0 flex-col" @submit.prevent="handleSave">
        <header class="shrink-0 border-b border-border px-4 py-3">
          <DialogTitle class="text-sm font-bold tracking-[0.08em]">Group Settings</DialogTitle>
        </header>
        <div class="min-h-0 overflow-y-auto p-4 max-[390px]:p-3">
          <div class="flex flex-col gap-3">
            <!-- General Section -->
            <section class="flex flex-col gap-1.5 border-b border-border pb-4">
              <h3 class="text-xs font-semibold tracking-[0.07em] text-muted-foreground uppercase">
                General
              </h3>
              <div class="flex items-center gap-2.5 text-xs">
                <label class="w-20 shrink-0 text-muted-foreground" for="group-name">Name</label>
                <input
                  id="group-name"
                  v-model="localName"
                  class="min-w-0 flex-1 h-7 px-2 border border-input rounded-sm bg-background text-foreground font-mono text-xs"
                  data-testid="group-name-input"
                  type="text"
                  maxlength="80" />
              </div>
              <div
                class="text-[11px] text-muted-foreground font-mono"
                data-testid="parent-breadcrumb">
                <span>{{ parentBreadcrumb }}</span>
              </div>
              <div class="flex items-center gap-2.5 text-xs">
                <label class="w-20 shrink-0 text-muted-foreground" for="group-parent">Parent</label>
                <select
                  id="group-parent"
                  v-model="parentId"
                  data-testid="group-parent-input"
                  class="min-w-0 flex-1 h-7 border border-input rounded-sm bg-background font-mono text-xs">
                  <option :value="null">Root</option>
                  <option
                    v-for="candidate in parentOptions"
                    :key="candidate.id"
                    :value="candidate.id">
                    {{ candidate.name }}
                  </option>
                </select>
              </div>
              <div
                class="text-[11px] text-muted-foreground font-mono"
                data-testid="effective-auth-label">
                {{ effectiveAuth.label }}
              </div>
              <div
                class="text-[11px] text-muted-foreground font-mono"
                data-testid="descendant-counts">
                {{ descendantRequestCount }} requests · {{ descendantGroupCount }} groups
              </div>
            </section>

            <section class="flex flex-col gap-1.5 border-b border-border pb-4">
              <div class="flex items-center gap-1.5">
                <h3 class="text-xs font-semibold tracking-[0.07em] text-muted-foreground uppercase">
                  New request defaults
                </h3>
                <HelpTooltip
                  text="These values apply only to new requests in this group. Unset values inherit from parent groups, then application defaults. Existing requests and duplicates are unchanged.">
                  <button
                    type="button"
                    class="inline-flex size-5 items-center justify-center rounded-full border border-input text-[10px] text-muted-foreground hover:text-foreground"
                    aria-label="Group request defaults help">
                    ?
                  </button>
                </HelpTooltip>
              </div>
              <div class="flex items-center gap-2.5 text-xs">
                <label class="w-20 shrink-0 text-muted-foreground" for="group-default-method">
                  Method
                </label>
                <select
                  id="group-default-method"
                  v-model="defaultMethod"
                  class="min-w-0 flex-1 h-7 border border-input rounded-sm bg-background font-mono text-xs">
                  <option value="">Inherit ({{ inheritedDefaults.method }})</option>
                  <option
                    v-for="method in ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS']"
                    :key="method"
                    :value="method">
                    {{ method }}
                  </option>
                </select>
              </div>
              <div class="flex items-center gap-2.5 text-xs">
                <label class="w-20 shrink-0 text-muted-foreground" for="group-default-url">
                  Initial URL
                </label>
                <input
                  id="group-default-url"
                  v-model="defaultUrl"
                  class="min-w-0 flex-1 h-7 px-2 border border-input rounded-sm bg-background font-mono text-xs"
                  :placeholder="inheritedDefaults.url || 'No initial URL'"
                  maxlength="65536" />
              </div>
            </section>

            <!-- Authorization Section -->
            <section class="flex flex-col gap-1.5 border-b border-border pb-4">
              <h3 class="text-xs font-semibold tracking-[0.07em] text-muted-foreground uppercase">
                Authorization
              </h3>
              <div class="flex gap-3.5 text-xs flex-wrap">
                <label class="flex items-center gap-1.5 cursor-pointer">
                  <input
                    v-model="authMode"
                    data-testid="auth-option-inherit"
                    type="radio"
                    name="auth-mode"
                    value="inherit" />
                  Inherit
                </label>
                <label class="flex items-center gap-1.5 cursor-pointer">
                  <input
                    v-model="authMode"
                    data-testid="auth-option-none"
                    type="radio"
                    name="auth-mode"
                    value="none" />
                  No auth
                </label>
                <label class="flex items-center gap-1.5 cursor-pointer">
                  <input
                    v-model="authMode"
                    data-testid="auth-option-bearer"
                    type="radio"
                    name="auth-mode"
                    value="bearer" />
                  Bearer
                </label>
                <label class="flex items-center gap-1.5 cursor-pointer">
                  <input
                    v-model="authMode"
                    data-testid="auth-option-basic"
                    type="radio"
                    name="auth-mode"
                    value="basic" />
                  Basic
                </label>
              </div>

              <div v-if="authMode === 'bearer'" class="flex items-center gap-2.5 text-xs">
                <label for="group-bearer-token" class="w-20 shrink-0 text-muted-foreground">
                  Token
                </label>
                <input
                  id="group-bearer-token"
                  v-model="bearerToken"
                  class="flex-1 h-7 px-2 border border-input rounded-sm bg-background text-foreground font-mono text-xs"
                  data-testid="bearer-token-input"
                  type="password"
                  autocomplete="off" />
              </div>

              <div v-if="authMode === 'basic'" class="flex flex-col gap-1.5">
                <div class="flex items-center gap-2.5 text-xs">
                  <label for="group-basic-username" class="w-20 shrink-0 text-muted-foreground">
                    Username
                  </label>
                  <input
                    id="group-basic-username"
                    v-model="basicUsername"
                    class="flex-1 h-7 px-2 border border-input rounded-sm bg-background text-foreground font-mono text-xs"
                    data-testid="basic-username-input"
                    type="text"
                    autocomplete="off" />
                </div>
                <div class="flex items-center gap-2.5 text-xs">
                  <label for="group-basic-password" class="w-20 shrink-0 text-muted-foreground">
                    Password
                  </label>
                  <input
                    id="group-basic-password"
                    v-model="basicPassword"
                    class="flex-1 h-7 px-2 border border-input rounded-sm bg-background text-foreground font-mono text-xs"
                    data-testid="basic-password-input"
                    type="password"
                    autocomplete="off" />
                </div>
              </div>
            </section>

            <!-- Tokens Section -->
            <section class="flex flex-col gap-1.5 border-b border-border pb-4">
              <div class="flex items-center gap-1.5 mb-2">
                <h3 class="text-xs font-semibold tracking-[0.07em] text-muted-foreground uppercase">
                  Tokens
                </h3>
                <HelpTooltip
                  text="Local tokens inherit through parent groups. Use {{name}} to interpolate tokens. Use {{!NAME}} to read NAME from Blink's process environment when a request is sent.">
                  <button
                    class="grid place-items-center w-4 h-4 p-0 border border-input rounded-full text-muted-foreground bg-muted font-mono text-[10px] font-semibold leading-none hover:text-foreground hover:bg-accent focus-visible:text-foreground focus-visible:bg-accent"
                    data-token-help="local"
                    type="button"
                    aria-label="Token syntax help">
                    ?
                  </button>
                </HelpTooltip>
              </div>
              <div
                v-if="isRoot"
                class="overflow-hidden border border-input rounded-sm bg-background"
                data-local-tokens>
                <EnvironmentTokensEditor
                  v-model:rows="envRows"
                  v-model:columns="envColumns"
                  @add-row="addEnvRow"
                  @add-column="addEnvColumn" />
              </div>
              <div
                v-else
                class="overflow-hidden border border-input rounded-sm bg-background"
                data-local-tokens>
                <KeyValueEditor v-model="localTokenRows" label="Local tokens" hide-enabled />
              </div>
              <p v-if="isRoot" class="text-[0.6875rem] text-muted-foreground">
                Add an environment column to switch values from the Browser. An empty cell uses the
                base value. Nested groups follow this group's environment.
              </p>
              <p v-else-if="group?.environments?.length" class="text-[0.6875rem] text-warning">
                This group's environments apply only while it is a root group.
              </p>
              <p
                v-if="tokenError"
                class="text-[10px] text-destructive"
                data-token-error
                role="alert">
                {{ tokenError }}
              </p>
            </section>

            <p v-if="formError" role="alert" class="text-xs text-destructive">
              {{ formError }}
            </p>
          </div>
        </div>
        <div class="flex shrink-0 justify-end gap-2 border-t border-border p-3">
          <button
            class="h-7.5 px-3.5 border border-border rounded-sm font-mono text-xs text-foreground bg-background hover:bg-accent"
            data-testid="cancel-button"
            type="button"
            @click="handleCancel">
            Cancel
          </button>
          <button
            class="h-7.5 px-3.5 border border-primary rounded-sm font-mono text-xs bg-primary text-primary-foreground hover:opacity-90"
            data-testid="save-button"
            type="submit">
            Save
          </button>
        </div>
      </form>
    </DialogContent>
  </DialogRoot>
</template>
