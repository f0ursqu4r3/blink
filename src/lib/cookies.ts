import { invoke } from "@tauri-apps/api/core";

export type StoredCookie = {
  domain: string;
  path: string;
  name: string;
  value: string;
  /** Epoch milliseconds; null for a session cookie. */
  expires: number | null;
  secure: boolean;
  httpOnly: boolean;
};

export const listCookies = () => invoke<StoredCookie[]>("list_cookies");
export const deleteCookie = ({ domain, path, name }: StoredCookie) =>
  invoke<void>("delete_cookie", { domain, path, name });
export const clearCookies = () => invoke<void>("clear_cookies");

/** Cookies whose domain, name, or value contains `query`, without case. */
export function filterCookies(cookies: StoredCookie[], query: string) {
  const needle = query.trim().toLowerCase();
  if (!needle) return cookies;
  return cookies.filter((cookie) =>
    [cookie.domain, cookie.name, cookie.value].some((text) =>
      text.toLowerCase().includes(needle),
    ),
  );
}
