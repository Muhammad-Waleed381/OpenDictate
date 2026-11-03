import { create } from "zustand";

type ThemePref = "system" | "light" | "dark";

interface ThemeStore {
  pref: ThemePref;
  setPref: (pref: ThemePref) => void;
}

// A new key: the white theme is the default look, so installs that had saved
// "system" under the old key open light once. Dark and System stay available.
const STORAGE_KEY = "od:theme-v2";

function loadPref(): ThemePref {
  const value = localStorage.getItem(STORAGE_KEY);
  return value === "light" || value === "dark" || value === "system"
    ? value
    : "light";
}

export const useTheme = create<ThemeStore>()((set) => ({
  pref: loadPref(),
  setPref: (pref) => {
    localStorage.setItem(STORAGE_KEY, pref);
    applyTheme(pref);
    set({ pref });
  },
}));

function systemDark(): boolean {
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

export function applyTheme(pref: ThemePref): void {
  const root = document.documentElement;
  root.classList.toggle("dark", pref === "dark" || (pref === "system" && systemDark()));
}

export function initTheme(): void {
  applyTheme(useTheme.getState().pref);
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    if (useTheme.getState().pref === "system") applyTheme("system");
  });
}
