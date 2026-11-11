import { cn } from "@/lib/utils";
// The installed app icon, so the window and the dock/taskbar always match.
import appIcon from "../../src-tauri/icons/128x128.png";

/** OpenDictate's mark: the app icon itself. */
export function Mark({ className }: { className?: string }) {
  return (
    <img
      src={appIcon}
      alt=""
      aria-hidden
      draggable={false}
      className={cn("size-7 shrink-0 select-none", className)}
    />
  );
}
