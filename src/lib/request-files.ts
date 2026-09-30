import { invoke, isTauri } from '@tauri-apps/api/core'

export type PickedFile = { path: string; name: string; sizeBytes: number }

export const FILES_UNAVAILABLE = 'Browser preview cannot send files. Use the desktop app.'

/**
 * Open the file dialog. Only a picked file can be sent: the desktop app
 * refuses paths the user did not choose. Resolves null on cancel.
 */
export async function pickRequestFile(): Promise<PickedFile | null> {
  if (!isTauri()) throw new Error(FILES_UNAVAILABLE)
  return invoke<PickedFile | null>('pick_request_file')
}
