<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  DialogContent,
  DialogOverlay,
  DialogPortal,
  DialogRoot,
  DialogTitle,
} from "reka-ui";
import { Search, X } from "lucide-vue-next";
import { Button } from "@/components/ui/button";
import {
  clearCookies,
  deleteCookie,
  filterCookies,
  listCookies,
  type StoredCookie,
} from "@/lib/cookies";
import { nativeTransport } from "@/lib/transport";

const props = defineProps<{ open: boolean; enabled: boolean }>();
const emit = defineEmits<{ "update:open": [open: boolean] }>();

const cookies = ref<StoredCookie[]>([]);
const query = ref("");
const error = ref("");
const loading = ref(false);
const shown = computed(() => filterCookies(cookies.value, query.value));
const domains = computed(
  () => new Set(cookies.value.map((c) => c.domain)).size,
);

async function load() {
  if (!nativeTransport) return;
  loading.value = true;
  try {
    cookies.value = await listCookies();
    error.value = "";
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    loading.value = false;
  }
}
watch(
  () => props.open,
  (open) => {
    if (open) {
      query.value = "";
      void load();
    }
  },
  { immediate: true },
);
async function run(action: () => Promise<void>) {
  try {
    await action();
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
  await load();
}
const expiry = (cookie: StoredCookie) =>
  cookie.expires === null
    ? "Session"
    : new Date(cookie.expires).toLocaleString([], {
        dateStyle: "short",
        timeStyle: "short",
      });
</script>

<template>
  <DialogRoot :open="open" @update:open="emit('update:open', $event)">
    <DialogPortal>
      <DialogOverlay class="fixed inset-0 z-50 bg-black/50" />
      <DialogContent
        class="fixed z-[60] left-1/2 top-1/2 flex max-h-[80dvh] w-[min(760px,calc(100vw-24px))] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-lg border border-border bg-background"
        :aria-describedby="undefined"
        data-cookies-dialog
      >
        <header
          class="flex shrink-0 items-center justify-between border-b border-border px-4 py-3"
        >
          <DialogTitle class="text-sm font-bold tracking-[0.08em]"
            >Cookies</DialogTitle
          >
          <span
            v-if="nativeTransport"
            class="font-mono text-[0.625rem] tracking-[0.1em] text-muted-foreground"
            >{{ cookies.length }} COOKIES · {{ domains }} DOMAINS</span
          >
        </header>
        <p v-if="!nativeTransport" class="p-4 text-xs text-muted-foreground">
          The browser preview cannot keep cookies. Use the desktop app for the
          cookie jar.
        </p>
        <template v-else>
          <p
            v-if="!enabled"
            class="border-b border-border px-4 py-2 text-[0.6875rem] text-warning"
            role="status"
          >
            Cookie storage is off in Application Settings. Blink does not send
            or keep these cookies.
          </p>
          <div class="border-b border-border px-4 py-2">
            <label
              class="flex items-center gap-1.5 rounded border border-input pl-2 text-muted-foreground focus-within:border-primary"
            >
              <Search :size="13" aria-hidden="true" />
              <span class="sr-only">Filter cookies</span>
              <input
                v-model="query"
                type="search"
                placeholder="Filter by domain, name, or value"
                autocomplete="off"
                spellcheck="false"
                data-cookie-filter
                class="h-7 w-full min-w-0 border-0 bg-transparent px-1.5 font-mono text-[0.6875rem]"
              />
            </label>
          </div>
          <div class="min-h-0 flex-1 overflow-auto">
            <table
              class="w-full table-fixed border-collapse font-mono text-[0.6875rem]"
              aria-label="Cookies"
            >
              <thead class="sticky top-0 bg-muted">
                <tr
                  class="text-left text-[0.625rem] uppercase tracking-widest text-muted-foreground"
                >
                  <th scope="col" class="w-[24%] px-3 py-2 font-medium">
                    Domain
                  </th>
                  <th scope="col" class="w-[18%] px-3 py-2 font-medium">
                    Name
                  </th>
                  <th scope="col" class="px-3 py-2 font-medium">Value</th>
                  <th scope="col" class="w-[18%] px-3 py-2 font-medium">
                    Expires
                  </th>
                  <th class="w-9"><span class="sr-only">Delete</span></th>
                </tr>
              </thead>
              <tbody>
                <tr
                  v-for="cookie in shown"
                  :key="`${cookie.domain} ${cookie.path} ${cookie.name}`"
                  class="border-b border-border align-top"
                  data-cookie
                >
                  <td class="px-3 py-1.5 wrap-anywhere">
                    {{ cookie.domain
                    }}<span
                      v-if="cookie.path !== '/'"
                      class="text-muted-foreground"
                      >{{ cookie.path }}</span
                    >
                  </td>
                  <td class="px-3 py-1.5 wrap-anywhere text-info">
                    {{ cookie.name }}
                  </td>
                  <td class="px-3 py-1.5">
                    <span
                      class="line-clamp-2 break-all"
                      :title="cookie.value"
                      >{{ cookie.value }}</span
                    >
                    <span
                      v-if="cookie.secure || cookie.httpOnly"
                      class="text-[0.5625rem] text-muted-foreground"
                      >{{
                        [
                          cookie.secure && "Secure",
                          cookie.httpOnly && "HttpOnly",
                        ]
                          .filter(Boolean)
                          .join(" · ")
                      }}</span
                    >
                  </td>
                  <td class="px-3 py-1.5 text-muted-foreground">
                    {{ expiry(cookie) }}
                  </td>
                  <td class="p-0.5">
                    <Button
                      variant="ghost"
                      class="size-7 p-0"
                      :aria-label="`Delete cookie ${cookie.name} for ${cookie.domain}`"
                      @click="run(() => deleteCookie(cookie))"
                    >
                      <X :size="13" aria-hidden="true" />
                    </Button>
                  </td>
                </tr>
              </tbody>
            </table>
            <p
              v-if="!loading && !shown.length"
              class="p-4 text-xs text-muted-foreground"
            >
              {{
                cookies.length
                  ? "No cookies match this filter."
                  : "No cookies. Responses that set cookies add them here."
              }}
            </p>
          </div>
          <p
            v-if="error"
            role="alert"
            class="px-4 py-2 text-xs text-destructive"
          >
            {{ error }}
          </p>
        </template>
        <footer
          class="flex shrink-0 justify-between gap-2 border-t border-border px-4 py-3"
        >
          <Button
            variant="secondary"
            data-clear-cookies
            :disabled="!cookies.length"
            @click="run(clearCookies)"
          >
            Clear all
          </Button>
          <Button variant="secondary" @click="emit('update:open', false)">
            Close
          </Button>
        </footer>
      </DialogContent>
    </DialogPortal>
  </DialogRoot>
</template>
