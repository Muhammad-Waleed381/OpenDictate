import "@testing-library/jest-dom/vitest";
import { afterEach, vi } from "vitest";
import { cleanup } from "@testing-library/react";

afterEach(() => cleanup());
Object.defineProperty(HTMLElement.prototype, "scrollTo", { value: vi.fn(), configurable: true });
Object.defineProperty(window, "matchMedia", {
  value: (query: string) => ({
    matches: false, media: query, addEventListener: vi.fn(), removeEventListener: vi.fn(),
    addListener: vi.fn(), removeListener: vi.fn(), dispatchEvent: vi.fn(),
  }),
});
vi.stubGlobal("ResizeObserver", class {
  observe() {}
  unobserve() {}
  disconnect() {}
});

// jsdom has no IntersectionObserver. Tests call scrollListEnd() to report every
// observed element as visible, as scrolling to the end of a list would.
const observers = new Set<{ fire: () => void }>();
vi.stubGlobal("IntersectionObserver", class {
  private targets: Element[] = [];
  private entry = { fire: () => this.callback(this.targets.map((target) => ({ target, isIntersecting: true }))) };
  constructor(private callback: (entries: { target: Element; isIntersecting: boolean }[]) => void) {}
  observe(target: Element) { this.targets.push(target); observers.add(this.entry); }
  unobserve() {}
  disconnect() { observers.delete(this.entry); }
});
(globalThis as { scrollListEnd?: () => void }).scrollListEnd = () => [...observers].forEach((o) => o.fire());
