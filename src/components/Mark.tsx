import { cn } from "@/lib/utils";

/** OpenDictate's mark: a voice level on a violet tile. */
export function Mark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" className={cn("size-6 shrink-0", className)} aria-hidden>
      <rect width="24" height="24" rx="6.5" fill="#562FF7" />
      <g fill="#EFECF0">
        <rect x="5" y="10" width="2.4" height="4" rx="1.2" />
        <rect x="9.2" y="6.5" width="2.4" height="11" rx="1.2" />
        <rect x="13.4" y="8.5" width="2.4" height="7" rx="1.2" />
      </g>
      <circle cx="18.6" cy="12" r="1.5" fill="#F354D0" />
    </svg>
  );
}
