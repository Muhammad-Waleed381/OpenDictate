import { cn } from "@/lib/utils"
import type { ButtonHTMLAttributes } from "react"

function Switch({
  className,
  checked = false,
  onCheckedChange,
  size = "default",
  ...props
}: Omit<ButtonHTMLAttributes<HTMLButtonElement>, "onChange"> & {
  checked?: boolean
  onCheckedChange?: (checked: boolean) => void
  size?: "sm" | "default"
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      data-slot="switch"
      data-size={size}
      className={cn(
        "peer relative inline-flex shrink-0 items-center rounded-full border-0 p-0 transition-colors duration-150 ease-fluid outline-none after:absolute after:-inset-x-3 after:-inset-y-2 focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50",
        size === "default" ? "h-5 w-9" : "h-4 w-7",
        className
      )}
      style={{ backgroundColor: checked ? "var(--brand)" : "var(--input)" }}
      {...props}
      onClick={() => onCheckedChange?.(!checked)}
    >
      <span
        data-slot="switch-thumb"
        className={cn(
          "pointer-events-none block rounded-full transition-transform duration-150 ease-fluid",
          "size-3.5"
        )}
        style={{
          backgroundColor: "#ffffff",
          transform: checked
            ? size === "default"
              ? "translateX(calc(100% + 8px))"
              : "translateX(100%)"
            : "translateX(0)",
        }}
      />
    </button>
  )
}

export { Switch }
