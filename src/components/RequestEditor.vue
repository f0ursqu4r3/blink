<script setup lang="ts">
import { computed, ref, useId } from "vue";
import { TabsRoot, TabsList, TabsTrigger, TabsContent } from "reka-ui";
import { Braces, KeyRound } from "lucide-vue-next";
import HelpTooltip from "./HelpTooltip.vue";
import { Button } from "@/components/ui/button";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import KeyValueEditor from "./KeyValueEditor.vue";
import { activePairs, supportsBody, type Draft } from "@/lib/request";
import type { AuthorizationConfig } from "@/lib/authorization";
import { formatJson } from "@/lib/json";
import { formatGraphql } from "@/lib/graphql";
const draft = defineModel<Draft>({ required: true });
const props = defineProps<{
  busy: boolean;
  effectiveAuth?: AuthorizationConfig;
  inheritedSource?: string;
}>();
const id = useId();
const tab = defineModel<string>("tab", { default: "query" });
const formatError = ref("");
const bodyPlaceholder = computed(() =>
  draft.value.bodyMode === "json"
    ? '{\n  "key": "value"\n}'
    : draft.value.bodyMode === "graphql"
      ? "query {\n  viewer {\n    id\n  }\n}"
      : "Request body",
);
const variablesPlaceholder = '{\n  "id": "1"\n}';
const bodyAllowed = computed(() => supportsBody(draft.value.method));

/** Resolved "what is selected in the auth dropdown" */
const authSelectValue = computed(() => {
  if (draft.value.localAuth === undefined) return "inherit";
  return draft.value.localAuth.type === "bearer"
    ? "bearer"
    : draft.value.localAuth.type === "basic"
      ? "basic"
      : "none";
});

function setAuthType(value: string) {
  if (props.busy) return;
  if (value === "inherit") {
    draft.value = { ...draft.value, localAuth: undefined };
  } else if (value === "none") {
    draft.value = { ...draft.value, localAuth: { type: "none" } };
  } else if (value === "bearer") {
    draft.value = {
      ...draft.value,
      localAuth: { type: "bearer", token: draft.value.token ?? "" },
    };
  } else if (value === "basic") {
    draft.value = {
      ...draft.value,
      localAuth: {
        type: "basic",
        username: draft.value.username ?? "",
        password: draft.value.password ?? "",
      },
    };
  }
}

const authBadgeCount = computed(() => {
  // Show badge if effective auth (inherited or local) is not none
  const effective = draft.value.localAuth ?? props.effectiveAuth;
  return effective && effective.type !== "none" ? 1 : 0;
});

const tabs = computed(() => [
  { id: "query", label: "Query", count: activePairs(draft.value.query).length },
  {
    id: "headers",
    label: "Headers",
    count: activePairs(draft.value.headers).length,
  },
  {
    id: "body",
    label: "Body",
    count: bodyAllowed.value && draft.value.bodyMode !== "none" ? 1 : 0,
  },
  { id: "auth", label: "Auth", count: authBadgeCount.value },
]);
const formattable = computed(
  () => draft.value.bodyMode === "json" || draft.value.bodyMode === "graphql",
);
async function formatBody() {
  if (props.busy) return;
  if (draft.value.bodyMode === "graphql") return formatGraphqlBody();
  try {
    draft.value.body = formatJson(draft.value.body);
    formatError.value = "";
  } catch {
    formatError.value = "Invalid JSON. The body was not changed.";
  }
}
async function formatGraphqlBody() {
  const body = draft.value.body;
  const variables = draft.value.variables ?? "";
  let formattedVariables = variables;
  if (variables.trim()) {
    try {
      formattedVariables = formatJson(variables);
    } catch {
      formatError.value = "Invalid JSON variables. The body was not changed.";
      return;
    }
  }
  let formattedBody: string;
  try {
    formattedBody = await formatGraphql(body);
  } catch {
    formatError.value = "Invalid GraphQL. The body was not changed.";
    return;
  }
  // Skip the update if the user edited while the formatter loaded.
  if (draft.value.body !== body || (draft.value.variables ?? "") !== variables)
    return;
  draft.value.body = formattedBody;
  draft.value.variables = formattedVariables;
  formatError.value = "";
}
function clearBody() {
  if (props.busy) return;
  draft.value.body = "";
  formatError.value = "";
}
</script>

<template>
  <section
    class="flex flex-col min-w-0 min-h-0"
    :aria-labelledby="`${id}-heading`"
  >
    <header
      class="h-9 shrink-0 px-4 flex items-center justify-between border-b border-border bg-muted font-mono text-[0.625rem] tracking-[0.12em]"
    >
      <h2 class="font-semibold uppercase text-[0.6875rem]">
        <span class="text-primary mr-2.5">01</span> Request
      </h2>
      <span class="text-muted-foreground">
        {{ busy ? "SENDING" : "COMPOSE" }}
      </span>
    </header>
    <TabsRoot v-model="tab" class="flex-1 min-h-0 flex flex-col">
      <TabsList
        class="flex shrink-0 border-b border-border px-2"
        aria-label="Request options"
      >
        <TabsTrigger
          v-for="item in tabs"
          :key="item.id"
          :value="item.id"
          class="tab-trigger h-9.5 px-3 border-b border-transparent text-xs text-muted-foreground flex gap-1.75 items-center hover:text-foreground hover:bg-muted data-[state=active]:text-primary data-[state=active]:border-b-primary pointer-coarse:min-h-11"
        >
          {{ item.label }}
          <span
            v-if="item.count"
            class="tab-count font-mono text-[0.625rem] text-muted-foreground"
          >
            {{ item.count }}
          </span>
        </TabsTrigger>
      </TabsList>
      <TabsContent
        value="query"
        class="flex-1 min-h-0 overflow-auto -outline-offset-2"
      >
        <KeyValueEditor v-model="draft.query" label="Query" :disabled="busy" />
        <div class="px-4 py-2">
          <HelpTooltip
            text="Enabled rows are appended to the URL. Duplicate keys are preserved."
          >
            <button
              type="button"
              class="text-[0.6875rem] text-muted-foreground underline decoration-dotted underline-offset-3"
            >
              Query help
            </button>
          </HelpTooltip>
        </div>
      </TabsContent>
      <TabsContent
        value="headers"
        class="flex-1 min-h-0 overflow-auto -outline-offset-2"
      >
        <KeyValueEditor
          v-model="draft.headers"
          label="Header"
          :disabled="busy"
        />
        <div class="px-4 py-2">
          <HelpTooltip
            text="Body mode sets Content-Type unless a header overrides it."
          >
            <button
              type="button"
              class="text-[0.6875rem] text-muted-foreground underline decoration-dotted underline-offset-3"
            >
              Header help
            </button>
          </HelpTooltip>
        </div>
      </TabsContent>
      <TabsContent
        value="body"
        class="flex-1 min-h-0 overflow-auto -outline-offset-2 data-[state=active]:flex data-[state=active]:flex-col"
      >
        <ContextMenu>
          <ContextMenuTrigger as-child>
            <div
              class="flex items-center px-3 py-2 gap-2.5 border-b border-border text-muted-foreground text-xs"
              data-body-actions
            >
              <label :for="`${id}-body-mode`">Body</label>
              <select
                :id="`${id}-body-mode`"
                v-model="draft.bodyMode"
                :disabled="busy"
                class="h-7 px-2 font-mono text-xs pointer-coarse:min-h-11 pointer-coarse:text-base"
                @contextmenu.stop
              >
                <option value="none">None</option>
                <option value="json">JSON</option>
                <option value="text">Text</option>
                <option value="graphql">GraphQL</option>
              </select>
              <Button
                v-if="formattable"
                variant="ghost"
                class="ml-auto"
                :disabled="busy || !draft.body"
                @click="formatBody"
              >
                <Braces :size="13" aria-hidden="true" />Format
              </Button>
            </div>
          </ContextMenuTrigger>
          <ContextMenuContent>
            <ContextMenuItem
              data-testid="body-menu-format"
              :disabled="busy || !formattable || !draft.body"
              @select="formatBody"
            >
              {{
                draft.bodyMode === "graphql" ? "Format GraphQL" : "Format JSON"
              }}
            </ContextMenuItem>
            <ContextMenuItem
              data-testid="body-menu-clear"
              :disabled="busy || !draft.body"
              @select="clearBody"
            >
              Clear body
            </ContextMenuItem>
            <ContextMenuSeparator />
            <ContextMenuItem
              :disabled="busy"
              @select="!busy && (draft.bodyMode = 'none')"
            >
              Body: none
            </ContextMenuItem>
            <ContextMenuItem
              :disabled="busy"
              @select="!busy && (draft.bodyMode = 'json')"
            >
              Body: JSON
            </ContextMenuItem>
            <ContextMenuItem
              :disabled="busy"
              @select="!busy && (draft.bodyMode = 'text')"
            >
              Body: text
            </ContextMenuItem>
            <ContextMenuItem
              :disabled="busy"
              @select="!busy && (draft.bodyMode = 'graphql')"
            >
              Body: GraphQL
            </ContextMenuItem>
          </ContextMenuContent>
        </ContextMenu>
        <p
          v-if="!bodyAllowed"
          class="px-4 py-3 text-[0.6875rem] leading-[1.7] text-muted-foreground border-b border-border"
        >
          {{ draft.method }} sends no body. Your draft is retained.
        </p>
        <template v-if="draft.bodyMode !== 'none'">
          <label class="sr-only" :for="`${id}-body`">{{
            draft.bodyMode === "graphql" ? "GraphQL query" : "Request body"
          }}</label>
          <textarea
            :id="`${id}-body`"
            v-model="draft.body"
            :disabled="busy"
            class="flex-1 min-h-45 w-full resize-none border-0 rounded-none p-4 font-mono text-[0.8125rem] leading-[1.75] bg-transparent tab-2 pointer-coarse:text-base"
            spellcheck="false"
            autocomplete="off"
            :placeholder="bodyPlaceholder"
            @input="formatError = ''"
            @contextmenu.stop
          />
          <template v-if="draft.bodyMode === 'graphql'">
            <label
              :for="`${id}-variables`"
              class="shrink-0 px-3 py-2 border-y border-border text-muted-foreground text-xs"
            >
              Variables
            </label>
            <textarea
              :id="`${id}-variables`"
              v-model="draft.variables"
              :disabled="busy"
              @input="formatError = ''"
              class="h-32 shrink-0 w-full resize-none border-0 rounded-none p-4 font-mono text-[0.8125rem] leading-[1.75] bg-transparent tab-2 pointer-coarse:text-base"
              spellcheck="false"
              autocomplete="off"
              :placeholder="variablesPlaceholder"
              @contextmenu.stop
            />
          </template>
        </template>
        <p
          v-else
          class="px-4 py-3 text-[0.6875rem] leading-[1.7] text-muted-foreground"
        >
          No request body.
        </p>
        <p
          v-if="formatError"
          role="alert"
          class="px-4 py-3 text-[0.6875rem] leading-[1.7] text-destructive"
        >
          {{ formatError }}
        </p>
      </TabsContent>
      <TabsContent
        value="auth"
        class="flex-1 min-h-0 overflow-auto -outline-offset-2"
      >
        <ContextMenu>
          <ContextMenuTrigger as-child>
            <div
              class="grid grid-cols-[100px_minmax(0,1fr)] gap-3 items-center p-4 text-xs"
              data-auth-actions
            >
              <label :for="`${id}-auth-type`" class="text-muted-foreground">
                Authorization
              </label>
              <select
                :id="`${id}-auth-type`"
                :value="authSelectValue"
                class="h-7 px-2 font-mono text-xs pointer-coarse:min-h-11 pointer-coarse:text-base"
                :disabled="busy"
                @change="
                  setAuthType(($event.target as HTMLSelectElement).value)
                "
                @contextmenu.stop
              >
                <option value="inherit">Inherit</option>
                <option value="none">No auth</option>
                <option value="bearer">Bearer token</option>
                <option value="basic">Basic auth</option>
              </select>
              <template v-if="draft.localAuth === undefined && effectiveAuth">
                <span class="text-muted-foreground text-[0.6875rem]">
                  Effective
                </span>
                <span
                  class="text-muted-foreground text-[0.6875rem] font-mono"
                  data-testid="effective-auth-note"
                >
                  Effective: {{ effectiveAuth.type }}
                </span>
                <template v-if="inheritedSource">
                  <span class="text-muted-foreground text-[0.6875rem]">
                    Source
                  </span>
                  <span
                    class="text-muted-foreground text-[0.6875rem] font-mono"
                  >
                    {{ inheritedSource }}
                  </span>
                </template>
              </template>
              <template v-if="draft.localAuth?.type === 'bearer'">
                <label :for="`${id}-auth-token`" class="text-muted-foreground">
                  Token
                </label>
                <input
                  :id="`${id}-auth-token`"
                  v-model="
                    (draft.localAuth as { type: 'bearer'; token: string }).token
                  "
                  type="password"
                  :disabled="busy"
                  autocomplete="off"
                  spellcheck="false"
                  placeholder="Bearer token"
                  class="h-7.5 min-w-0 px-2 font-mono pointer-coarse:min-h-11 pointer-coarse:text-base"
                  @contextmenu.stop
                />
              </template>
              <template v-if="draft.localAuth?.type === 'basic'">
                <label :for="`${id}-auth-user`" class="text-muted-foreground">
                  Username
                </label>
                <input
                  :id="`${id}-auth-user`"
                  v-model="
                    (
                      draft.localAuth as {
                        type: 'basic';
                        username: string;
                        password: string;
                      }
                    ).username
                  "
                  :disabled="busy"
                  autocomplete="off"
                  spellcheck="false"
                  class="h-7.5 min-w-0 px-2 font-mono pointer-coarse:min-h-11 pointer-coarse:text-base"
                  @contextmenu.stop
                />
                <label
                  :for="`${id}-auth-password`"
                  class="text-muted-foreground"
                >
                  Password
                </label>
                <input
                  :id="`${id}-auth-password`"
                  v-model="
                    (
                      draft.localAuth as {
                        type: 'basic';
                        username: string;
                        password: string;
                      }
                    ).password
                  "
                  type="password"
                  :disabled="busy"
                  autocomplete="off"
                  class="h-7.5 min-w-0 px-2 font-mono pointer-coarse:min-h-11 pointer-coarse:text-base"
                  @contextmenu.stop
                />
              </template>
              <!-- Backward-compat: flat auth fields when no localAuth set and not inherit mode -->
              <template
                v-if="
                  draft.localAuth === undefined && effectiveAuth === undefined
                "
              >
                <!-- legacy flat auth select hidden - use localAuth going forward -->
              </template>
            </div>
          </ContextMenuTrigger>
          <ContextMenuContent>
            <ContextMenuItem :disabled="busy" @select="setAuthType('inherit')">
              Inherit
            </ContextMenuItem>
            <ContextMenuItem :disabled="busy" @select="setAuthType('none')">
              No auth
            </ContextMenuItem>
            <ContextMenuItem :disabled="busy" @select="setAuthType('bearer')">
              Bearer token
            </ContextMenuItem>
            <ContextMenuItem :disabled="busy" @select="setAuthType('basic')">
              Basic auth
            </ContextMenuItem>
            <ContextMenuSeparator />
            <ContextMenuItem
              data-testid="auth-menu-clear-local"
              :disabled="draft.localAuth === undefined"
              @select="setAuthType('inherit')"
            >
              Clear local override
            </ContextMenuItem>
          </ContextMenuContent>
        </ContextMenu>
        <p
          class="px-4 py-3 text-[0.6875rem] leading-[1.7] text-muted-foreground flex items-center gap-2"
        >
          <KeyRound :size="13" aria-hidden="true" />Credentials are saved
          locally in plaintext. Copy cURL includes them.
        </p>
      </TabsContent>
    </TabsRoot>
    <footer
      class="h-7 px-4 border-t border-border flex items-center justify-between font-mono text-[0.5625rem] tracking-[0.12em] text-muted-foreground"
    >
      {{ activePairs(draft.query).length }} QUERY ·
      {{ activePairs(draft.headers).length }} HEADERS
      <span>
        AUTH / {{ (draft.localAuth?.type ?? draft.auth).toUpperCase() }}
      </span>
    </footer>
  </section>
</template>
