import { computed, readonly, ref, type ComputedRef, type Ref } from 'vue'
import { applyTheme, loadTheme, parseTheme, saveTheme, type ThemeSetting } from '@/lib/theme'

export type ThemeState = {
  /** Theme shown now: the saved theme, or an unsaved preview. */
  draft: Readonly<Ref<ThemeSetting | null>>
  /** Why the draft text does not parse, or "". */
  error: Readonly<Ref<string>>
  /** Why the last commit did not reach storage, or "". */
  saveError: Readonly<Ref<string>>
  /** Saved theme name for the status bar. */
  name: ComputedRef<string>
  preview(next: ThemeSetting | null): void
  commit(): string
  revert(): void
  /** Re-read storage and show the saved theme. Used when storage changes
   * outside this state (tests, other windows). */
  reload(): void
}

const storageFailure = 'Theme not saved: local storage is full or disabled.'

export function createThemeState(storage: Storage | null, doc: Document): ThemeState {
  const saved = ref<ThemeSetting | null>(loadTheme(storage))
  const draft = ref<ThemeSetting | null>(saved.value)
  const error = ref('')
  const saveError = ref('')

  function preview(next: ThemeSetting | null) {
    draft.value = next
    saveError.value = ''
    if (!next) {
      error.value = ''
      applyTheme(doc, null)
      return
    }
    const parsed = parseTheme(next)
    if (!parsed.ok) {
      error.value = parsed.error
      return
    }
    error.value = ''
    applyTheme(doc, parsed.palette, next.accent)
  }

  function commit() {
    if (error.value) return error.value
    if (JSON.stringify(draft.value) === JSON.stringify(saved.value)) return ''
    saved.value = draft.value
    try {
      if (!storage) throw new Error('no storage')
      saveTheme(storage, draft.value)
      saveError.value = ''
    } catch {
      saveError.value = storageFailure
    }
    return saveError.value
  }

  function revert() {
    preview(saved.value)
  }

  function reload() {
    saved.value = loadTheme(storage)
    revert()
  }

  revert()
  return {
    draft: readonly(draft) as Readonly<Ref<ThemeSetting | null>>,
    error: readonly(error),
    saveError: readonly(saveError),
    name: computed(() => saved.value?.name ?? 'Blink'),
    preview,
    commit,
    revert,
    reload,
  }
}

function localStore(): Storage | null {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage
  } catch {
    // Some webviews throw on access when storage is disabled.
    return null
  }
}

let shared: ThemeState | undefined

/** The app theme. Call once before mount so the stored theme paints first. */
export function useTheme(): ThemeState {
  return (shared ??= createThemeState(localStore(), document))
}
