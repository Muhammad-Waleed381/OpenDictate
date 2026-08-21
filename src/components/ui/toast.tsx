import { create } from "zustand";
import { cn } from "@/lib/utils";

type ToastKind = "success" | "error" | "info";

interface ToastItem {
  id: number;
  kind: ToastKind;
  message: string;
}

interface ToastStore {
  items: ToastItem[];
  push: (kind: ToastKind, message: string) => void;
  dismiss: (id: number) => void;
}

let nextId = 1;

export const useToastStore = create<ToastStore>()((set) => ({
  items: [],
  push: (kind, message) => {
    const id = nextId++;
    set((state) => ({ items: [...state.items.slice(-4), { id, kind, message }] }));
    const ttl = kind === "error" ? 6000 : 4000;
    setTimeout(() => set((state) => ({ items: state.items.filter((t) => t.id !== id) })), ttl);
  },
  dismiss: (id) =>
    set((state) => ({ items: state.items.filter((t) => t.id !== id) })),
}));

export const toast = {
  success: (message: string) => useToastStore.getState().push("success", message),
  error: (message: string) => useToastStore.getState().push("error", message),
  info: (message: string) => useToastStore.getState().push("info", message),
};

const KIND_STYLES: Record<ToastKind, string> = {
  success: "bg-card text-card-foreground",
  error: "bg-destructive text-destructive-foreground",
  info: "bg-primary text-primary-foreground",
};

const KIND_MARK: Record<ToastKind, string> = {
  success: "✓",
  error: "✕",
  info: "›",
};

function ToastRow({ item }: { item: ToastItem }) {
  return (
    <button
      onClick={() => useToastStore.getState().dismiss(item.id)}
      className={cn(
        "pointer-events-auto flex w-full max-w-sm cursor-pointer items-start gap-2 rounded-md border-2 border-border px-3 py-2 text-left text-xs font-bold uppercase tracking-wide shadow-brutal animate-od-slide-up",
        KIND_STYLES[item.kind],
      )}
      role="status"
    >
      <span aria-hidden>{KIND_MARK[item.kind]}</span>
      <span className="min-w-0 break-words">{item.message}</span>
    </button>
  );
}

export function Toaster() {
  const items = useToastStore((s) => s.items);
  return (
    <div className="pointer-events-none fixed right-4 bottom-4 z-50 flex flex-col gap-2">
      {items.map((item) => (
        <ToastRow key={item.id} item={item} />
      ))}
    </div>
  );
}
