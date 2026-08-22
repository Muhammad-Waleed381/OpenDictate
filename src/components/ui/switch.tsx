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
        "peer relative inline-flex shrink-0 items-center rounded-none border-2 border-border p-0 transition-all duration-150 ease-spring outline-none after:absolute after:-inset-x-3 after:-inset-y-2 focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50",
        size === "default" ? "h-5 w-9" : "h-4 w-7",
        className
      )}
      style={{ backgroundColor: checked ? "#000000" : "#ffffff" }}
      {...props}
      onClick={() => onCheckedChange?.(!checked)}
    >
      <span
        data-slot="switch-thumb"
        className={cn(
          "pointer-events-none block rounded-none border-border transition-transform duration-150 ease-spring",
          "size-3",
          checked
            ? "border-l-2"
            : "border-r-2"
        )}
        style={{
          backgroundColor: checked ? "#ffffff" : "#000000",
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
