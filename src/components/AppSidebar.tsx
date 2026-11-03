import { useEffect } from "react";
import type { LucideIcon } from "lucide-react";
import { Activity, BookOpen, Cpu, FileText, History, House, PanelLeftClose, PanelLeftOpen, Settings } from "lucide-react";
import { useStore } from "@/lib/store";
import { cn } from "@/lib/utils";
import { Mark } from "@/components/Mark";

export type TabId = "home" | "activity" | "dictionary" | "snippets" | "history" | "models" | "settings";

interface NavItem {
  id: TabId;
  label: string;
  icon: LucideIcon;
}

/** Grouped the way people use the app: dictate, tune the words, set it up. */
export const NAV_GROUPS: { label: string; items: NavItem[] }[] = [
  {
    label: "Dictation",
    items: [
      { id: "home", label: "Home", icon: House },
      { id: "history", label: "History", icon: History },
      { id: "activity", label: "Activity", icon: Activity },
    ],
  },
  {
    label: "Words",
    items: [
      { id: "dictionary", label: "Dictionary", icon: BookOpen },
      { id: "snippets", label: "Snippets", icon: FileText },
    ],
  },
  {
    label: "Setup",
    items: [
      { id: "models", label: "Models", icon: Cpu },
      { id: "settings", label: "Settings", icon: Settings },
    ],
  },
];

export const NAV_ITEMS = NAV_GROUPS.flatMap((group) => group.items);

/** Shown beside a link; History loads one page, so a full page reads "500+". */
function useCounts(): Partial<Record<TabId, string>> {
  const history = useStore((s) => s.history.length);
  const dictionary = useStore((s) => s.dictionary.length);
  const snippets = useStore((s) => s.snippets.length);
  const show = (n: number, cap?: number) => (n === 0 ? undefined : cap && n >= cap ? `${cap}+` : String(n));
  return { history: show(history, 500), dictionary: show(dictionary), snippets: show(snippets) };
}

function StatusBlock({ collapsed, onOpenModels }: { collapsed: boolean; onOpenModels: () => void }) {
  const recording = useStore((s) => s.recording);
  const ready = useStore((s) => s.models?.stt_ready ?? false);
  const hydrated = useStore((s) => s.hydrated);
  const modelName = useStore((s) => s.catalog.find((m) => m.id === s.settings?.stt_model)?.name);

  const state = recording ? "Recording" : !hydrated ? "Starting" : ready ? "Ready" : "Model needed";
  const dot = recording
    ? "animate-od-blink bg-record-live"
    : !hydrated
      ? "bg-input"
      : ready
        ? "bg-positive"
        : "bg-notice";
  const detail = modelName ?? "No model selected";

  return (
    <button
      type="button"
      onClick={onOpenModels}
      title={collapsed ? `${state}: ${detail}` : "Open Models"}
      aria-label={`${state}, ${detail}. Open Models`}
      className={cn(
        "flex w-full cursor-pointer items-center rounded-lg text-left transition-colors duration-150 hover:bg-sidebar-accent focus-visible:outline-2 focus-visible:outline-ring",
        collapsed ? "size-10 justify-center self-center" : "gap-2.5 px-3 py-2",
      )}
    >
      <span className={cn("size-2 shrink-0 rounded-full", dot)} aria-hidden />
      {!collapsed && (
        <span className="flex min-w-0 flex-col">
          <span className="text-[13px] font-medium text-sidebar-foreground">{state}</span>
          <span className="truncate text-xs text-muted-foreground">{detail}</span>
        </span>
      )}
    </button>
  );
}

export function AppSidebar({
  tab,
  onSelect,
  collapsed,
  onToggle,
}: {
  tab: TabId;
  onSelect: (tab: TabId) => void;
  collapsed: boolean;
  onToggle: () => void;
}) {
  const counts = useCounts();

  // Ctrl+B (Cmd+B on a Mac) folds the sidebar, as in most desktop apps.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && !event.altKey && !event.shiftKey && event.key.toLowerCase() === "b") {
        event.preventDefault();
        onToggle();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onToggle]);

  return (
    <aside
      aria-label="Sidebar"
      className={cn(
        "flex shrink-0 flex-col border-r border-sidebar-border bg-sidebar transition-[width] duration-200 ease-(--ease-fluid)",
        collapsed ? "w-15" : "w-52",
      )}
    >
      <div className={cn("flex h-14 shrink-0 items-center gap-2.5", collapsed ? "justify-center" : "px-4")}>
        <Mark />
        {!collapsed && (
          <span className="flex min-w-0 flex-col leading-tight">
            <span className="truncate text-sm font-semibold text-sidebar-foreground">OpenDictate</span>
            <span className="truncate text-xs text-muted-foreground">Runs on this device</span>
          </span>
        )}
      </div>

      <nav className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-2.5 pt-2">
        {NAV_GROUPS.map((group, index) => (
          <div key={group.label} role="group" aria-label={group.label} className="flex flex-col gap-0.5">
            {collapsed ? (
              index > 0 && <span className="mx-auto mb-1 h-px w-5 bg-sidebar-border" aria-hidden />
            ) : (
              <span className="px-2.5 pb-1 text-xs font-medium text-muted-foreground" aria-hidden>
                {group.label}
              </span>
            )}
            {group.items.map((item) => {
              const active = tab === item.id;
              const count = counts[item.id];
              return (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => onSelect(item.id)}
                  title={collapsed ? item.label : undefined}
                  aria-current={active ? "page" : undefined}
                  className={cn(
                    "flex h-8 cursor-pointer items-center rounded-md text-[13px] transition-[background-color,color,box-shadow] duration-150 focus-visible:outline-2 focus-visible:outline-ring",
                    collapsed ? "mx-auto w-9 justify-center" : "gap-2.5 px-2.5",
                    active
                      ? "bg-sidebar-accent font-medium text-sidebar-foreground shadow-[0_1px_2px_rgb(26_18_64/0.08)] dark:shadow-none"
                      : "text-muted-foreground hover:bg-sidebar-accent/70 hover:text-sidebar-foreground",
                  )}
                >
                  <item.icon className={cn("size-4 shrink-0", active && "text-brand")} strokeWidth={1.75} />
                  {!collapsed && <span className="min-w-0 flex-1 truncate text-left">{item.label}</span>}
                  {!collapsed && count && (
                    <span className="text-xs text-muted-foreground tabular-nums" aria-hidden>
                      {count}
                    </span>
                  )}
                </button>
              );
            })}
          </div>
        ))}
      </nav>

      <div className={cn("flex shrink-0 flex-col gap-1 border-t border-sidebar-border p-2.5", collapsed && "items-center")}>
        <StatusBlock collapsed={collapsed} onOpenModels={() => onSelect("models")} />
        <div className={cn("flex items-center", collapsed ? "justify-center" : "justify-between pl-3")}>
          {!collapsed && <span className="text-xs text-muted-foreground tabular-nums">Version {__APP_VERSION__}</span>}
          <button
            type="button"
            onClick={onToggle}
            title={collapsed ? "Expand sidebar (Ctrl+B)" : "Collapse sidebar (Ctrl+B)"}
            aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
            className="flex size-8 shrink-0 cursor-pointer items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-sidebar-accent hover:text-sidebar-foreground focus-visible:outline-2 focus-visible:outline-ring"
          >
            {collapsed ? <PanelLeftOpen className="size-4" /> : <PanelLeftClose className="size-4" />}
          </button>
        </div>
      </div>
    </aside>
  );
}
